#![no_std]
//! What the firmware keeps across deep sleep, and the decisions around it
//!. Pure, so the host tests it; the firmware moves the bytes to and from
//! RTC memory, which survives deep sleep but not a power loss and is not zeroed
//! reliably, hence the magic and the checksum.

extern crate alloc;

use serde::{Deserialize, Serialize};
use whether_core::model::ProviderId;
use whether_core::time::UnixTime;
use whether_providers::schedule::Scheduler;
use whether_providers::{Reservation, UsageLedger};

/// Marks a buffer that holds an [`RtcState`] (`WhSl`).
const MAGIC: u32 = 0x5768_536C;
/// Bumped when [`RtcState`] changes shape (2: the ledger keeps a period per provider); older data is then ignored.
const VERSION: u16 = 2;
/// Magic, version, length and CRC-32.
const HEADER: usize = 4 + 2 + 2 + 4;

/// The access point a wake should join directly, skipping the scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiHint {
    /// The channel.
    pub channel: u8,
    /// The access point's MAC address.
    pub bssid: [u8; 6],
}

/// Everything a wake needs from the previous one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtcState {
    /// Wakes since the last cold boot.
    pub wakes: u32,
    /// The RTC clock was set from a server's `Date` header.
    pub clock_set: bool,
    /// The exact One Call count. Resuming from the flash reservation on every
    /// wake would count a whole block of calls each time.
    pub ledger: Option<UsageLedger>,
    /// The reservation last stored in flash.
    pub stored: Option<Reservation>,
    /// Due times and failure counts.
    pub scheduler: Option<Scheduler>,
    /// The access point last joined.
    pub wifi: Option<WifiHint>,
    /// A hash of what the display shows, to skip redrawing an unchanged view.
    pub view_hash: u64,
}

/// Why a state could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// The buffer is too small.
    TooLarge,
}

/// CRC-32 (IEEE 802.3, reflected).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

impl RtcState {
    /// Writes the state into `buf` with a header; returns the bytes used.
    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, EncodeError> {
        let (head, body) = buf
            .split_at_mut_checked(HEADER)
            .ok_or(EncodeError::TooLarge)?;
        let len = postcard::to_slice(self, body)
            .map_err(|_| EncodeError::TooLarge)?
            .len();
        let len16 = u16::try_from(len).map_err(|_| EncodeError::TooLarge)?;
        head[..4].copy_from_slice(&MAGIC.to_le_bytes());
        head[4..6].copy_from_slice(&VERSION.to_le_bytes());
        head[6..8].copy_from_slice(&len16.to_le_bytes());
        head[8..12].copy_from_slice(&crc32(&body[..len]).to_le_bytes());
        Ok(HEADER + len)
    }

    /// Reads a state written by [`Self::encode`]; `None` for anything else
    /// (a cold boot's random bytes, another version, a torn write).
    pub fn decode(buf: &[u8]) -> Option<Self> {
        let head = buf.get(..HEADER)?;
        let word =
            |at: usize| u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
        let half = |at: usize| u16::from_le_bytes([head[at], head[at + 1]]);
        if word(0) != MAGIC || half(4) != VERSION {
            return None;
        }
        let body = buf.get(HEADER..HEADER + usize::from(half(6)))?;
        if crc32(body) != word(8) {
            return None;
        }
        postcard::from_bytes(body).ok()
    }
}

/// The ledger for this run. After a deep-sleep wake with a valid state, the
/// exact count carries on. After a cold boot (or a lost state) every call of the
/// stored reservation counts as used, so a power loss can waste calls but never
/// add any.
pub fn ledger_for(
    provider: ProviderId,
    rtc: Option<&RtcState>,
    flash: Option<Reservation>,
) -> UsageLedger {
    if let Some(ledger) = rtc.and_then(|s| s.ledger.as_ref()) {
        return ledger.clone();
    }
    flash.map_or_else(UsageLedger::new, |r| {
        UsageLedger::from_reservation(provider, r)
    })
}

/// How long to deep-sleep at `now`: until the scheduler's next due time, at
/// least `min_s` (a wake costs seconds of radio time) and at most `max_s` (the
/// RTC clock drifts, so a long sleep is split).
pub fn sleep_secs(scheduler: &Scheduler, now: UnixTime, min_s: u32, max_s: u32) -> u32 {
    let wait = scheduler
        .next_wakeup(now)
        .map_or(i64::from(max_s), |t| t.0 - now.0);
    let wait = u32::try_from(wait.max(0)).unwrap_or(max_s);
    wait.clamp(min_s.max(1), max_s.max(min_s.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixTime = UnixTime(1_791_474_885);
    const OWM: ProviderId = ProviderId::OpenWeather;

    fn state() -> RtcState {
        let mut ledger = UsageLedger::new();
        ledger
            .try_spend(OWM, whether_providers::Budget::daily(300), 1, NOW)
            .unwrap();
        let mut scheduler = Scheduler::new(1, 1200, &[]);
        scheduler.on_success(0, NOW, None, 0.0);
        RtcState {
            wakes: 3,
            clock_set: true,
            ledger: Some(ledger),
            stored: Some(Reservation {
                day: 20_734,
                reserved: 10,
            }),
            scheduler: Some(scheduler),
            wifi: Some(WifiHint {
                channel: 10,
                bssid: [1, 2, 3, 4, 5, 6],
            }),
            view_hash: 42,
        }
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn states_round_trip_and_fit_in_rtc_memory() {
        let mut buf = [0u8; 512];
        let len = state().encode(&mut buf).unwrap();
        assert!(len < 256, "{len} bytes");
        assert_eq!(RtcState::decode(&buf), Some(state()));
        let mut small = [0u8; 16];
        assert_eq!(state().encode(&mut small), Err(EncodeError::TooLarge));
    }

    #[test]
    fn foreign_or_damaged_bytes_are_ignored() {
        let mut buf = [0u8; 512];
        state().encode(&mut buf).unwrap();
        assert_eq!(RtcState::decode(&[0u8; 512]), None, "zeroed (cold boot)");
        assert_eq!(RtcState::decode(&[0xA5; 512]), None, "random");
        let mut flipped = buf;
        flipped[HEADER + 3] ^= 1;
        assert_eq!(RtcState::decode(&flipped), None, "a flipped bit");
        let mut other = buf;
        other[4] = 9;
        assert_eq!(RtcState::decode(&other), None, "another version");
        let mut long = buf;
        long[6] = 0xFF;
        long[7] = 0xFF;
        assert_eq!(RtcState::decode(&long), None, "a length past the buffer");
    }

    #[test]
    fn a_wake_keeps_the_exact_count_and_a_cold_boot_resumes_from_flash() {
        let s = state();
        let flash = s.stored;
        // A wake: one call used, not the ten reserved.
        assert_eq!(ledger_for(OWM, Some(&s), flash).used(OWM, NOW), 1);
        // A cold boot: the whole reservation counts.
        assert_eq!(ledger_for(OWM, None, flash).used(OWM, NOW), 10);
        // A wake whose state has no ledger yet also falls back to flash.
        let empty = RtcState::default();
        assert_eq!(ledger_for(OWM, Some(&empty), flash).used(OWM, NOW), 10);
        assert_eq!(ledger_for(OWM, None, None).used(OWM, NOW), 0);
    }

    #[test]
    fn sleeps_last_until_the_next_due_time_within_bounds() {
        let s = state().scheduler.unwrap();
        assert_eq!(sleep_secs(&s, NOW, 30, 3600), 1200);
        assert_eq!(
            sleep_secs(&s, UnixTime(NOW.0 + 1190), 30, 3600),
            30,
            "never shorter than min"
        );
        assert_eq!(
            sleep_secs(&s, UnixTime(NOW.0 + 5000), 30, 3600),
            30,
            "overdue"
        );
        assert_eq!(sleep_secs(&s, NOW, 30, 600), 600, "a long sleep is split");
        assert_eq!(sleep_secs(&s, NOW, 0, 0), 1, "never zero");
        let none = Scheduler::new(0, 1200, &[]);
        assert_eq!(sleep_secs(&none, NOW, 30, 3600), 3600);
    }
}
