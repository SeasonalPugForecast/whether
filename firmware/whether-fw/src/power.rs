//! Deep sleep between refreshes (`device.power = "deep-sleep"`): one wake
//! joins Wi-Fi, fetches what is due and sleeps until the next due time. What the
//! next wake needs (`fw_sleep::RtcState`) is kept in RTC fast memory, and the
//! wall clock in the RTC timer, which both run through deep sleep.

use embassy_futures::select::{Either, select};
use embassy_time::Timer;
use esp_hal::rtc_cntl::sleep::{LowPower, RtcSleepConfig};
use esp_hal::rtc_cntl::{Rtc, SocResetReason, reset_reason};
use esp_hal::system::Cpu;
use esp_hal::time::{Duration, Instant};
use fw_sleep::RtcState;
use log::{info, warn};
use whether_core::time::UnixTime;

use crate::fetch::{self, Cycle};
use crate::net::clock;
use crate::net::http::EspTransport;

/// Bytes of RTC fast memory for the state (it needs about 100).
const RTC_BYTES: usize = 512;
/// The shortest sleep: a wake costs seconds of radio time.
const MIN_SLEEP_S: u32 = 30;
/// The longest sleep: the RTC slow clock drifts, so a response's `Date` header
/// corrects it at least this often.
const MAX_SLEEP_S: u32 = 3600;
/// How long a wake waits for Wi-Fi and an address before giving up.
const NETWORK_TIMEOUT_S: u64 = 30;

/// The kept state. Zeroed after a power-on, kept through deep sleep;
/// `RtcState::decode` rejects anything it did not write.
#[esp_hal::ram(unstable(rtc_fast, persistent))]
static mut RTC_STATE: [u8; RTC_BYTES] = [0; RTC_BYTES];

/// The state the previous run left, or `None` after anything but a deep-sleep
/// wake (power-on, reset, flash).
pub fn restore() -> Option<RtcState> {
    if reset_reason(Cpu::ProCpu) != Some(SocResetReason::CoreDeepSleep) {
        return None;
    }
    // SAFETY: only `restore` and `sleep` touch RTC_STATE, both from the main
    // task before or after the other tasks run; the copy reads plain bytes.
    #[allow(
        unsafe_code,
        reason = "a copy of a `static mut` placed in RTC memory by esp-hal"
    )]
    let bytes = unsafe { core::ptr::read(&raw const RTC_STATE) };
    let state = RtcState::decode(&bytes);
    if state.is_none() {
        warn!("woke from deep sleep without a valid kept state");
    }
    state
}

/// Restores the wall clock from the RTC timer when the previous run had set it.
pub fn restore_clock(rtc: &Rtc<'_>, state: Option<&RtcState>) {
    if state.is_some_and(|s| s.clock_set) {
        let secs = rtc.current_time_us() / 1_000_000;
        clock::set(UnixTime(i64::try_from(secs).unwrap_or(0)));
    }
}

/// One wake's work: wait for the network, check the TLS roots after a cold boot,
/// fetch what is due. Returns the kept state for the next wake.
pub async fn wake(
    stack: embassy_net::Stack<'static>,
    transport: &mut EspTransport,
    cycle: &mut Cycle,
    previous: Option<&RtcState>,
) -> RtcState {
    let online = select(stack.wait_config_up(), Timer::after_secs(NETWORK_TIMEOUT_S)).await;
    fetch::publish_link(stack);
    if let Either::First(()) = online {
        if previous.is_none() {
            fetch::check_tls(transport).await;
        }
        cycle.fetch_due(transport).await;
    } else {
        warn!("no network after {NETWORK_TIMEOUT_S} s");
        cycle.fail_due();
    }
    let (ledger, stored) = cycle.budget_state();
    RtcState {
        wakes: previous.map_or(0, |s| s.wakes.saturating_add(1)),
        clock_set: clock::now().is_some(),
        ledger,
        stored,
        scheduler: Some(cycle.scheduler().clone()),
        wifi: crate::net::wifi::last_ap().or_else(|| previous.and_then(|s| s.wifi)),
        view_hash: previous.map_or(0, |s| s.view_hash),
    }
}

/// Keeps `state`, hands the clock to the RTC timer and sleeps until the next due
/// time.
pub fn sleep(rtc: &Rtc<'_>, mut low_power: LowPower<'_>, state: &RtcState) -> ! {
    if let Some(now) = clock::now() {
        rtc.set_current_time_us(u64::try_from(now.0).unwrap_or(0) * 1_000_000);
    }
    let mut bytes = [0u8; RTC_BYTES];
    match state.encode(&mut bytes) {
        Ok(len) => info!("keeping {len} bytes of state across the sleep"),
        Err(e) => warn!("state not kept ({e:?}); the next wake starts cold"),
    }
    // SAFETY: as in `restore`; nothing else runs code that touches RTC_STATE.
    #[allow(
        unsafe_code,
        reason = "a write to a `static mut` placed in RTC memory by esp-hal"
    )]
    unsafe {
        core::ptr::write(&raw mut RTC_STATE, bytes);
    }
    let seconds = state.scheduler.as_ref().map_or(MAX_SLEEP_S, |s| {
        fw_sleep::sleep_secs(s, fetch::now(), MIN_SLEEP_S, MAX_SLEEP_S)
    });
    info!("wake {} done; sleeping {seconds} s", state.wakes);
    low_power.set_wakeup_deadline(Instant::now() + Duration::from_secs(u64::from(seconds)));
    // `deep()` would power RTC fast memory down with everything else; keep it.
    let mut config = RtcSleepConfig::deep();
    config.set_rtc_fastmem_pd_en(false);
    low_power.sleep_deep(config)
}
