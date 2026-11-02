//! Call budgets for metered APIs (OpenWeather One Call 4.0; periods and
//! weighted costs).
//!
//! A metered request is sent only after [`UsageLedger::try_spend`] succeeds, so a
//! device never exceeds its configured [`Budget`]. A budget covers one UTC day or one
//! UTC month, and a request costs one unit unless the provider says otherwise. Day
//! counters follow UTC days, like OpenWeather's, and no metered call is made in a
//! window around the provider's reset (00:00–00:05 UTC), so the two counters can
//! never disagree about which day a call belongs to. Month counters restart on the
//! first day of the month, UTC, with no such window. Calls are counted when sent and
//! never refunded, even when they fail. The caller persists the ledger before
//! performing each metered request.

use alloc::collections::BTreeMap;

use whether_core::model::ProviderId;
use whether_core::time::{UnixTime, civil_from_days, days_from_civil};

const DAY: i64 = 86_400;
/// The longest UTC month, in seconds.
const LONGEST_MONTH_S: u64 = 31 * 86_400;
/// No metered calls this many seconds before midnight UTC…
pub const RESET_GUARD_BEFORE_S: i64 = 5 * 60;
/// …or after it (OpenWeather resets between 00:00 and 00:05 UTC; the rest is margin
/// for clock error).
pub const RESET_GUARD_AFTER_S: i64 = 10 * 60;

/// Why a metered call was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuotaError {
    /// The period's budget is used up.
    #[error("the {} limit of {limit} calls is used up until {}", .period.adjective(), .period.resumes())]
    Exhausted {
        /// The limit.
        limit: u32,
        /// The period the limit covers.
        period: Period,
    },
    /// The provider's counter may be resetting.
    #[error("metered calls pause from 23:55 to 00:10 UTC while the daily counter resets")]
    ResetWindow,
    /// No ledger was attached to the fetch, so usage cannot be tracked.
    #[error("no usage ledger is attached, so metered calls are disabled")]
    NoLedger,
    /// The provider has no budget configured.
    #[error("the provider has no call budget configured")]
    NoLimit,
}

/// How long a [`Budget`] lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Period {
    /// One UTC day.
    Day,
    /// One UTC calendar month.
    Month,
}

impl Period {
    /// The first UTC day (days since the epoch) of the period that contains `day`.
    fn start_of_day(self, day: i64) -> i64 {
        match self {
            Self::Day => day,
            Self::Month => {
                let (year, month, _) = civil_from_days(day);
                days_from_civil(year, month, 1)
            }
        }
    }

    /// The first UTC day of the period that contains `now`.
    fn start_of(self, now: UnixTime) -> i64 {
        self.start_of_day(utc_day(now))
    }

    fn adjective(self) -> &'static str {
        match self {
            Self::Day => "daily",
            Self::Month => "monthly",
        }
    }

    fn resumes(self) -> &'static str {
        match self {
            Self::Day => "00:10 UTC",
            Self::Month => "the first of next month, UTC",
        }
    }
}

/// How many cost units a provider may use in one period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The period the limit covers.
    pub period: Period,
    /// The most cost units allowed in one period.
    pub limit: u32,
}

impl Budget {
    /// `limit` per UTC day.
    pub const fn daily(limit: u32) -> Self {
        Self {
            period: Period::Day,
            limit,
        }
    }

    /// `limit` per UTC month.
    pub const fn monthly(limit: u32) -> Self {
        Self {
            period: Period::Month,
            limit,
        }
    }

    /// The shortest refresh interval (seconds) at which fetches costing
    /// `cost_per_fetch` for each of `locations` stay within the limit in every
    /// period. A month is paced as 31 days, so it fits whichever month it is.
    pub fn paced_refresh_secs(self, cost_per_fetch: u32, locations: u32) -> u32 {
        match self.period {
            Period::Day => paced_refresh_secs(cost_per_fetch, locations, self.limit),
            Period::Month => pace(
                cost_per_fetch,
                locations,
                self.limit,
                LONGEST_MONTH_S,
                LONGEST_MONTH_S,
            ),
        }
    }
}

/// What one provider has used in its current period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Counter {
    /// The period `used` counts.
    period: Period,
    /// The first UTC day (days since the epoch) of that period.
    start: i64,
    /// Cost units sent in the period.
    used: u32,
}

/// Cost units spent per provider, each counted in its own period.
///
/// Serialized as `{"counters":{"openweather":{"period":"day","start":20731,"used":1}}}`.
/// JSON also reads the earlier `{"day":20731,"used":{"openweather":1}}`, which
/// counted every provider on one UTC day.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UsageLedger {
    counters: BTreeMap<ProviderId, Counter>,
}

/// The ledger as saved before this change, or in the current form.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct Saved {
    #[serde(default)]
    counters: BTreeMap<ProviderId, Counter>,
    #[serde(default)]
    day: Option<i64>,
    #[serde(default)]
    used: BTreeMap<ProviderId, u32>,
}

/// The binary form, which only ever had the current shape.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct Plain {
    counters: BTreeMap<ProviderId, Counter>,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for UsageLedger {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if !deserializer.is_human_readable() {
            // Binary formats are positional, so they cannot carry the optional fields.
            return Plain::deserialize(deserializer).map(|p| Self {
                counters: p.counters,
            });
        }
        let saved = Saved::deserialize(deserializer)?;
        let mut counters = saved.counters;
        if let Some(day) = saved.day {
            for (provider, used) in saved.used {
                counters.entry(provider).or_insert(Counter {
                    period: Period::Day,
                    start: day,
                    used,
                });
            }
        }
        Ok(Self { counters })
    }
}

/// UTC days since the epoch.
fn utc_day(now: UnixTime) -> i64 {
    now.0.div_euclid(DAY)
}

/// Whether `now` lies in the no-call window around midnight UTC.
pub fn in_reset_window(now: UnixTime) -> bool {
    let second = now.0.rem_euclid(DAY);
    !(RESET_GUARD_AFTER_S..DAY - RESET_GUARD_BEFORE_S).contains(&second)
}

impl UsageLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cost units `provider` has used in its current period. A clock that went back
    /// to an earlier period still sees the later period's count, so it cannot unlock a
    /// new budget.
    pub fn used(&self, provider: ProviderId, now: UnixTime) -> u32 {
        self.counters.get(&provider).map_or(0, |c| {
            if c.period.start_of(now) > c.start {
                0
            } else {
                c.used
            }
        })
    }

    /// Cost units left under `limit` in the provider's current period.
    pub fn remaining(&self, provider: ProviderId, limit: u32, now: UnixTime) -> u32 {
        limit.saturating_sub(self.used(provider, now))
    }

    /// Counts a request of `cost` units (1 for an ordinary call) if it fits under
    /// `budget` and, for a day budget, `now` is outside the reset window. Changing a
    /// provider's period starts a new count.
    pub fn try_spend(
        &mut self,
        provider: ProviderId,
        budget: Budget,
        cost: u32,
        now: UnixTime,
    ) -> Result<(), QuotaError> {
        if budget.period == Period::Day && in_reset_window(now) {
            return Err(QuotaError::ResetWindow);
        }
        let start = budget.period.start_of(now);
        let counter = self.counters.entry(provider).or_insert(Counter {
            period: budget.period,
            start,
            used: 0,
        });
        if counter.period != budget.period || start > counter.start {
            *counter = Counter {
                period: budget.period,
                start,
                used: 0,
            };
        }
        match counter.used.checked_add(cost) {
            Some(used) if used <= budget.limit => {
                counter.used = used;
                Ok(())
            }
            _ => Err(QuotaError::Exhausted {
                limit: budget.limit,
                period: budget.period,
            }),
        }
    }
}

/// Calls a device has reserved in flash for one provider and UTC day. Writing
/// the count before every call would wear the flash, so the device writes
/// `reserved = used + block - 1` when a call goes beyond the stored reservation,
/// and after a restart counts every reserved call as used: a power loss can waste
/// budget but never add to it. Devices reserve day budgets only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reservation {
    /// UTC days since the epoch.
    pub day: i64,
    /// Calls reserved on `day`.
    pub reserved: u32,
}

impl UsageLedger {
    /// A ledger resumed from a stored reservation: every reserved call is used.
    pub fn from_reservation(provider: ProviderId, r: Reservation) -> Self {
        let mut counters = BTreeMap::new();
        counters.insert(
            provider,
            Counter {
                period: Period::Day,
                start: r.day,
                used: r.reserved,
            },
        );
        Self { counters }
    }

    /// The reservation to store, before sending, after a call has been counted
    /// with [`Self::try_spend`]: `None` when `stored` already covers today's count,
    /// or when the provider's budget is not a day budget.
    pub fn reservation_after_spend(
        &self,
        provider: ProviderId,
        stored: Option<Reservation>,
        block: u32,
    ) -> Option<Reservation> {
        let counter = self
            .counters
            .get(&provider)
            .filter(|c| c.period == Period::Day)?;
        let covered = stored
            .filter(|r| r.day == counter.start)
            .map_or(0, |r| r.reserved);
        (counter.used > covered).then(|| Reservation {
            day: counter.start,
            reserved: counter.used + block.max(1) - 1,
        })
    }
}

/// The shortest refresh interval (seconds) at which `calls_per_fetch` metered calls
/// for each of `locations` stay within `limit` calls per day. A day's usable window
/// of length W at interval I holds `floor(W / I) + 1` rounds.
pub fn paced_refresh_secs(calls_per_fetch: u32, locations: u32, limit: u32) -> u32 {
    let usable = (DAY - RESET_GUARD_BEFORE_S - RESET_GUARD_AFTER_S).unsigned_abs();
    pace(
        calls_per_fetch,
        locations,
        limit,
        usable,
        DAY.unsigned_abs(),
    )
}

/// The interval for [`paced_refresh_secs`] over a period of `whole` seconds, of
/// which `usable` seconds may hold calls.
fn pace(cost_per_fetch: u32, locations: u32, limit: u32, usable: u64, whole: u64) -> u32 {
    let per_round = u64::from(cost_per_fetch) * u64::from(locations.max(1));
    if per_round == 0 {
        return 0;
    }
    match u64::from(limit) / per_round {
        0 => u32::MAX,
        1 => u32::try_from(whole).unwrap_or(u32::MAX),
        rounds => u32::try_from(usable.div_ceil(rounds - 1)).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWM: ProviderId = ProviderId::OpenWeather;
    /// A second provider, for budgets with a period of their own.
    const PIRATE: ProviderId = ProviderId::OpenMeteo;
    /// 2026-10-05 12:00:00 UTC.
    const NOON: UnixTime = UnixTime(1_791_201_600);

    fn at(day_offset: i64, second: i64) -> UnixTime {
        UnixTime(NOON.0 - 12 * 3600 + day_offset * DAY + second)
    }

    #[test]
    fn spends_up_to_the_limit() {
        let mut ledger = UsageLedger::new();
        for _ in 0..3 {
            ledger.try_spend(OWM, Budget::daily(3), 1, NOON).unwrap();
        }
        assert_eq!(
            ledger.try_spend(OWM, Budget::daily(3), 1, NOON),
            Err(QuotaError::Exhausted {
                limit: 3,
                period: Period::Day
            })
        );
        assert_eq!(ledger.used(OWM, NOON), 3);
        assert_eq!(ledger.remaining(OWM, 3, NOON), 0);
        assert_eq!(ledger.used(ProviderId::OpenMeteo, NOON), 0);
    }

    #[test]
    fn a_new_utc_day_resets_the_count() {
        let mut ledger = UsageLedger::new();
        ledger.try_spend(OWM, Budget::daily(1), 1, NOON).unwrap();
        assert!(
            ledger
                .try_spend(OWM, Budget::daily(1), 1, at(0, 23 * 3600))
                .is_err()
        );
        let tomorrow = at(1, 3600);
        assert_eq!(ledger.used(OWM, tomorrow), 0);
        ledger
            .try_spend(OWM, Budget::daily(1), 1, tomorrow)
            .unwrap();
    }

    #[test]
    fn a_clock_going_back_does_not_unlock_budget() {
        let mut ledger = UsageLedger::new();
        ledger
            .try_spend(OWM, Budget::daily(1), 1, at(1, 3600))
            .unwrap();
        assert_eq!(
            ledger.used(OWM, NOON),
            1,
            "yesterday's clock sees today's count"
        );
        assert_eq!(
            ledger.try_spend(OWM, Budget::daily(1), 1, NOON),
            Err(QuotaError::Exhausted {
                limit: 1,
                period: Period::Day
            })
        );
    }

    #[test]
    fn no_calls_around_the_reset() {
        let mut ledger = UsageLedger::new();
        for second in [0, 300, 599, DAY - 300, DAY - 1] {
            assert_eq!(
                ledger.try_spend(OWM, Budget::daily(10), 1, at(0, second)),
                Err(QuotaError::ResetWindow),
                "{second}"
            );
        }
        assert!(
            ledger
                .try_spend(OWM, Budget::daily(10), 1, at(0, 600))
                .is_ok()
        );
        assert!(
            ledger
                .try_spend(OWM, Budget::daily(10), 1, at(0, DAY - 301))
                .is_ok()
        );
        assert_eq!(ledger.used(OWM, NOON), 2, "refused calls are not counted");
    }

    #[test]
    fn a_zero_limit_allows_nothing() {
        let mut ledger = UsageLedger::new();
        assert_eq!(
            ledger.try_spend(OWM, Budget::daily(0), 1, NOON),
            Err(QuotaError::Exhausted {
                limit: 0,
                period: Period::Day
            })
        );
    }

    #[test]
    fn pacing() {
        // 4 calls per fetch, 1 location, 900 calls a day: one fetch every 6.4 minutes.
        assert_eq!(paced_refresh_secs(4, 1, 900), 382);
        assert_eq!(paced_refresh_secs(4, 3, 900), 1156);
        assert_eq!(paced_refresh_secs(0, 3, 900), 0);
        assert_eq!(paced_refresh_secs(4, 1, 3), u32::MAX);
        assert_eq!(paced_refresh_secs(4, 1, 4), 86_400);
    }

    #[test]
    fn paced_days_stay_within_the_limit() {
        let usable = DAY - RESET_GUARD_BEFORE_S - RESET_GUARD_AFTER_S;
        for calls in 1..=8 {
            for locations in 1..=5 {
                for limit in [100, 500, 900, 1000] {
                    let interval = i64::from(paced_refresh_secs(calls, locations, limit));
                    let rounds = if interval > usable {
                        1
                    } else {
                        usable / interval + 1
                    };
                    if interval != i64::from(u32::MAX) {
                        assert!(
                            rounds * i64::from(calls * locations) <= i64::from(limit),
                            "{calls} {locations} {limit}"
                        );
                    }
                }
            }
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn round_trips_through_json() {
        let mut ledger = UsageLedger::new();
        ledger.try_spend(OWM, Budget::daily(10), 1, NOON).unwrap();
        ledger
            .try_spend(PIRATE, Budget::monthly(100), 4, NOON)
            .unwrap();
        let text = serde_json::to_string(&ledger).unwrap();
        assert_eq!(
            text,
            r#"{"counters":{"openweather":{"period":"day","start":20731,"used":1},"open-meteo":{"period":"month","start":20727,"used":4}}}"#
        );
        assert_eq!(serde_json::from_str::<UsageLedger>(&text).unwrap(), ledger);
    }

    /// The ledger the phone and the desktop saved before this change (`UsageLedger` as
    /// serialized earlier): one UTC day for every provider.
    #[cfg(feature = "serde")]
    #[test]
    fn reads_the_ledger_saved_before_periods() {
        let mut ledger: UsageLedger =
            serde_json::from_str(r#"{"day":20731,"used":{"openweather":7}}"#).unwrap();
        assert_eq!(ledger.used(OWM, NOON), 7);
        assert_eq!(ledger.used(OWM, at(1, 3600)), 0, "a new day starts at 0");
        assert_eq!(ledger.used(PIRATE, NOON), 0);
        // The count carries on under a day budget at cost 1.
        ledger.try_spend(OWM, Budget::daily(8), 1, NOON).unwrap();
        assert_eq!(
            ledger.try_spend(OWM, Budget::daily(8), 1, NOON),
            Err(QuotaError::Exhausted {
                limit: 8,
                period: Period::Day
            })
        );
        // Saved again, it is in the new form.
        let text = serde_json::to_string(&ledger).unwrap();
        assert_eq!(
            text,
            r#"{"counters":{"openweather":{"period":"day","start":20731,"used":8}}}"#
        );
        // An empty old ledger, and the empty ledger of a fresh install.
        for empty in [r#"{"day":0,"used":{}}"#, r"{}"] {
            assert_eq!(
                serde_json::from_str::<UsageLedger>(empty).unwrap(),
                UsageLedger::new()
            );
        }
        // A yesterday's old ledger counts as yesterday's.
        let old: UsageLedger =
            serde_json::from_str(r#"{"day":20730,"used":{"openweather":900}}"#).unwrap();
        assert_eq!(old.used(OWM, NOON), 0);
    }

    #[test]
    fn providers_count_in_their_own_periods() {
        let mut ledger = UsageLedger::new();
        ledger.try_spend(OWM, Budget::daily(2), 1, NOON).unwrap();
        ledger
            .try_spend(PIRATE, Budget::monthly(5), 1, NOON)
            .unwrap();
        // The next day resets the day counter and keeps the month's.
        let tomorrow = at(1, 3600);
        assert_eq!(ledger.used(OWM, tomorrow), 0);
        assert_eq!(ledger.used(PIRATE, tomorrow), 1);
        ledger
            .try_spend(PIRATE, Budget::monthly(5), 1, tomorrow)
            .unwrap();
        assert_eq!(ledger.used(OWM, NOON), 1, "today's count is still there");
    }

    #[test]
    fn a_month_budget_resets_on_the_first_utc_day() {
        let budget = Budget::monthly(3);
        let mut ledger = UsageLedger::new();
        ledger.try_spend(PIRATE, budget, 1, NOON).unwrap();
        ledger.try_spend(PIRATE, budget, 2, at(20, 3600)).unwrap();
        // The last second of October is still October.
        let last = at(26, DAY - 1);
        assert_eq!(ledger.used(PIRATE, last), 3);
        assert_eq!(
            ledger.try_spend(PIRATE, budget, 1, last),
            Err(QuotaError::Exhausted {
                limit: 3,
                period: Period::Month
            })
        );
        // November starts at 00:00:00 UTC, with no pause around the reset.
        let november = at(27, 0);
        assert_eq!(ledger.used(PIRATE, november), 0);
        ledger.try_spend(PIRATE, budget, 3, november).unwrap();
        assert_eq!(ledger.remaining(PIRATE, 3, november), 0);
        // A clock that goes back to October cannot unlock budget.
        assert_eq!(ledger.used(PIRATE, NOON), 3);
        assert!(ledger.try_spend(PIRATE, budget, 1, NOON).is_err());
    }

    #[test]
    fn months_follow_the_calendar() {
        let budget = Budget::monthly(1);
        let mut ledger = UsageLedger::new();
        // 2028-02-29 (a leap day) and 2028-03-01, 00:30 UTC.
        let leap_day = UnixTime(days_from_civil(2028, 2, 29) * DAY + 1800);
        let march = UnixTime(days_from_civil(2028, 3, 1) * DAY + 1800);
        ledger.try_spend(PIRATE, budget, 1, leap_day).unwrap();
        assert!(ledger.try_spend(PIRATE, budget, 1, leap_day).is_err());
        ledger.try_spend(PIRATE, budget, 1, march).unwrap();
        // December rolls into January.
        let mut ledger = UsageLedger::new();
        let december = UnixTime(days_from_civil(2026, 12, 31) * DAY);
        let january = UnixTime(days_from_civil(2027, 1, 1) * DAY);
        ledger.try_spend(PIRATE, budget, 1, december).unwrap();
        assert_eq!(ledger.used(PIRATE, january), 0);
    }

    #[test]
    fn requests_can_cost_more_than_one() {
        let budget = Budget::daily(10);
        let mut ledger = UsageLedger::new();
        ledger.try_spend(OWM, budget, 6, NOON).unwrap();
        // 6 + 5 would pass the limit: refused and not counted.
        assert_eq!(
            ledger.try_spend(OWM, budget, 5, NOON),
            Err(QuotaError::Exhausted {
                limit: 10,
                period: Period::Day
            })
        );
        assert_eq!(ledger.used(OWM, NOON), 6);
        ledger.try_spend(OWM, budget, 4, NOON).unwrap();
        assert_eq!(ledger.remaining(OWM, 10, NOON), 0);
        // Free requests pass at the limit; a request dearer than the whole budget never does.
        ledger.try_spend(OWM, budget, 0, NOON).unwrap();
        let mut fresh = UsageLedger::new();
        assert!(fresh.try_spend(OWM, budget, 11, NOON).is_err());
        assert_eq!(fresh.used(OWM, NOON), 0);
        assert!(
            fresh
                .try_spend(OWM, Budget::daily(u32::MAX), u32::MAX, NOON)
                .is_ok()
        );
        assert!(
            fresh
                .try_spend(OWM, Budget::daily(u32::MAX), 1, NOON)
                .is_err()
        );
    }

    #[test]
    fn changing_the_period_starts_a_new_count() {
        let mut ledger = UsageLedger::new();
        ledger.try_spend(OWM, Budget::daily(5), 5, NOON).unwrap();
        ledger.try_spend(OWM, Budget::monthly(5), 1, NOON).unwrap();
        assert_eq!(ledger.used(OWM, NOON), 1);
        // And back: the month's count does not leak into a day budget.
        ledger.try_spend(OWM, Budget::daily(5), 1, NOON).unwrap();
        assert_eq!(ledger.used(OWM, NOON), 1);
    }

    #[test]
    fn quota_messages_name_the_period() {
        let day = QuotaError::Exhausted {
            limit: 900,
            period: Period::Day,
        };
        assert_eq!(
            day.to_string(),
            "the daily limit of 900 calls is used up until 00:10 UTC"
        );
        let month = QuotaError::Exhausted {
            limit: 9_000,
            period: Period::Month,
        };
        assert_eq!(
            month.to_string(),
            "the monthly limit of 9000 calls is used up until the first of next month, UTC"
        );
    }

    #[test]
    fn pacing_a_month_fits_every_month() {
        let month = Budget::monthly(9_000);
        // 1 call per fetch: 9,000 rounds in 31 days = one every 298 s.
        let interval = month.paced_refresh_secs(1, 1);
        assert_eq!(interval, 298);
        let rounds = i64::from(31 * 86_400 / interval) + 1;
        assert!(rounds <= 9_000, "{rounds}");
        // Weighted fetches and more places stretch it; too little budget never fetches.
        assert!(month.paced_refresh_secs(4, 3) > interval * 11);
        assert_eq!(month.paced_refresh_secs(0, 3), 0);
        assert_eq!(Budget::monthly(3).paced_refresh_secs(4, 1), u32::MAX);
        assert_eq!(Budget::monthly(4).paced_refresh_secs(4, 1), 31 * 86_400);
        // A day budget paces as before.
        assert_eq!(Budget::daily(900).paced_refresh_secs(4, 1), 382);
    }

    #[test]
    fn only_day_counters_are_reserved_in_flash() {
        let mut ledger = UsageLedger::new();
        ledger
            .try_spend(PIRATE, Budget::monthly(100), 1, NOON)
            .unwrap();
        assert_eq!(ledger.reservation_after_spend(PIRATE, None, 10), None);
        assert_eq!(ledger.reservation_after_spend(OWM, None, 10), None);
    }

    #[test]
    fn reservations_cover_blocks_of_calls() {
        let mut ledger = UsageLedger::new();
        let mut stored = None;
        let mut writes = 0;
        for _ in 0..25 {
            ledger.try_spend(OWM, Budget::daily(900), 1, NOON).unwrap();
            if let Some(r) = ledger.reservation_after_spend(OWM, stored, 10) {
                stored = Some(r);
                writes += 1;
            }
            let r = stored.unwrap();
            assert!(
                r.reserved >= ledger.used(OWM, NOON),
                "every sent call is reserved"
            );
        }
        assert_eq!(writes, 3, "calls 1, 11 and 21 open a block");
        assert_eq!(stored.unwrap().reserved, 30);
    }

    #[test]
    fn a_restart_counts_every_reserved_call() {
        let stored = Reservation {
            day: NOON.0.div_euclid(DAY),
            reserved: 30,
        };
        let mut ledger = UsageLedger::from_reservation(OWM, stored);
        assert_eq!(ledger.used(OWM, NOON), 30);
        ledger.try_spend(OWM, Budget::daily(900), 1, NOON).unwrap();
        assert_eq!(
            ledger.reservation_after_spend(OWM, Some(stored), 10),
            Some(Reservation {
                day: stored.day,
                reserved: 40
            })
        );
        let mut limited = UsageLedger::from_reservation(OWM, stored);
        assert_eq!(
            limited.try_spend(OWM, Budget::daily(30), 1, NOON),
            Err(QuotaError::Exhausted {
                limit: 30,
                period: Period::Day
            })
        );
        // A new UTC day starts a new count and a new reservation.
        let tomorrow = at(1, 3600);
        ledger
            .try_spend(OWM, Budget::daily(900), 1, tomorrow)
            .unwrap();
        assert_eq!(
            ledger.reservation_after_spend(OWM, Some(stored), 10),
            Some(Reservation {
                day: stored.day + 1,
                reserved: 10
            })
        );
    }
}
