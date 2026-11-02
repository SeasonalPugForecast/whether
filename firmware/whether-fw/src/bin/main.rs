#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::timer::timg::TimerGroup;
use fw_config::FwPower;
use log::{error, info};
use static_cell::StaticCell;
use whether_fw::fetch::{Cycle, Fetcher};
use whether_fw::ledger::LedgerStore;
use whether_fw::net::http::EspTransport;
use whether_fw::net::wifi;
use whether_fw::power;

extern crate alloc;

/// The configuration validated and embedded by build.rs (postcard).
static CONFIG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/config.bin"));
/// The decoded configuration, shared with the tasks.
static CONFIG_CELL: StaticCell<fw_config::FwRuntimeConfig> = StaticCell::new();

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[allow(
    clippy::default_trait_access,
    reason = "the Wi-Fi setup is esp-generate's code, kept as generated"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.4.0
    // generator parameters: -o esp32 -o esp32-wroom-32 -o unstable-hal -o alloc -o wifi -o embassy -o log -o esp-backtrace

    whether_fw::diag::paint();
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO2
    // - GPIO5
    // - GPIO12
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _gpio6 = peripherals.GPIO6;
    let _gpio7 = peripherals.GPIO7;
    let _gpio8 = peripherals.GPIO8;
    let _gpio9 = peripherals.GPIO9;
    let _gpio10 = peripherals.GPIO10;
    let _gpio11 = peripherals.GPIO11;
    let _gpio16 = peripherals.GPIO16;
    let _gpio20 = peripherals.GPIO20;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);
    // A second region from ordinary RAM: TLS plus the Wi-Fi driver peaked at
    // 86.7 KB of the first (81.5 KB of 115 KB in the deep-sleep cycle). Only
    // 14 KB, because the main stack needs at least 56 KB (certificate parsing
    // overflowed 43.5 KB; see .cargo/esp-config.toml) and static data grows: at
    // 16 KB the build had 36 bytes to spare (2026-08-04).
    esp_alloc::heap_allocator!(size: 14 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!("Embassy initialized!");

    // INVARIANT: build.rs encoded this blob from the same fw-config version.
    let cfg = fw_config::FwRuntimeConfig::decode(CONFIG).expect("embedded configuration decodes");
    info!(
        "whether-fw {} board={} chip=esp32 display={:?} rotation={} refresh={}s",
        env!("CARGO_PKG_VERSION"),
        cfg.device.board,
        cfg.device.display,
        cfg.device.rotation,
        cfg.refresh_s,
    );
    for (i, loc) in cfg.locations.iter().enumerate() {
        info!("location {}: {}", i + 1, loc.name);
    }
    info!(
        "providers {:?}; heap used {} free {}",
        cfg.providers
            .iter()
            .map(fw_config::FwProvider::id)
            .collect::<alloc::vec::Vec<_>>(),
        esp_alloc::HEAP.used(),
        esp_alloc::HEAP.free(),
    );

    let cfg: &'static fw_config::FwRuntimeConfig = CONFIG_CELL.init(cfg);
    whether_fw::state::init(cfg.locations.iter().map(|l| l.name.clone()));
    let deep_sleep = cfg.device.power == FwPower::DeepSleep;
    let rtc = Rtc::new(peripherals.RTC_TIMER);
    let kept = if deep_sleep { power::restore() } else { None };
    if deep_sleep {
        info!(
            "power: deep sleep; {}",
            kept.as_ref().map_or_else(
                || "cold boot".into(),
                |s| alloc::format!("wake {} after a deep sleep", s.wakes + 1)
            )
        );
        power::restore_clock(&rtc, kept.as_ref());
    }
    let store = match LedgerStore::open(peripherals.FLASH) {
        Ok(store) => Some(store),
        Err(e) => {
            error!("call ledger unavailable: {e:?}");
            None
        }
    };

    // The RNG is truly random only while the radio runs, so it is read after
    // Wi-Fi has started.
    let rng = Rng::new();
    let stack = match wifi::start(
        spawner,
        peripherals.WIFI,
        &cfg.wifi.ssid,
        &cfg.wifi.password,
        u64::from(rng.random()) << 32 | u64::from(rng.random()),
        kept.as_ref().and_then(|s| s.wifi),
    ) {
        Ok(stack) => stack,
        Err(e) => {
            error!("network not started: {e:?}");
            loop {
                Timer::after_secs(3600).await;
            }
        }
    };
    info!("joining Wi-Fi network {:?}", cfg.wifi.ssid);
    let mut seed = [0u8; 32];
    rng.read(&mut seed);
    let mut transport =
        EspTransport::new(stack, cfg.tls.roots.clone(), cfg.user_agent.clone(), seed);
    let jitter_seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    let mut cycle = Cycle::new(cfg, store, kept.as_ref(), jitter_seed).await;
    if deep_sleep {
        let state = power::wake(stack, &mut transport, &mut cycle, kept.as_ref()).await;
        power::sleep(&rtc, LowPower::new(peripherals.LPWR), &state);
    }
    let fetcher = Fetcher {
        stack,
        transport,
        cycle,
    };
    match whether_fw::fetch::run(fetcher) {
        Ok(token) => spawner.spawn(token),
        Err(e) => error!("fetcher not started: {e:?}"),
    }

    // The displays arrive later; until then, log the link changes
    // they would show (the fetcher logs the data).
    let mut link = whether_fw::state::LinkState::Down;
    loop {
        whether_fw::state::CHANGED.wait().await;
        let now = whether_fw::state::read(|s| s.link);
        if now != link {
            info!("network: {now:?}");
            link = now;
        }
    }
}
