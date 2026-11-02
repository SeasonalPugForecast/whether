//! When each location is due: refresh intervals with ±10 % jitter, backoff after
//! failures and a floor for manual refreshes. Pure (every method takes `now`), so
//! the Linux runtime and the firmware share it.

use alloc::vec::Vec;

use whether_core::time::UnixTime;

/// First retry delay after a failure.
pub const BACKOFF_START_S: u32 = 30;
/// Longest retry delay.
pub const BACKOFF_MAX_S: u32 = 30 * 60;
/// Manual refreshes closer together than this are ignored.
pub const MANUAL_FLOOR_S: i64 = 60;
/// Scheduled intervals vary by ±10 %.
const JITTER: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Slot {
    /// `None`: due now.
    next_due: Option<UnixTime>,
    failures: u32,
    not_before: Option<UnixTime>,
    min_interval_s: u32,
    last_manual: Option<UnixTime>,
}

/// Per-location due times with backoff, jitter and floors. With the `serde`
/// feature it can be saved and restored, e.g. across a device's deep sleep.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Scheduler {
    slots: Vec<Slot>,
    refresh_s: u32,
}

/// `seconds` scaled by `1 + 0.1 * jitter` (`jitter` in -1..=1).
fn jittered(seconds: u32, jitter: f32) -> i64 {
    let factor = 1.0 + JITTER * jitter.clamp(-1.0, 1.0);
    // Intervals are at most a day, so the product is exact enough and in range.
    libm::round(f64::from(seconds) * f64::from(factor)) as i64
}

/// The backoff after `failures` consecutive failures: 30 s doubling to 30 min.
pub fn backoff_s(failures: u32) -> u32 {
    let doublings = failures.saturating_sub(1).min(16);
    BACKOFF_START_S
        .saturating_mul(1 << doublings)
        .min(BACKOFF_MAX_S)
}

impl Scheduler {
    /// `n` locations, all due now. `min_interval_s[i]` (e.g. One Call pacing) raises
    /// location `i`'s interval above `refresh_s`; missing entries are 0.
    pub fn new(n: usize, refresh_s: u32, min_interval_s: &[u32]) -> Self {
        let slots = (0..n)
            .map(|i| Slot {
                min_interval_s: min_interval_s.get(i).copied().unwrap_or(0),
                ..Slot::default()
            })
            .collect();
        Self { slots, refresh_s }
    }

    /// The number of locations.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether there are no locations.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The effective interval of location `i`.
    pub fn interval_s(&self, i: usize) -> u32 {
        self.slots
            .get(i)
            .map_or(self.refresh_s, |s| self.refresh_s.max(s.min_interval_s))
    }

    fn earliest(slot: &Slot) -> Option<UnixTime> {
        match (slot.next_due, slot.not_before) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// Locations due at `now`.
    pub fn due(&self, now: UnixTime) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| Self::earliest(s).is_none_or(|t| t <= now))
            .map(|(i, _)| i)
            .collect()
    }

    /// When the next location becomes due (`now` if one already is).
    pub fn next_wakeup(&self, now: UnixTime) -> Option<UnixTime> {
        self.slots
            .iter()
            .map(|s| Self::earliest(s).unwrap_or(now).max(now))
            .min()
    }

    /// When location `i` is next due.
    pub fn next_due(&self, i: usize) -> Option<UnixTime> {
        self.slots.get(i).and_then(Self::earliest)
    }

    fn reschedule(&mut self, i: usize, now: UnixTime, not_before: Option<UnixTime>, jitter: f32) {
        let interval = self.interval_s(i);
        let Some(slot) = self.slots.get_mut(i) else {
            return;
        };
        // Jitter may lengthen the paced minimum but never shorten it.
        let wait = jittered(interval, jitter).max(i64::from(slot.min_interval_s));
        slot.next_due = Some(UnixTime(now.0 + wait));
        slot.not_before = not_before;
        slot.failures = 0;
    }

    /// Location `i` fetched successfully.
    pub fn on_success(
        &mut self,
        i: usize,
        now: UnixTime,
        not_before: Option<UnixTime>,
        jitter: f32,
    ) {
        self.reschedule(i, now, not_before, jitter);
    }

    /// The server said the data is unchanged.
    pub fn on_not_modified(&mut self, i: usize, now: UnixTime, not_before: Option<UnixTime>) {
        self.reschedule(i, now, not_before, 0.0);
    }

    /// Location `i` failed: retry after a growing delay (±10 %), but not sooner than
    /// its paced minimum.
    pub fn on_failure(&mut self, i: usize, now: UnixTime, jitter: f32) {
        let Some(slot) = self.slots.get_mut(i) else {
            return;
        };
        slot.failures = slot.failures.saturating_add(1);
        let wait = jittered(backoff_s(slot.failures), jitter).min(i64::from(BACKOFF_MAX_S));
        let wait = if slot.min_interval_s > 0 {
            wait.max(i64::from(slot.min_interval_s))
        } else {
            wait
        };
        slot.next_due = Some(UnixTime(now.0 + wait));
    }

    /// Makes location `i` (or all) due now, unless refreshed manually less than 60 s
    /// ago. Returns whether anything was scheduled. Call budgets still apply.
    pub fn request_refresh(&mut self, i: Option<usize>, now: UnixTime) -> bool {
        let mut scheduled = false;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if i.is_some_and(|i| i != index) {
                continue;
            }
            if slot
                .last_manual
                .is_some_and(|t| now.0 - t.0 < MANUAL_FLOOR_S)
            {
                continue;
            }
            slot.last_manual = Some(now);
            slot.next_due = Some(now);
            scheduled = true;
        }
        scheduled
    }
}

/// A tiny linear congruential generator for jitter (no `rand` dependency).
#[derive(Clone, Debug)]
pub struct Jitter(u64);

impl Jitter {
    /// Seeded, e.g. from the process id.
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// The next value in -1..=1.
    pub fn sample(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = (self.0 >> 40) as f32 / (1u64 << 24) as f32;
        unit * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const T0: UnixTime = UnixTime(1_791_217_800);

    fn at(s: i64) -> UnixTime {
        UnixTime(T0.0 + s)
    }

    #[test]
    fn everything_is_due_at_start() {
        let s = Scheduler::new(3, 900, &[]);
        assert_eq!((s.len(), s.is_empty()), (3, false));
        assert_eq!(s.due(T0), vec![0, 1, 2]);
        assert_eq!(s.next_wakeup(T0), Some(T0));
        assert_eq!(Scheduler::new(0, 900, &[]).next_wakeup(T0), None);
    }

    #[test]
    fn success_schedules_the_refresh_interval() {
        let mut s = Scheduler::new(2, 900, &[]);
        s.on_success(0, T0, None, 0.0);
        assert_eq!(s.due(T0), vec![1]);
        assert_eq!(s.next_due(0), Some(at(900)));
        s.on_success(1, T0, None, 0.0);
        assert_eq!(s.next_wakeup(T0), Some(at(900)));
        assert_eq!(s.due(at(900)), vec![0, 1]);
    }

    #[test]
    fn jitter_stays_within_ten_percent() {
        let mut s = Scheduler::new(1, 1000, &[]);
        s.on_success(0, T0, None, 1.0);
        assert_eq!(s.next_due(0), Some(at(1100)));
        s.on_success(0, T0, None, -1.0);
        assert_eq!(s.next_due(0), Some(at(900)));
        s.on_success(0, T0, None, 5.0);
        assert_eq!(s.next_due(0), Some(at(1100)), "clamped");
        let mut j = Jitter::new(42);
        assert!(
            (0..1000)
                .map(|_| j.sample())
                .all(|v| (-1.0..=1.0).contains(&v))
        );
    }

    #[test]
    fn paced_minimum_is_never_shortened() {
        let mut s = Scheduler::new(1, 900, &[1140]);
        assert_eq!(s.interval_s(0), 1140);
        s.on_success(0, T0, None, -1.0);
        assert_eq!(s.next_due(0), Some(at(1140)));
        s.on_failure(0, T0, 0.0);
        assert_eq!(
            s.next_due(0),
            Some(at(1140)),
            "retries wait for the pace too"
        );
    }

    #[test]
    fn backoff_sequence() {
        let seq: Vec<u32> = (1..=8).map(backoff_s).collect();
        assert_eq!(seq, vec![30, 60, 120, 240, 480, 960, 1800, 1800]);
        let mut s = Scheduler::new(1, 900, &[]);
        s.on_failure(0, T0, 0.0);
        assert_eq!(s.next_due(0), Some(at(30)));
        s.on_failure(0, T0, 0.0);
        assert_eq!(s.next_due(0), Some(at(60)));
        s.on_success(0, T0, None, 0.0);
        s.on_failure(0, T0, 0.0);
        assert_eq!(s.next_due(0), Some(at(30)), "success resets the backoff");
    }

    #[test]
    fn not_before_is_respected() {
        let mut s = Scheduler::new(1, 300, &[]);
        s.on_success(0, T0, Some(at(1800)), 0.0);
        assert_eq!(s.due(at(300)), [] as [usize; 0]);
        assert_eq!(s.next_wakeup(at(300)), Some(at(1800)));
        s.on_not_modified(0, at(1800), Some(at(3600)));
        assert_eq!(s.next_wakeup(at(1800)), Some(at(3600)));
    }

    #[test]
    fn manual_refresh_floor() {
        let mut s = Scheduler::new(2, 900, &[]);
        s.on_success(0, T0, None, 0.0);
        s.on_success(1, T0, None, 0.0);
        assert!(s.request_refresh(Some(0), at(10)));
        assert_eq!(s.due(at(10)), vec![0]);
        s.on_success(0, at(10), None, 0.0);
        assert!(!s.request_refresh(Some(0), at(30)), "within 60 s");
        assert!(
            s.request_refresh(None, at(30)),
            "location 1 was never refreshed manually"
        );
        assert_eq!(s.due(at(30)), vec![1]);
        assert!(s.request_refresh(Some(0), at(71)));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_saved_schedule_keeps_its_due_times_and_failures() {
        let mut s = Scheduler::new(2, 900, &[0, 1200]);
        s.on_success(0, T0, None, 0.0);
        s.on_failure(1, T0, 0.0);
        s.on_failure(1, at(60), 0.0);
        let json = serde_json::to_string(&s).unwrap();
        let mut restored: Scheduler = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, s);
        assert_eq!(restored.next_due(0), Some(at(900)));
        // The failure count survived: the third failure waits 120 s (but the
        // paced minimum of 1200 s wins).
        restored.on_failure(1, at(200), 0.0);
        assert_eq!(restored.next_due(1), Some(at(1400)));
    }
}
