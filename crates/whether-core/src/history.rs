//! What the weather was: observed past days, and each calendar date's normals,
//! records and chance of precipitation over a range of years. History
//! sits beside the forecast, with its own sources, refresh and cache. Its climate
//! statistics describe calendar dates, so the same types answer for today, the
//! coming week, or a trip's dates next summer.

use alloc::string::String;
use alloc::vec::Vec;

use crate::builder::ModelError;
use crate::model::{ProviderId, ResolvedLocation, Station};
use crate::time::{LocalDate, UnixTime, is_leap_year};
use crate::units::{Celsius, Mm, Percent};

/// Where the history comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum HistoryKind {
    /// A weather station's own measurements.
    #[default]
    Station,
    /// A reanalysis grid (ERA5): smooth, about 25 km, not a station's record.
    Reanalysis,
}

/// The years statistics cover, both included: `1991–2020`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct YearRange {
    /// The first year.
    pub first: i32,
    /// The last year.
    pub last: i32,
}

impl YearRange {
    /// The official US normals period.
    pub const NORMALS_1991_2020: Self = Self {
        first: 1991,
        last: 2020,
    };
}

/// One observed day.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PastDay {
    /// The local date.
    pub date: LocalDate,
    /// The high.
    pub temp_max: Option<Celsius>,
    /// The low.
    pub temp_min: Option<Celsius>,
    /// Rain and melted snow; 0 with `trace`.
    pub precip_sum: Option<Mm>,
    /// Snowfall (depth of new snow).
    pub snow_sum: Option<Mm>,
    /// Precipitation too little to measure fell.
    #[cfg_attr(feature = "serde", serde(default))]
    pub trace: bool,
}

impl PastDay {
    /// A day with nothing known yet.
    pub fn new(date: LocalDate) -> Self {
        Self {
            date,
            temp_max: None,
            temp_min: None,
            precip_sum: None,
            snow_sum: None,
            trace: false,
        }
    }
}

/// An extreme and the year it happened.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Record {
    /// The value, in its field's canonical unit (°C or mm).
    pub value: f32,
    /// The year.
    pub year: i32,
}

/// A calendar date's climate over [`History::period`]: averages, extremes and
/// how often it rained or snowed. `date` is the date it is shown for (any
/// year); the statistics are for its month and day.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DayClimate {
    /// The date it describes.
    pub date: LocalDate,
    /// The average high.
    pub normal_max: Option<Celsius>,
    /// The average low.
    pub normal_min: Option<Celsius>,
    /// The average precipitation.
    pub normal_precip: Option<Mm>,
    /// The share of years with measurable precipitation (≥ 0.25 mm) that day.
    pub precip_chance: Option<Percent>,
    /// The highest high.
    pub record_max: Option<Record>,
    /// The lowest low.
    pub record_min: Option<Record>,
    /// The lowest high.
    pub record_low_max: Option<Record>,
    /// The highest low.
    pub record_high_min: Option<Record>,
    /// The wettest.
    pub record_precip: Option<Record>,
}

impl DayClimate {
    /// A date with nothing known yet.
    pub fn new(date: LocalDate) -> Self {
        Self {
            date,
            normal_max: None,
            normal_min: None,
            normal_precip: None,
            precip_chance: None,
            record_max: None,
            record_min: None,
            record_low_max: None,
            record_high_min: None,
            record_precip: None,
        }
    }

    /// Fills what this lacks from `other`; the number of fields filled.
    fn fill_from(&mut self, other: &Self) -> usize {
        let mut filled = 0;
        macro_rules! fill {
            ($($field:ident),+) => {
                $(if self.$field.is_none() && other.$field.is_some() {
                    self.$field = other.$field;
                    filled += 1;
                })+
            };
        }
        fill!(
            normal_max,
            normal_min,
            normal_precip,
            precip_chance,
            record_max,
            record_min,
            record_low_max,
            record_high_min,
            record_precip
        );
        filled
    }
}

/// A place's history.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct History {
    /// Where it applies.
    pub location: ResolvedLocation,
    /// The main source.
    pub source: ProviderId,
    /// Sources that filled gaps; all are credited.
    pub supplemented_by: Vec<ProviderId>,
    /// When it was received.
    pub fetched_at: UnixTime,
    /// A station's record, or a reanalysis.
    pub kind: HistoryKind,
    /// The station that measured the days and normals.
    pub station: Option<Station>,
    /// The station (or threaded record) the records come from, when another.
    pub records_station: Option<Station>,
    /// Observed days, ascending, up to yesterday.
    pub days: Vec<PastDay>,
    /// Climate per date, ascending.
    pub climate: Vec<DayClimate>,
    /// The years the climate covers.
    pub period: Option<YearRange>,
    /// Normals for every calendar date that a source computed (ERA5), for the
    /// caller to keep: they never change, and computing them is costly.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub normals: Option<Normals>,
}

/// One calendar date's normals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DayNormal {
    /// The average high.
    pub max: Option<Celsius>,
    /// The average low.
    pub min: Option<Celsius>,
    /// The average precipitation.
    pub precip: Option<Mm>,
    /// The share of days with measurable precipitation (≥ 0.25 mm).
    pub precip_chance: Option<Percent>,
}

/// Normals for all 366 calendar dates over a range of years, computed from daily
/// values (a reanalysis): each date pools the 15 dates around it, every year.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Normals {
    /// The years they cover.
    pub period: YearRange,
    /// Who computed them from what.
    pub source: ProviderId,
    /// 1 January to 31 December of a leap year (29 February included).
    pub days: Vec<DayNormal>,
}

/// One day's values for [`Normals::from_daily`]: date, high, low, precipitation.
pub type DailyValues = (LocalDate, Option<f32>, Option<f32>, Option<f32>);

/// Measurable precipitation, in mm.
pub const WET_MM: f32 = 0.25;
/// Days on each side pooled into a date's normals.
const POOL_DAYS: i64 = 7;
/// The fewest values a normal needs.
const MIN_VALUES: usize = 30;

/// The index of a calendar date in a leap year (0 = 1 January, 59 = 29 February).
fn leap_index(month: u8, day: u8) -> Option<usize> {
    const DAYS: [u8; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let m = usize::from(month.checked_sub(1)?);
    if m >= 12 || day == 0 || day > DAYS[m] {
        return None;
    }
    Some(DAYS[..m].iter().map(|d| usize::from(*d)).sum::<usize>() + usize::from(day) - 1)
}

impl Normals {
    /// The normals of `date`'s month and day.
    pub fn on(&self, date: LocalDate) -> Option<&DayNormal> {
        self.days.get(leap_index(date.month, date.day)?)
    }

    /// Normals from daily values over `period`: each calendar date averages
    /// every value within 7 days of it in every year (so 29 February pools its
    /// neighbours too); a date with fewer than 30 values has none.
    pub fn from_daily(period: YearRange, source: ProviderId, values: &[DailyValues]) -> Self {
        #[derive(Clone, Copy, Default)]
        struct Sums {
            max: (f64, usize),
            min: (f64, usize),
            precip: (f64, usize),
            wet: usize,
        }
        // Per calendar date first, then pooled.
        let mut by_date = alloc::vec![Sums::default(); 366];
        for (date, max, min, precip) in values {
            if date.year < period.first || date.year > period.last {
                continue;
            }
            let Some(i) = leap_index(date.month, date.day) else {
                continue;
            };
            let s = &mut by_date[i];
            for (sum, v) in [
                (&mut s.max, max),
                (&mut s.min, min),
                (&mut s.precip, precip),
            ] {
                if let Some(v) = v.filter(|v| v.is_finite()) {
                    sum.0 += f64::from(v);
                    sum.1 += 1;
                }
            }
            if precip.is_some_and(|p| p >= WET_MM) {
                s.wet += 1;
            }
        }
        let mean = |(sum, n): (f64, usize)| {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            (n >= MIN_VALUES).then(|| (sum / n as f64) as f32)
        };
        let days = (0..366_i64)
            .map(|i| {
                let mut pool = Sums::default();
                for d in -POOL_DAYS..=POOL_DAYS {
                    // INVARIANT: rem_euclid(366) is in 0..366.
                    let s = by_date[usize::try_from((i + d).rem_euclid(366)).unwrap_or(0)];
                    for (to, from) in [
                        (&mut pool.max, s.max),
                        (&mut pool.min, s.min),
                        (&mut pool.precip, s.precip),
                    ] {
                        to.0 += from.0;
                        to.1 += from.1;
                    }
                    pool.wet += s.wet;
                }
                DayNormal {
                    max: mean(pool.max).map(Celsius),
                    min: mean(pool.min).map(Celsius),
                    precip: mean(pool.precip).map(Mm),
                    #[allow(clippy::cast_precision_loss)]
                    precip_chance: (pool.precip.1 >= MIN_VALUES)
                        .then(|| Percent(100.0 * pool.wet as f32 / pool.precip.1 as f32)),
                }
            })
            .collect();
        Self {
            period,
            source,
            days,
        }
    }
}

/// The most dates one climate query covers.
pub const MAX_QUERY_DAYS: usize = 31;

/// Why a [`ClimateQuery`] cannot be asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    /// A date that does not exist.
    #[error("no such date")]
    Date,
    /// The last date comes before the first.
    #[error("the last date comes before the first")]
    Backwards,
    /// More than [`MAX_QUERY_DAYS`] dates.
    #[error("at most 31 days")]
    TooLong,
    /// The years run backwards.
    #[error("the first year comes after the last")]
    Years,
}

/// Dates to plan for and the years to look back over: "12–19 July 2027, as it
/// was in 1995–2024" (the vacation planner). The dates are those of
/// the trip; their month and day are what the years are searched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClimateQuery {
    /// The first date.
    pub from: LocalDate,
    /// The last date (inclusive).
    pub to: LocalDate,
    /// The years to look back over.
    pub years: YearRange,
}

impl ClimateQuery {
    /// A query of real dates, `from` to `to` (at most 31), over `years`.
    pub fn new(from: LocalDate, to: LocalDate, years: YearRange) -> Result<Self, QueryError> {
        let real = |d: LocalDate| {
            (1..=12).contains(&d.month)
                && d.day >= 1
                && d.day <= crate::time::days_in_month(d.year, d.month)
        };
        if !real(from) || !real(to) {
            return Err(QueryError::Date);
        }
        let days = to.days_since_epoch() - from.days_since_epoch();
        if days < 0 {
            return Err(QueryError::Backwards);
        }
        if usize::try_from(days).map_or(true, |d| d >= MAX_QUERY_DAYS) {
            return Err(QueryError::TooLong);
        }
        if years.first > years.last {
            return Err(QueryError::Years);
        }
        Ok(Self { from, to, years })
    }

    /// Every date, `from` to `to`.
    pub fn dates(&self) -> Vec<LocalDate> {
        (self.from.days_since_epoch()..=self.to.days_since_epoch())
            .map(|d| {
                let (year, month, day) = crate::time::civil_from_days(d);
                LocalDate { year, month, day }
            })
            .collect()
    }
}

/// The climate of each of `query`'s dates from daily values (a reanalysis):
/// the same month and day in each of its years (29 February: 28 February in
/// the other years), averaged, its extremes with their years, and the share of
/// years with measurable precipitation. A date with values from fewer than
/// half the years has none.
pub fn climate_from_daily(query: &ClimateQuery, values: &[DailyValues]) -> Vec<DayClimate> {
    let span = query.years.last - query.years.first + 1;
    let wanted = usize::try_from(span / 2).unwrap_or(0).max(1);
    query
        .dates()
        .into_iter()
        .map(|date| {
            let leap_day = (date.month, date.day) == (2, 29);
            let same: Vec<&DailyValues> = values
                .iter()
                .filter(|(d, ..)| {
                    let same_day = (d.month, d.day) == (date.month, date.day)
                        || (leap_day && (d.month, d.day) == (2, 28) && !is_leap_year(d.year));
                    same_day && (query.years.first..=query.years.last).contains(&d.year)
                })
                .collect();
            let column = |pick: fn(&DailyValues) -> Option<f32>| -> Vec<(f32, i32)> {
                same.iter()
                    .filter_map(|v| pick(v).filter(|x| x.is_finite()).map(|x| (x, v.0.year)))
                    .collect()
            };
            let (max, min, precip) = (column(|v| v.1), column(|v| v.2), column(|v| v.3));
            let enough = |c: &[(f32, i32)]| c.len() >= wanted;
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let mean = |c: &[(f32, i32)]| {
                enough(c).then(|| {
                    (c.iter().map(|(x, _)| f64::from(*x)).sum::<f64>() / c.len() as f64) as f32
                })
            };
            // Ties go to the latest year.
            let extreme = |c: &[(f32, i32)], highest: bool| {
                enough(c)
                    .then(|| {
                        c.iter().copied().reduce(|a, b| {
                            let better = if highest { b.0 >= a.0 } else { b.0 <= a.0 };
                            if better { b } else { a }
                        })
                    })
                    .flatten()
                    .map(|(value, year)| Record { value, year })
            };
            let mut c = DayClimate::new(date);
            c.normal_max = mean(&max).map(Celsius);
            c.normal_min = mean(&min).map(Celsius);
            c.normal_precip = mean(&precip).map(Mm);
            #[allow(clippy::cast_precision_loss)]
            {
                c.precip_chance = enough(&precip).then(|| {
                    let wet = precip.iter().filter(|(p, _)| *p >= WET_MM).count();
                    Percent(100.0 * wet as f32 / precip.len() as f32)
                });
            }
            c.record_max = extreme(&max, true);
            c.record_low_max = extreme(&max, false);
            c.record_min = extreme(&min, false);
            c.record_high_min = extreme(&min, true);
            c.record_precip = extreme(&precip, true);
            c
        })
        .collect()
}

impl History {
    /// Fills each of `dates`' empty normals and chance of precipitation from
    /// `normals`, and the period when there is none.
    pub fn fill_normals(&mut self, normals: &Normals, dates: impl IntoIterator<Item = LocalDate>) {
        let mut filled = false;
        for date in dates {
            let Some(n) = normals.on(date) else { continue };
            let i = if let Some(i) = self.climate.iter().position(|c| c.date == date) {
                i
            } else {
                self.climate.push(DayClimate::new(date));
                self.climate.len() - 1
            };
            let c = &mut self.climate[i];
            for (field, value) in [(&mut c.normal_max, n.max), (&mut c.normal_min, n.min)] {
                if field.is_none() && value.is_some() {
                    *field = value;
                    filled = true;
                }
            }
            if c.normal_precip.is_none() && n.precip.is_some() {
                c.normal_precip = n.precip;
                filled = true;
            }
            if c.precip_chance.is_none() && n.precip_chance.is_some() {
                c.precip_chance = n.precip_chance;
                filled = true;
            }
        }
        self.climate.sort_by_key(|c| c.date);
        if filled && self.period.is_none() {
            self.period = Some(normals.period);
        }
    }

    /// The observed day `date`.
    pub fn day(&self, date: LocalDate) -> Option<&PastDay> {
        self.days.iter().find(|d| d.date == date)
    }

    /// The climate of `date`.
    pub fn climate_on(&self, date: LocalDate) -> Option<&DayClimate> {
        self.climate.iter().find(|c| c.date == date)
    }

    /// Fills what this lacks from `other` (another source for the same place):
    /// missing days and dates are added, missing fields filled; this one's values
    /// win. `other` is credited when it filled anything. Returns the number of
    /// days, dates and fields filled.
    pub fn fill_gaps_from(&mut self, other: &Self) -> usize {
        let mut filled = 0;
        for day in &other.days {
            if let Some(mine) = self.days.iter_mut().find(|d| d.date == day.date) {
                for (field, theirs) in [
                    (&mut mine.temp_max, day.temp_max),
                    (&mut mine.temp_min, day.temp_min),
                ] {
                    if field.is_none() && theirs.is_some() {
                        *field = theirs;
                        filled += 1;
                    }
                }
                for (field, theirs) in [
                    (&mut mine.precip_sum, day.precip_sum),
                    (&mut mine.snow_sum, day.snow_sum),
                ] {
                    if field.is_none() && theirs.is_some() {
                        *field = theirs;
                        filled += 1;
                    }
                }
            } else {
                self.days.push(*day);
                filled += 1;
            }
        }
        for climate in &other.climate {
            if let Some(mine) = self.climate.iter_mut().find(|c| c.date == climate.date) {
                filled += mine.fill_from(climate);
            } else {
                self.climate.push(*climate);
                filled += 1;
            }
        }
        self.days.sort_by_key(|d| d.date);
        self.climate.sort_by_key(|c| c.date);
        if self.period.is_none() && other.period.is_some() && filled > 0 {
            self.period = other.period;
        }
        if filled > 0
            && other.source != self.source
            && !self.supplemented_by.contains(&other.source)
        {
            self.supplemented_by.push(other.source);
        }
        filled
    }
}

/// Collects a [`History`] as provider responses are parsed.
#[derive(Clone, Debug)]
pub struct HistoryBuilder {
    source: ProviderId,
    location: Option<ResolvedLocation>,
    kind: HistoryKind,
    station: Option<Station>,
    records_station: Option<Station>,
    days: Vec<PastDay>,
    climate: Vec<DayClimate>,
    period: Option<YearRange>,
    normals: Option<Normals>,
}

impl HistoryBuilder {
    /// An empty builder for `source`.
    pub fn new(source: ProviderId) -> Self {
        Self {
            source,
            location: None,
            kind: HistoryKind::Station,
            station: None,
            records_station: None,
            days: Vec::new(),
            climate: Vec::new(),
            period: None,
            normals: None,
        }
    }

    /// The source.
    pub fn source(&self) -> ProviderId {
        self.source
    }

    /// The location so far.
    pub fn location(&self) -> Option<&ResolvedLocation> {
        self.location.as_ref()
    }

    /// Sets the location.
    pub fn set_location(&mut self, location: ResolvedLocation) {
        self.location = Some(location);
    }

    /// Sets whether this is a station's record or a reanalysis.
    pub fn set_kind(&mut self, kind: HistoryKind) {
        self.kind = kind;
    }

    /// Sets the station that measured the days and normals.
    pub fn set_station(&mut self, station: Station) {
        self.station = Some(station);
    }

    /// Sets the station the records come from.
    pub fn set_records_station(&mut self, station: Station) {
        self.records_station = Some(station);
    }

    /// Sets the years the climate covers.
    pub fn set_period(&mut self, period: YearRange) {
        self.period = Some(period);
    }

    /// Keeps normals the source computed for every date, for the caller to cache.
    pub fn set_normals(&mut self, normals: Normals) {
        self.normals = Some(normals);
    }

    /// The observed day `date`, added if new.
    pub fn day_mut(&mut self, date: LocalDate) -> &mut PastDay {
        let i = if let Some(i) = self.days.iter().position(|x| x.date == date) {
            i
        } else {
            self.days.push(PastDay::new(date));
            self.days.len() - 1
        };
        &mut self.days[i]
    }

    /// The climate of `date`, added if new.
    pub fn climate_mut(&mut self, date: LocalDate) -> &mut DayClimate {
        let i = if let Some(i) = self.climate.iter().position(|x| x.date == date) {
            i
        } else {
            self.climate.push(DayClimate::new(date));
            self.climate.len() - 1
        };
        &mut self.climate[i]
    }

    /// Discards everything collected, keeping the source (used when falling back).
    pub fn reset(&mut self) {
        *self = Self::new(self.source);
    }

    /// The history, days and dates ascending. Non-finite values become `None`.
    pub fn finish(self, fetched_at: UnixTime) -> Result<History, ModelError> {
        let location = self.location.ok_or(ModelError::MissingLocation)?;
        let mut days = self.days;
        days.sort_by_key(|d| d.date);
        for d in &mut days {
            d.temp_max = d.temp_max.filter(|c| c.0.is_finite());
            d.temp_min = d.temp_min.filter(|c| c.0.is_finite());
            d.precip_sum = d.precip_sum.filter(|m| m.0.is_finite());
            d.snow_sum = d.snow_sum.filter(|m| m.0.is_finite());
        }
        let mut climate = self.climate;
        climate.sort_by_key(|c| c.date);
        for c in &mut climate {
            c.normal_max = c.normal_max.filter(|v| v.0.is_finite());
            c.normal_min = c.normal_min.filter(|v| v.0.is_finite());
            c.normal_precip = c.normal_precip.filter(|v| v.0.is_finite());
            c.precip_chance = c.precip_chance.filter(|v| v.0.is_finite());
            for r in [
                &mut c.record_max,
                &mut c.record_min,
                &mut c.record_low_max,
                &mut c.record_high_min,
                &mut c.record_precip,
            ] {
                *r = r.filter(|r| r.value.is_finite());
            }
        }
        Ok(History {
            location,
            source: self.source,
            supplemented_by: Vec::new(),
            fetched_at,
            kind: self.kind,
            station: self.station,
            records_station: self.records_station,
            days,
            climate,
            period: self.period,
            normals: self.normals,
        })
    }
}

/// `"1991–2020"`.
pub fn period_text(period: YearRange) -> String {
    alloc::format!("{}–{}", period.first, period.last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u8) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 10,
            day,
        }
    }

    fn built(source: ProviderId, f: impl FnOnce(&mut HistoryBuilder)) -> History {
        let mut b = HistoryBuilder::new(source);
        b.set_location(ResolvedLocation::default());
        f(&mut b);
        b.finish(UnixTime(0)).unwrap()
    }

    #[test]
    fn days_and_dates_come_out_ascending_and_finite() {
        let h = built(ProviderId::Acis, |b| {
            b.day_mut(date(4)).temp_max = Some(Celsius(20.0));
            b.day_mut(date(3)).temp_max = Some(Celsius(f32::NAN));
            b.day_mut(date(4)).temp_min = Some(Celsius(9.0));
            b.climate_mut(date(6)).normal_max = Some(Celsius(19.4));
            b.climate_mut(date(5)).record_max = Some(Record {
                value: f32::INFINITY,
                year: 1938,
            });
        });
        assert_eq!(
            h.days.iter().map(|d| d.date.day).collect::<Vec<_>>(),
            [3, 4]
        );
        assert_eq!(h.day(date(3)).unwrap().temp_max, None);
        assert_eq!(h.day(date(4)).unwrap().temp_min, Some(Celsius(9.0)));
        assert_eq!(
            h.climate.iter().map(|c| c.date.day).collect::<Vec<_>>(),
            [5, 6]
        );
        assert_eq!(h.climate_on(date(5)).unwrap().record_max, None);
        assert!(
            HistoryBuilder::new(ProviderId::Acis)
                .finish(UnixTime(0))
                .is_err()
        );
    }

    #[test]
    fn gaps_are_filled_and_the_filler_credited() {
        let mut station = built(ProviderId::Acis, |b| {
            b.day_mut(date(4)).temp_max = Some(Celsius(20.0));
            b.climate_mut(date(5)).normal_max = Some(Celsius(19.4));
        });
        let grid = built(ProviderId::OpenMeteo, |b| {
            b.set_period(YearRange::NORMALS_1991_2020);
            let d = b.day_mut(date(4));
            d.temp_max = Some(Celsius(25.0));
            d.temp_min = Some(Celsius(8.0));
            b.day_mut(date(3)).temp_max = Some(Celsius(18.0));
            let c = b.climate_mut(date(5));
            c.normal_max = Some(Celsius(30.0));
            c.normal_min = Some(Celsius(9.0));
        });
        assert_eq!(station.fill_gaps_from(&grid), 3);
        let four = station.day(date(4)).unwrap();
        assert_eq!(
            (four.temp_max, four.temp_min),
            (Some(Celsius(20.0)), Some(Celsius(8.0)))
        );
        assert_eq!(station.days[0].date, date(3));
        let five = station.climate_on(date(5)).unwrap();
        assert_eq!(
            (five.normal_max, five.normal_min),
            (Some(Celsius(19.4)), Some(Celsius(9.0)))
        );
        assert_eq!(station.supplemented_by, [ProviderId::OpenMeteo]);
        assert_eq!(station.period, Some(YearRange::NORMALS_1991_2020));
        assert_eq!(station.fill_gaps_from(&grid), 0, "nothing left to fill");
    }

    /// 30 years of a site where the high is `20 + day of year / 100` °C, the
    /// low 10 °C, and it rains 1 mm on every third day.
    fn thirty_years(skip: impl Fn(i32) -> bool) -> Vec<DailyValues> {
        let mut out = Vec::new();
        for year in 1991..=2020 {
            if skip(year) {
                continue;
            }
            let first = crate::time::days_from_civil(year, 1, 1);
            let last = crate::time::days_from_civil(year, 12, 31);
            for d in first..=last {
                let (y, month, day) = crate::time::civil_from_days(d);
                #[allow(clippy::cast_precision_loss)]
                let doy = (d - first) as f32;
                let wet = d % 3 == 0;
                out.push((
                    LocalDate {
                        year: y,
                        month,
                        day,
                    },
                    Some(20.0 + doy / 100.0),
                    Some(10.0),
                    Some(if wet { 1.0 } else { 0.0 }),
                ));
            }
        }
        out
    }

    #[test]
    fn normals_pool_fifteen_days_of_thirty_years() {
        let n = Normals::from_daily(
            YearRange::NORMALS_1991_2020,
            ProviderId::OpenMeteo,
            &thirty_years(|_| false),
        );
        assert_eq!(n.days.len(), 366);
        let july_1 = n
            .on(LocalDate {
                year: 2027,
                month: 7,
                day: 1,
            })
            .unwrap();
        // The high rises 0.01 °C a day, so the pooled mean is the day's own.
        assert!(
            (july_1.max.unwrap().0 - (20.0 + 1.81)).abs() < 0.01,
            "{july_1:?}"
        );
        assert_eq!(july_1.min, Some(Celsius(10.0)));
        assert!((july_1.precip.unwrap().0 - 1.0 / 3.0).abs() < 0.01);
        assert!((july_1.precip_chance.unwrap().0 - 100.0 / 3.0).abs() < 1.0);
        // 29 February pools its neighbours of every year, not just leap years.
        let feb_29 = n
            .on(LocalDate {
                year: 2028,
                month: 2,
                day: 29,
            })
            .unwrap();
        assert!((feb_29.max.unwrap().0 - 20.59).abs() < 0.02, "{feb_29:?}");
        assert!(
            n.on(LocalDate {
                year: 2027,
                month: 2,
                day: 30
            })
            .is_none()
        );
    }

    #[test]
    fn missing_years_still_give_normals_but_too_few_values_do_not() {
        let most = Normals::from_daily(
            YearRange::NORMALS_1991_2020,
            ProviderId::OpenMeteo,
            &thirty_years(|y| y % 2 == 0),
        );
        assert!(
            most.days.iter().all(|d| d.max.is_some()),
            "15 years × 15 days"
        );
        let one = Normals::from_daily(
            YearRange::NORMALS_1991_2020,
            ProviderId::OpenMeteo,
            &thirty_years(|y| y != 2000),
        );
        assert!(
            one.days.iter().all(|d| d.max.is_none()),
            "15 values are too few"
        );
        let outside = Normals::from_daily(
            YearRange {
                first: 2021,
                last: 2025,
            },
            ProviderId::OpenMeteo,
            &thirty_years(|_| false),
        );
        assert!(
            outside.days.iter().all(|d| d.max.is_none()),
            "only the period counts"
        );
    }

    #[test]
    fn normals_fill_only_what_is_missing() {
        let n = Normals::from_daily(
            YearRange::NORMALS_1991_2020,
            ProviderId::OpenMeteo,
            &thirty_years(|_| false),
        );
        let mut h = built(ProviderId::Acis, |b| {
            b.climate_mut(date(17)).normal_max = Some(Celsius(19.4));
        });
        h.fill_normals(&n, [date(17), date(18)]);
        let seventeen = h.climate_on(date(17)).unwrap();
        assert_eq!(
            seventeen.normal_max,
            Some(Celsius(19.4)),
            "the station's stays"
        );
        assert_eq!(seventeen.normal_min, Some(Celsius(10.0)));
        assert!(h.climate_on(date(18)).unwrap().precip_chance.is_some());
        assert_eq!(h.period, Some(YearRange::NORMALS_1991_2020));
    }

    #[test]
    fn periods_read_as_years() {
        assert_eq!(period_text(YearRange::NORMALS_1991_2020), "1991–2020");
    }

    fn on(year: i32, month: u8, day: u8) -> LocalDate {
        LocalDate { year, month, day }
    }

    const YEARS: YearRange = YearRange {
        first: 1995,
        last: 2024,
    };

    #[test]
    fn queries_are_real_dates_up_to_a_month() {
        let q = ClimateQuery::new(on(2026, 12, 30), on(2027, 1, 2), YEARS).unwrap();
        assert_eq!(
            q.dates(),
            [
                on(2026, 12, 30),
                on(2026, 12, 31),
                on(2027, 1, 1),
                on(2027, 1, 2)
            ]
        );
        let err = |from, to, years| ClimateQuery::new(from, to, years).unwrap_err();
        assert_eq!(
            err(on(2027, 2, 29), on(2027, 3, 1), YEARS),
            QueryError::Date
        );
        assert_eq!(
            err(on(2027, 7, 19), on(2027, 7, 12), YEARS),
            QueryError::Backwards
        );
        assert_eq!(
            err(on(2027, 7, 1), on(2027, 8, 1), YEARS),
            QueryError::TooLong
        );
        assert!(ClimateQuery::new(on(2027, 7, 1), on(2027, 7, 31), YEARS).is_ok());
        let backwards = YearRange {
            first: 2024,
            last: 1995,
        };
        assert_eq!(
            err(on(2027, 7, 1), on(2027, 7, 2), backwards),
            QueryError::Years
        );
    }

    /// Every day of 1990–2025: the high is 25 °C plus a tenth of a degree a
    /// year since 1995, the low 15 °C minus as much, and it rains 2 mm in
    /// even years.
    fn daily() -> Vec<DailyValues> {
        let (first, last) = (
            crate::time::days_from_civil(1990, 1, 1),
            crate::time::days_from_civil(2025, 12, 31),
        );
        (first..=last)
            .map(|d| {
                let (year, month, day) = crate::time::civil_from_days(d);
                #[allow(clippy::cast_precision_loss)]
                let t = (year - 1995) as f32 / 10.0;
                let rain = if year % 2 == 0 { 2.0 } else { 0.0 };
                (
                    LocalDate { year, month, day },
                    Some(25.0 + t),
                    Some(15.0 - t),
                    Some(rain),
                )
            })
            .collect()
    }

    #[test]
    fn climate_over_the_chosen_years() {
        let q = ClimateQuery::new(on(2027, 7, 12), on(2027, 7, 19), YEARS).unwrap();
        let days = climate_from_daily(&q, &daily());
        assert_eq!(days.len(), 8);
        let c = days[0];
        assert_eq!(c.date, on(2027, 7, 12));
        // Means over 1995–2024 only: 25 + 1.45, 15 − 1.45.
        assert!((c.normal_max.unwrap().0 - 26.45).abs() < 0.01, "{c:?}");
        assert!((c.normal_min.unwrap().0 - 13.55).abs() < 0.01);
        assert!((c.normal_precip.unwrap().0 - 1.0).abs() < 0.01);
        assert!((c.precip_chance.unwrap().0 - 50.0).abs() < 0.01);
        let year = |r: Option<Record>| r.unwrap().year;
        assert_eq!(year(c.record_max), 2024);
        assert_eq!(year(c.record_low_max), 1995);
        assert_eq!(year(c.record_min), 2024);
        assert_eq!(year(c.record_high_min), 1995);
        assert_eq!(year(c.record_precip), 2024, "ties go to the latest");
        assert!((c.record_max.unwrap().value - 27.9).abs() < 0.01);
    }

    #[test]
    fn leap_days_and_thin_years() {
        // 29 February: itself in leap years, 28 February in the others.
        let short = YearRange {
            first: 2021,
            last: 2024,
        };
        let q = ClimateQuery::new(on(2028, 2, 28), on(2028, 3, 1), short).unwrap();
        let days = climate_from_daily(&q, &daily());
        let values = |c: &DayClimate| c.precip_chance.unwrap().0;
        // 2021–2023 from the 28th and 2024 from the 29th: wet in 2022 and 2024.
        assert!((values(&days[1]) - 50.0).abs() < 0.01);
        assert_eq!(days[1].record_max.unwrap().year, 2024);
        // Fewer years than half the range: nothing.
        let thin: Vec<DailyValues> = daily()
            .into_iter()
            .filter(|(d, ..)| d.year == 2024)
            .collect();
        assert_eq!(climate_from_daily(&q, &thin)[0].normal_max, None);
        // Nothing for those years: nothing at all.
        let none = climate_from_daily(&q, &[]);
        assert_eq!(none[0], DayClimate::new(on(2028, 2, 28)));
    }
}
