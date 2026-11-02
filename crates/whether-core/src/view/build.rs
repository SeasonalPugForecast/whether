//! Builds the [`ViewModel`] from snapshots and presentation settings.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::condition::Condition;
use crate::derive::{current_hour_index, dew_point, pressure_trend, today_index};
use crate::history::{DayClimate, History};
use crate::icon::IconId;
use crate::model::{Daily, Hourly, WeatherSnapshot};
use crate::presentation::{AqiScalePref, DailyIcon, DetailKind, TemperatureBasis, WidgetKind};
use crate::scales::{
    AqiCategory, AqiScale, TempLevel, compass16, eu_aqi_category, moon_phase, owm_aqi_category,
    us_aqi_category, uv_level, wind_arrow,
};
use crate::strings::{
    Text, aqi_text, detail_title, moon_phase_text, trend_text, uv_text, weekday_text,
};
use crate::theme::{SkyTheme, sky_theme};
use crate::time::{LocalDate, UnixTime, UtcOffset, civil_from_days, format_hm, weekday_from_days};
use crate::units::{Celsius, Degrees, Mps, Percent};
use crate::view::format::{Formatter, number};
use crate::view::summary::{NOWCAST_MIN_RATE, day_precip_sentence, nowcast_sentence, summary};
use crate::view::{
    AirQualityVm, AlertVm, CurrentVm, DailyVm, DayVm, DetailVm, HourVm, HourlyVm, LocationHeaderVm,
    LocationView, MoonVm, NowcastTickVm, NowcastVm, PrecipHourVm, PrecipVm, RiseSetVm, StatusInfo,
    StatusVm, SunEventVm, SunVm, TabVm, TimelineGroup, TimelineSpanVm, TwilightKind, ViewInput,
    ViewModel, WindVm,
};

/// The lit share of the moon's disc in percent, from its phase (0 new, 0.5
/// full).
fn moon_illumination(phase: f32) -> f32 {
    (1.0 - libm::cosf(phase * core::f32::consts::TAU)) / 2.0 * 100.0
}

/// The timeline's kind of weather for a condition.
fn timeline_group(c: Condition) -> TimelineGroup {
    use crate::condition::Intensity::Heavy;
    match c {
        Condition::Clear | Condition::MainlyClear => TimelineGroup::Clear,
        Condition::PartlyCloudy => TimelineGroup::PartlyCloudy,
        Condition::Cloudy | Condition::Overcast | Condition::Unknown => TimelineGroup::Cloudy,
        Condition::Fog | Condition::Haze | Condition::Smoke | Condition::Dust => TimelineGroup::Fog,
        Condition::Drizzle(_) | Condition::FreezingDrizzle(_) => TimelineGroup::Drizzle,
        Condition::Rain(Heavy) | Condition::RainShowers(Heavy) | Condition::FreezingRain(Heavy) => {
            TimelineGroup::HeavyRain
        }
        Condition::Rain(_) | Condition::RainShowers(_) | Condition::FreezingRain(_) => {
            TimelineGroup::Rain
        }
        Condition::Sleet(_)
        | Condition::SleetShowers(_)
        | Condition::Snow(_)
        | Condition::SnowShowers(_)
        | Condition::SnowGrains => TimelineGroup::Snow,
        Condition::Thunderstorm(_)
        | Condition::ThunderstormHail(_)
        | Condition::Squall
        | Condition::Tornado => TimelineGroup::Storm,
    }
}

/// A rate in mm an hour on Dark Sky's nowcast scale: thirds for light
/// (to 2.5), moderate (to 7.6) and heavy (to 50).
fn nowcast_intensity(rate: f32) -> f32 {
    const BANDS: [(f32, f32); 3] = [(0.0, 2.5), (2.5, 7.6), (7.6, 50.0)];
    if rate <= 0.0 {
        return 0.0;
    }
    for (i, (lo, hi)) in BANDS.iter().enumerate() {
        if rate < *hi {
            return (i as f32 + (rate - lo) / (hi - lo)) / 3.0;
        }
    }
    1.0
}

/// Builds the view model for `input.active`. Never panics.
pub fn build_view(input: &ViewInput<'_>) -> ViewModel {
    let f = Formatter::new(input.presentation, input.strings);
    let tabs = input.locations.iter().map(|l| tab(&f, l)).collect();
    let active = input.active.min(input.locations.len().saturating_sub(1));
    let mut vm = ViewModel {
        tabs,
        active,
        location: LocationHeaderVm::default(),
        status: StatusVm::default(),
        attribution: String::new(),
        theme: SkyTheme::ClearDay,
        is_day: true,
        feels_like_leads: input.presentation.temperature_basis == TemperatureBasis::FeelsLike,
        widgets: Vec::new(),
        current: None,
        summary: None,
        hourly: None,
        daily: None,
        details: Vec::new(),
        air_quality: None,
        sun: None,
        moon: None,
        nowcast: None,
        alerts: Vec::new(),
        radar: None,
        precipitation: None,
        history: None,
    };
    let Some(loc) = input.locations.get(active) else {
        return vm;
    };
    vm.location.name = loc.name.to_string();
    vm.status = status(&f, loc.status, loc.snapshot, input.now);
    if let Some(snap) = loc.snapshot {
        Builder::new(f, snap, input.now, loc.history).fill(&mut vm);
    } else if let Some(h) = loc.history
        && input.presentation.shows(WidgetKind::History)
    {
        // History without a forecast (`whether history`, or a failed fetch):
        // the place's own day, and no forecast high to compare with yesterday.
        let today = input.now.local_date(h.location.utc_offset);
        vm.history = Some(super::history::history(&f, h, today, None));
        vm.widgets = alloc::vec![WidgetKind::History];
        vm.attribution = super::history::attribution(h);
    }
    vm
}

fn tab(f: &Formatter<'_>, loc: &LocationView<'_>) -> TabVm {
    let current = loc.snapshot.and_then(|s| s.current.as_ref());
    TabVm {
        name: loc.name.to_string(),
        temp: f
            .temperature(current.and_then(|c| f.lead(c.temperature, c.apparent)))
            .text,
        icon: current.map_or(IconId::NotAvailable, |c| c.condition.icon(c.is_day)),
    }
}

fn status(
    f: &Formatter<'_>,
    info: &StatusInfo,
    snap: Option<&WeatherSnapshot>,
    now: UnixTime,
) -> StatusVm {
    let s = f.strings;
    let updated = match info.last_success {
        Some(t) => f.updated(t, now),
        None if info.refreshing => s.get(Text::Fetching).to_string(),
        None => s.get(Text::NoData).to_string(),
    };
    StatusVm {
        updated,
        provider: snap.map(provider_label).unwrap_or_default(),
        stale: info.stale,
        refreshing: info.refreshing,
        error: info.error.clone(),
        observed: snap.and_then(|snap| observed(f, snap)),
    }
}

/// "Observed at <station>, <time>" when a station measured the current
/// conditions.
fn observed(f: &Formatter<'_>, snap: &WeatherSnapshot) -> Option<String> {
    let station = snap.location.station.as_ref()?;
    let time = snap.current.as_ref()?.time;
    Some(f.strings.fmt(
        Text::ObservedAt,
        &[
            ("station", &station.name),
            ("time", &f.clock(time, snap.offset_at(time))),
        ],
    ))
}

/// The primary provider, followed by the models that supplied its forecast
/// ("Open-Meteo · HRRR → NBM → ECMWF").
fn provider_label(snap: &WeatherSnapshot) -> String {
    let mut label = snap.provider.display_name().to_string();
    for (i, id) in snap.models.iter().enumerate() {
        label.push_str(if i == 0 { " · " } else { " → " });
        label.push_str(crate::models::short_name(id));
    }
    label
}

fn round_u8(x: f32) -> u8 {
    libm::roundf(x).clamp(0.0, 255.0) as u8
}

struct Builder<'a> {
    f: Formatter<'a>,
    snap: &'a WeatherSnapshot,
    now: UnixTime,
    /// The offset now; other times use [`Self::at`] (DST).
    offset: UtcOffset,
    today: Option<&'a Daily>,
    /// The location's history, for the history widget, day normals and the
    /// normals and records cards.
    history: Option<&'a History>,
}

impl<'a> Builder<'a> {
    fn new(
        f: Formatter<'a>,
        snap: &'a WeatherSnapshot,
        now: UnixTime,
        history: Option<&'a History>,
    ) -> Self {
        let offset = snap.offset_at(now);
        let today = today_index(&snap.daily, now, offset).map(|i| &snap.daily[i]);
        Self {
            f,
            snap,
            now,
            offset,
            today,
            history,
        }
    }

    /// The climate of `date`, when the history has it.
    fn climate(&self, date: LocalDate) -> Option<&'a DayClimate> {
        self.history?.climate_on(date)
    }

    /// Whether anything shown comes from the history, so it is credited.
    fn shows_history(&self) -> bool {
        let p = self.f.presentation;
        self.history.is_some()
            && (p.shows(WidgetKind::History)
                || p.shows(WidgetKind::Daily)
                || (p.shows(WidgetKind::Details)
                    && (p.details.contains(&DetailKind::Normals)
                        || p.details.contains(&DetailKind::Records))))
    }

    /// The UTC offset local times use at `t`.
    fn at(&self, t: UnixTime) -> UtcOffset {
        self.snap.offset_at(t)
    }

    fn shows(&self, w: WidgetKind) -> bool {
        self.f.presentation.shows(w)
    }

    fn fill(&self, vm: &mut ViewModel) {
        let s = self.f.strings;
        let local = self.now.to_local(self.offset);
        vm.location.region.clone_from(&self.snap.location.region);
        vm.location.local_time = format!(
            "{} {}",
            s.get(weekday_text(local.weekday)),
            self.f.clock(self.now, self.offset)
        );
        vm.attribution = self.attribution();

        let current_hour =
            current_hour_index(&self.snap.hourly, self.now).map(|i| &self.snap.hourly[i]);
        vm.is_day = self
            .snap
            .current
            .as_ref()
            .map(|c| c.is_day)
            .or(current_hour.map(|h| h.is_day))
            .unwrap_or(true);
        let condition = self
            .snap
            .current
            .as_ref()
            .map(|c| c.condition)
            .or(current_hour.map(|h| h.condition))
            .unwrap_or_default();
        vm.theme = sky_theme(condition, vm.is_day, self.twilight());

        if self.shows(WidgetKind::Current) {
            vm.current = self.current();
        }
        if self.shows(WidgetKind::Summary) {
            vm.summary = summary(&self.f, self.snap, self.today, self.now);
        }
        if self.shows(WidgetKind::Hourly) {
            vm.hourly = self.hourly();
        }
        if self.shows(WidgetKind::Daily) {
            vm.daily = self.daily();
        }
        if self.shows(WidgetKind::Details) {
            vm.details = self.details();
        }
        if self.shows(WidgetKind::AirQuality) {
            vm.air_quality = self.air_quality();
        }
        if self.shows(WidgetKind::Sun) {
            vm.sun = self.sun(vm.is_day);
        }
        if self.shows(WidgetKind::Moon) {
            vm.moon = self.moon();
        }
        if self.shows(WidgetKind::Nowcast) {
            vm.nowcast = self.nowcast();
        }
        if self.shows(WidgetKind::Alerts) {
            vm.alerts = self.alerts();
        }
        if self.shows(WidgetKind::Precipitation) {
            vm.precipitation = self.precipitation();
        }
        if self.shows(WidgetKind::History)
            && let Some(h) = self.history
        {
            let today = self.now.local_date(self.offset);
            let high = self.today.and_then(|d| d.temp_max);
            vm.history = Some(super::history::history(&self.f, h, today, high));
        }
        vm.widgets = self
            .f
            .presentation
            .widgets
            .iter()
            .copied()
            .filter(|w| match w {
                WidgetKind::Current => vm.current.is_some(),
                WidgetKind::Summary => vm.summary.is_some(),
                WidgetKind::Hourly => vm.hourly.is_some(),
                WidgetKind::Daily => vm.daily.is_some(),
                WidgetKind::Details => !vm.details.is_empty(),
                WidgetKind::AirQuality => vm.air_quality.is_some(),
                WidgetKind::Sun => vm.sun.is_some(),
                WidgetKind::Moon => vm.moon.is_some(),
                WidgetKind::Nowcast => vm.nowcast.is_some(),
                WidgetKind::Alerts => !vm.alerts.is_empty(),
                // The imagery arrives beside the snapshot; `view_for` drops the widget
                // when there is none.
                WidgetKind::Radar => true,
                WidgetKind::Precipitation => vm.precipitation.is_some(),
                WidgetKind::History => vm.history.is_some(),
            })
            .collect();
    }

    fn attribution(&self) -> String {
        let mut out = String::from(self.snap.provider.attribution());
        let mut credits: Vec<&str> = self
            .snap
            .supplemented_by
            .iter()
            .map(|p| p.short_credit())
            .collect();
        if let Some(h) = self.history.filter(|_| self.shows_history()) {
            for p in core::iter::once(&h.source).chain(&h.supplemented_by) {
                let credit = p.short_credit();
                if *p != self.snap.provider && !credits.contains(&credit) {
                    credits.push(credit);
                }
            }
        }
        for credit in credits {
            out.push_str(" · ");
            out.push_str(credit);
        }
        out
    }

    fn twilight(&self) -> bool {
        let window = i64::from(self.f.presentation.twilight_minutes) * 60;
        let near = |t: Option<UnixTime>| t.is_some_and(|t| (self.now.0 - t.0).abs() <= window);
        self.today
            .is_some_and(|d| near(d.sunrise) || near(d.sunset))
    }

    fn temp_level(c: Option<Celsius>) -> TempLevel {
        c.map_or(TempLevel::Mild, |c| TempLevel::from_celsius(c.0))
    }

    fn wind(&self, speed: Option<Mps>, gust: Option<Mps>, dir: Option<Degrees>) -> WindVm {
        WindVm {
            speed: self.f.wind(speed),
            gust: self.f.wind(gust),
            direction_deg: dir.map(|d| d.0),
            compass: dir.map_or("", |d| compass16(d.0)),
            arrow: dir.map_or(' ', |d| wind_arrow(d.0)),
        }
    }

    fn current(&self) -> Option<CurrentVm> {
        let c = self.snap.current.as_ref()?;
        let shown = self.f.lead(c.temperature, c.apparent);
        let feels_like = self.f.temperature(c.apparent);
        // Beside the big number: the other temperature.
        let leads = self.f.presentation.temperature_basis == TemperatureBasis::FeelsLike;
        let feels_like_label = if leads && c.apparent.is_some() {
            let actual = self.f.temperature(c.temperature);
            self.f
                .strings
                .fmt(Text::ActualTemp, &[("temp", &actual.text)])
        } else {
            self.f
                .strings
                .fmt(Text::FeelsLike, &[("temp", &feels_like.text)])
        };
        let today = |actual: fn(&Daily) -> Option<Celsius>,
                     feels: fn(&Daily) -> Option<Celsius>| {
            self.f
                .temperature(self.today.and_then(|d| self.f.lead(actual(d), feels(d))))
        };
        Some(CurrentVm {
            temperature: self.f.temperature(shown),
            temp_level: Self::temp_level(shown),
            feels_like,
            feels_like_label,
            high: today(|d| d.temp_max, |d| d.apparent_max),
            low: today(|d| d.temp_min, |d| d.apparent_min),
            condition: c.condition,
            condition_label: self.f.strings.get(c.condition.label()).to_string(),
            icon: c.condition.icon(c.is_day),
        })
    }

    fn hour(&self, h: &Hourly, is_now: bool) -> HourVm {
        let label = if is_now {
            self.f.strings.get(Text::Now).to_string()
        } else {
            self.f.hour_label(h.time, self.at(h.time))
        };
        let date = h.time.local_date(self.at(h.time));
        HourVm {
            time: h.time,
            label,
            day: self.relative_day_label(date),
            date_short: self.f.date(date),
            icon: h.condition.icon(h.is_day),
            is_day: h.is_day,
            temperature: self.f.temperature(self.f.lead(h.temperature, h.apparent)),
            temp_level: Self::temp_level(self.f.lead(h.temperature, h.apparent)),
            feels_like: self.f.temperature(h.apparent),
            actual: self.f.temperature(h.temperature),
            precip_probability: h.precip_probability.map(|p| round_u8(p.0)),
            precipitation: self.f.precipitation(h.precipitation),
            wind: self.wind(h.wind_speed, h.wind_gust, h.wind_dir),
            humidity: h.humidity.map(|p| round_u8(p.0)),
            uv: h.uv_index,
            humidity_text: h
                .humidity
                .map(|p| crate::view::format::chance(round_u8(p.0))),
            condition_label: self.f.strings.get(h.condition.label()).to_string(),
            dew_point: self.f.temperature(
                h.temperature
                    .zip(h.humidity)
                    .and_then(|(t, rh)| crate::derive::dew_point(t, rh)),
            ),
            pressure: self.f.pressure(h.pressure_msl),
            visibility: self.f.distance(h.visibility),
            cloud_cover: self.f.percent(h.cloud_cover),
            uv_text: h.uv_index.map(|uv| number(uv, 0)),
        }
    }

    fn precipitation(&self) -> Option<PrecipVm> {
        /// Heavy rain, in millimetres an hour: the top of the intensity scale.
        const HEAVY_MM_PER_HOUR: f32 = 7.6;
        /// Below this chance, an hour without an amount counts as dry.
        const DRY_CHANCE: u8 = 15;
        let all = &self.snap.hourly;
        let current = current_hour_index(all, self.now);
        let start = current.unwrap_or_else(|| all.partition_point(|h| h.time < self.now));
        let take = usize::from(self.f.presentation.precip_hours.clamp(1, 48));
        let rows: Vec<&Hourly> = all.get(start..)?.iter().take(take).collect();
        if rows.is_empty() {
            return None;
        }
        let mut total = 0.0;
        let mut any_amount = false;
        let hours: Vec<PrecipHourVm> = rows
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let step_hours = rows
                    .get(i + 1)
                    .map_or(1.0, |n| (n.time.0 - h.time.0) as f32 / 3_600.0)
                    .max(0.25);
                let mm = h.precipitation.map(|p| p.0);
                if let Some(mm) = mm {
                    total += mm;
                    any_amount = true;
                }
                let chance_value = h.precip_probability.map(|p| round_u8(p.0));
                PrecipHourVm {
                    time: h.time,
                    label: if i == 0 && current.is_some() {
                        self.f.strings.get(Text::Now).to_string()
                    } else {
                        self.f.hour_label(h.time, self.at(h.time))
                    },
                    chance: chance_value.map(crate::view::format::chance),
                    chance_value,
                    amount: self.f.precipitation(h.precipitation),
                    intensity: mm.map_or(0.0, |mm| {
                        (mm / step_hours / HEAVY_MM_PER_HOUR).clamp(0.0, 1.0)
                    }),
                    frozen: h.condition.is_frozen(),
                }
            })
            .collect();
        let max_chance = hours.iter().filter_map(|h| h.chance_value).max();
        let dry = total <= 0.0 && max_chance.is_none_or(|c| c < DRY_CHANCE);
        let count = hours.len().to_string();
        Some(PrecipVm {
            dry: dry.then(|| self.f.strings.fmt(Text::PrecipDryHours, &[("n", &count)])),
            hours,
            total: self
                .f
                .precipitation(any_amount.then_some(crate::units::Mm(total))),
            max_chance: max_chance.map(crate::view::format::chance),
        })
    }

    fn hourly(&self) -> Option<HourlyVm> {
        let all = &self.snap.hourly;
        let current = current_hour_index(all, self.now);
        let start = current.unwrap_or_else(|| all.partition_point(|h| h.time < self.now));
        let take = usize::from(self.f.presentation.hourly_hours.max(1));
        let shown: Vec<&Hourly> = all.get(start..)?.iter().take(take).collect();
        let hours: Vec<HourVm> = shown
            .iter()
            .enumerate()
            .map(|(i, h)| self.hour(h, i == 0 && current.is_some()))
            .collect();
        let timeline = self.timeline(&shown[..shown.len().min(24)]);
        if hours.is_empty() {
            return None;
        }
        let temps = hours.iter().filter_map(|h| h.temperature.value);
        let temp_min = temps.clone().fold(f32::INFINITY, f32::min);
        let temp_max = temps.fold(f32::NEG_INFINITY, f32::max);
        let precip_max = hours
            .iter()
            .filter_map(|h| h.precipitation.value)
            .fold(0.0, f32::max);
        let (temp_min, temp_max) = if temp_min.is_finite() {
            (temp_min, temp_max)
        } else {
            (0.0, 0.0)
        };
        Some(HourlyVm {
            hours,
            temp_min,
            temp_max,
            precip_max,
            now_index: 0,
            timeline,
        })
    }

    /// `hours` as stretches of one kind of weather, each labelled with its most
    /// severe condition.
    fn timeline(&self, hours: &[&Hourly]) -> Vec<TimelineSpanVm> {
        let mut spans: Vec<(usize, usize, TimelineGroup, Condition)> = Vec::new();
        for (i, h) in hours.iter().enumerate() {
            let group = timeline_group(h.condition);
            match spans.last_mut() {
                Some((_, len, g, worst)) if *g == group => {
                    *len += 1;
                    if h.condition.severity_rank() > worst.severity_rank() {
                        *worst = h.condition;
                    }
                }
                _ => spans.push((i, 1, group, h.condition)),
            }
        }
        spans
            .into_iter()
            .map(|(start, len, group, worst)| TimelineSpanVm {
                start,
                len,
                group,
                label: self.f.strings.get(worst.label()).to_string(),
            })
            .collect()
    }

    /// The hours of `day`'s local date.
    fn day_hours(&self, day: &Daily) -> Vec<&Hourly> {
        self.snap
            .hourly
            .iter()
            .filter(|h| h.time.local_date(self.at(h.time)) == day.date)
            .collect()
    }

    /// [`Self::day_label`] counted from now.
    fn relative_day_label(&self, date: LocalDate) -> String {
        let today = self.now.local_date(self.offset);
        let (year, month, day) = civil_from_days(today.days_since_epoch() + 1);
        self.day_label(date, today, LocalDate { year, month, day })
    }

    /// "Today", "Tomorrow" or the weekday.
    fn day_label(&self, date: LocalDate, today: LocalDate, tomorrow: LocalDate) -> String {
        let s = self.f.strings;
        if date == today {
            s.get(Text::Today).to_string()
        } else if date == tomorrow {
            s.get(Text::Tomorrow).to_string()
        } else {
            s.get(weekday_text(weekday_from_days(date.days_since_epoch())))
                .to_string()
        }
    }

    /// When a day's rain or snow falls, and the day in a forecaster's words
    /// (from its hours, else from its own values beyond them).
    fn day_texts(&self, d: &Daily) -> (Option<String>, Option<String>) {
        let hours = self.day_hours(d);
        let precip = day_precip_sentence(&self.f, self.snap, &hours);
        let outlook = if hours.is_empty() {
            Some(super::outlook::daily_outlook(&self.f, d))
        } else {
            super::outlook::day_outlook(&self.f, self.snap, &hours, precip.as_deref())
        };
        (precip, outlook)
    }

    fn daily(&self) -> Option<DailyVm> {
        let today: LocalDate = self.now.local_date(self.offset);
        let today_days = today.days_since_epoch();
        let tomorrow = {
            let (year, month, day) = civil_from_days(today_days + 1);
            LocalDate { year, month, day }
        };
        let take = usize::from(self.f.presentation.daily_days.max(1));
        let days: Vec<&Daily> = self
            .snap
            .daily
            .iter()
            .filter(|d| d.date >= today)
            .take(take)
            .collect();
        if days.is_empty() {
            return None;
        }
        let celsius = |c: Option<Celsius>| c.map(|c| c.0);
        let high_of = |d: &Daily| self.f.lead(d.temp_max, d.apparent_max);
        let low_of = |d: &Daily| self.f.lead(d.temp_min, d.apparent_min);
        let lows = days
            .iter()
            .filter_map(|d| celsius(low_of(d)).or(celsius(high_of(d))));
        let highs = days
            .iter()
            .filter_map(|d| celsius(high_of(d)).or(celsius(low_of(d))));
        let min = lows.fold(f32::INFINITY, f32::min);
        let max = highs.fold(f32::NEG_INFINITY, f32::max);
        let span = max - min;
        let frac = |c: Option<f32>, fallback: f32| match c {
            Some(c) if span > 0.0 && span.is_finite() => ((c - min) / span).clamp(0.0, 1.0),
            _ => fallback,
        };
        let s = self.f.strings;
        let rows = days
            .iter()
            .map(|d| {
                let label = self.day_label(d.date, today, tomorrow);
                let condition = match self.f.presentation.daily_icon {
                    DailyIcon::Daylight => crate::derive::daylight_condition(d, &self.snap.hourly)
                        .unwrap_or(d.condition),
                    DailyIcon::Provider => d.condition,
                };
                let normal = self.climate(d.date);
                let (precip_text, outlook) = self.day_texts(d);
                let mut row = DayVm {
                    label,
                    date_short: self.f.date(d.date),
                    icon: condition.icon(true),
                    condition_label: s.get(condition.label()).to_string(),
                    high: self.f.temperature(high_of(d)),
                    low: self.f.temperature(low_of(d)),
                    actual_high: self.f.temperature(d.temp_max),
                    actual_low: self.f.temperature(d.temp_min),
                    temp_level_high: Self::temp_level(high_of(d)),
                    precip_probability: d.precip_probability_max.map(|p| round_u8(p.0)),
                    precip_sum: self.f.precipitation(d.precip_sum),
                    range: (
                        frac(celsius(low_of(d)), 0.5),
                        frac(celsius(high_of(d)), 0.5),
                    ),
                    sunrise: d.sunrise.map(|t| self.f.clock(t, self.at(t))),
                    sunset: d.sunset.map(|t| self.f.clock(t, self.at(t))),
                    uv_max: d.uv_index_max,
                    wind_max: self.f.wind(d.wind_speed_max),
                    gust_max: self.day_gust(d),
                    precip_text,
                    outlook,
                    hours: Vec::new(),
                    timeline: Vec::new(),
                    normal_high: self.f.temperature(normal.and_then(|c| c.normal_max)),
                    normal_low: self.f.temperature(normal.and_then(|c| c.normal_min)),
                };
                if self.f.presentation.daily_hours {
                    let hours = self.day_hours(d);
                    let current = current_hour_index(&self.snap.hourly, self.now)
                        .map(|i| self.snap.hourly[i].time);
                    row.hours = hours
                        .iter()
                        .map(|h| self.hour(h, Some(h.time) == current))
                        .collect();
                    row.timeline = self.timeline(&hours);
                }
                row
            })
            .collect();
        let unit = self.f.presentation.units.temperature;
        let (period_min, period_max) = if min.is_finite() {
            (Celsius(min).to(unit), Celsius(max).to(unit))
        } else {
            (0.0, 0.0)
        };
        Some(DailyVm {
            days: rows,
            period_min,
            period_max,
        })
    }

    /// A day's strongest gust: its own, else its hours' strongest.
    fn day_gust(&self, d: &Daily) -> crate::view::Quantity {
        self.f.wind(d.wind_gust_max.or_else(|| {
            self.day_hours(d)
                .iter()
                .filter_map(|h| h.wind_gust)
                .reduce(|a, b| if b.0 > a.0 { b } else { a })
        }))
    }

    fn details(&self) -> Vec<DetailVm> {
        let Some(c) = self.snap.current.as_ref() else {
            return Vec::new();
        };
        let s = self.f.strings;
        let mut out = Vec::new();
        for &kind in &self.f.presentation.details {
            let card = |value: String| DetailVm {
                kind,
                title: s.get(detail_title(kind)).to_string(),
                value,
                caption: None,
                level: None,
                gauge: None,
                direction_deg: None,
                trend: None,
            };
            let card = match kind {
                DetailKind::Wind => c.wind_speed.map(|v| {
                    let mut value = self.f.wind(Some(v)).text;
                    if let Some(d) = c.wind_dir {
                        value.push(' ');
                        value.push_str(compass16(d.0));
                    }
                    DetailVm {
                        caption: c.wind_gust.map(|g| {
                            s.fmt(Text::GustsCaption, &[("speed", &self.f.wind(Some(g)).text)])
                        }),
                        direction_deg: c.wind_dir.map(|d| d.0),
                        ..card(value)
                    }
                }),
                DetailKind::Gusts => c.wind_gust.map(|g| card(self.f.wind(Some(g)).text)),
                DetailKind::Humidity => c.humidity.map(|h| {
                    let dew = c.dew_point.or_else(|| dew_point(c.temperature?, h));
                    DetailVm {
                        caption: dew.map(|d| {
                            s.fmt(
                                Text::DewPointCaption,
                                &[("temp", &self.f.temperature(Some(d)).text)],
                            )
                        }),
                        gauge: Some((h.0 / 100.0).clamp(0.0, 1.0)),
                        ..card(self.f.percent(Some(h)).text)
                    }
                }),
                DetailKind::DewPoint => c
                    .dew_point
                    .or_else(|| dew_point(c.temperature?, c.humidity?))
                    .map(|d| card(self.f.temperature(Some(d)).text)),
                DetailKind::Pressure => c.pressure_msl.map(|p| {
                    let trend = pressure_trend(&self.snap.hourly, self.now);
                    DetailVm {
                        caption: trend.map(|t| s.get(trend_text(t)).to_string()),
                        gauge: Some(((p.0 - 970.0) / 80.0).clamp(0.0, 1.0)),
                        trend,
                        ..card(self.f.pressure(Some(p)).text)
                    }
                }),
                DetailKind::Uv => c.uv_index.map(|uv| {
                    let category = uv_level(uv);
                    DetailVm {
                        caption: Some(s.get(uv_text(category)).to_string()),
                        level: Some(category.level()),
                        gauge: Some((uv / 11.0).clamp(0.0, 1.0)),
                        ..card(number(uv, 0))
                    }
                }),
                DetailKind::Visibility => c.visibility.map(|v| card(self.f.distance(Some(v)).text)),
                DetailKind::CloudCover => c.cloud_cover.map(|p: Percent| DetailVm {
                    gauge: Some((p.0 / 100.0).clamp(0.0, 1.0)),
                    ..card(self.f.percent(Some(p)).text)
                }),
                DetailKind::Precipitation => c.precipitation.map(|p| DetailVm {
                    caption: self.today.and_then(|d| d.precip_sum).map(|sum| {
                        s.fmt(
                            Text::TodayTotalCaption,
                            &[("amount", &self.f.precipitation(Some(sum)).text)],
                        )
                    }),
                    ..card(self.f.precipitation(Some(p)).text)
                }),
                DetailKind::FeelsLike => c.apparent.map(|a| card(self.f.temperature(Some(a)).text)),
                // Today's climate from the history.
                DetailKind::Normals => self
                    .climate(self.now.local_date(self.offset))
                    .and_then(|c| super::history::normals_value(&self.f, c))
                    .map(card),
                DetailKind::Records => self
                    .climate(self.now.local_date(self.offset))
                    .and_then(|c| super::history::records_value(&self.f, c))
                    .map(|(value, years)| DetailVm {
                        caption: Some(years),
                        ..card(value)
                    }),
            };
            out.extend(card);
        }
        out
    }

    fn air_quality(&self) -> Option<AirQualityVm> {
        let aq = self.snap.air_quality.as_ref()?;
        let us = aq
            .us_aqi
            .map(|v| (AqiScale::Us, v, us_aqi_category(v), f32::from(v) / 300.0));
        let eu = aq.european_aqi.map(|v| {
            (
                AqiScale::European,
                v,
                eu_aqi_category(v),
                f32::from(v) / 100.0,
            )
        });
        let owm = aq.owm_index.and_then(|i| {
            owm_aqi_category(i)
                .map(|c| (AqiScale::Owm, u16::from(i), c, (f32::from(i) - 1.0) / 4.0))
        });
        let chosen: Option<(AqiScale, u16, AqiCategory, f32)> = match self.f.presentation.aqi_scale
        {
            AqiScalePref::Us => us.or(eu),
            AqiScalePref::European => eu.or(us),
        }
        .or(owm);
        let (scale, index, category, gauge) = chosen?;
        let pollutants = [
            ("PM2.5", aq.pm2_5),
            ("PM10", aq.pm10),
            ("O₃", aq.o3),
            ("NO₂", aq.no2),
            ("SO₂", aq.so2),
            ("CO", aq.co),
        ]
        .into_iter()
        .filter_map(|(name, v)| {
            v.map(|v| (name, format!("{} µg/m³", number(v, u8::from(v < 100.0)))))
        })
        .collect();
        Some(AirQualityVm {
            scale,
            index,
            category_label: self.f.strings.get(aqi_text(category.label)).to_string(),
            level: category.level,
            gauge: gauge.clamp(0.0, 1.0),
            pollutants,
        })
    }

    fn sun(&self, is_day: bool) -> Option<SunVm> {
        let d = self.today?;
        let missing = &self.f.presentation.missing;
        let clock = |t: Option<UnixTime>| {
            t.map_or_else(|| missing.clone(), |t| self.f.clock(t, self.at(t)))
        };
        let daylight = d
            .daylight_s
            .or_else(|| u32::try_from(d.sunset?.0 - d.sunrise?.0).ok())
            .map_or_else(|| missing.clone(), format_hm);
        let progress = match (d.sunrise, d.sunset) {
            (Some(rise), Some(set)) if set.0 > rise.0 && (rise.0..set.0).contains(&self.now.0) => {
                Some(((self.now.0 - rise.0) as f32 / (set.0 - rise.0) as f32).clamp(0.0, 1.0))
            }
            _ => None,
        };
        let (latitude, longitude) = (self.snap.location.latitude, self.snap.location.longitude);
        let noon = crate::sun::solar_noon(d.date, longitude);
        Some(SunVm {
            sunrise: clock(d.sunrise),
            sunset: clock(d.sunset),
            daylight,
            progress,
            is_day,
            solar_noon: Some(clock(Some(noon))),
            twilight: self.sun_events(d.date, latitude, longitude, noon),
            rise: self.rise_set(d.sunrise, Text::Sunrise),
            set: self.rise_set(d.sunset, Text::Sunset),
            noon: self.rise_set(Some(noon), Text::SolarNoon),
        })
    }

    /// A sunrise, sunset, moonrise or moonset as the view lists it.
    fn rise_set(&self, at: Option<UnixTime>, text: Text) -> Option<RiseSetVm> {
        let at = at?;
        Some(RiseSetVm {
            label: self.f.strings.get(text).to_string(),
            time: self.f.clock(at, self.at(at)),
            at,
        })
    }

    /// The day's dawns, dusks and golden hours, in time order.
    fn sun_events(
        &self,
        date: LocalDate,
        latitude: f64,
        longitude: f64,
        noon: UnixTime,
    ) -> Vec<SunEventVm> {
        use crate::sun::{ASTRONOMICAL, CIVIL, GOLDEN, NAUTICAL, SunDay, sun_crossing};
        let mut events = Vec::new();
        for (kind, altitude, dawn, dusk) in [
            (
                TwilightKind::Astronomical,
                ASTRONOMICAL,
                Text::AstronomicalDawn,
                Text::AstronomicalDusk,
            ),
            (
                TwilightKind::Nautical,
                NAUTICAL,
                Text::NauticalDawn,
                Text::NauticalDusk,
            ),
            (TwilightKind::Civil, CIVIL, Text::CivilDawn, Text::CivilDusk),
            (
                TwilightKind::Golden,
                GOLDEN,
                Text::GoldenHourEnds,
                Text::GoldenHourBegins,
            ),
        ] {
            if let SunDay::RiseSet { rise, set } = sun_crossing(date, latitude, longitude, altitude)
            {
                for (at, text) in [(rise, dawn), (set, dusk)] {
                    events.push(SunEventVm {
                        label: self.f.strings.get(text).to_string(),
                        time: self.f.clock(at, self.at(at)),
                        at,
                        morning: at.0 < noon.0,
                        kind,
                    });
                }
            }
        }
        events.sort_by_key(|e| e.at.0);
        events
    }

    fn moon(&self) -> Option<MoonVm> {
        let d = self.today?;
        let phase = d.moon_phase?;
        let icon = moon_phase(phase);
        Some(MoonVm {
            phase,
            phase_label: moon_phase_text(icon)
                .map(|t| self.f.strings.get(t).to_string())
                .unwrap_or_default(),
            icon,
            moonrise: d.moonrise.map(|t| self.f.clock(t, self.at(t))),
            moonset: d.moonset.map(|t| self.f.clock(t, self.at(t))),
            illumination: Some(self.f.strings.fmt(
                Text::MoonIlluminated,
                &[("n", &number(moon_illumination(phase), 0))],
            )),
            rise: self.rise_set(d.moonrise, Text::Moonrise),
            set: self.rise_set(d.moonset, Text::Moonset),
        })
    }

    fn nowcast(&self) -> Option<NowcastVm> {
        let hours = self.f.presentation.nowcast_hours.clamp(1, 8);
        let span = i64::from(hours) * 3_600;
        let horizon = self.now.0 + span;
        // Minute data for the one-hour graph when it covers the hour, else the
        // 15-minute steps.
        let minutes = &self.snap.minutely_1;
        let by_minute = hours == 1
            && minutes.first().is_some_and(|m| m.time.0 <= self.now.0 + 60)
            && minutes
                .last()
                .is_some_and(|m| m.time.0 + 60 >= horizon - 60);
        let (m, step_minutes) = if by_minute {
            (minutes, 1_u8)
        } else {
            (&self.snap.minutely_15, 15)
        };
        let step = i64::from(step_minutes) * 60;
        let series_mm: Vec<f32> = m
            .iter()
            .filter(|p| p.time.0 + step > self.now.0 && p.time.0 < horizon)
            .map(|p| p.precipitation.map_or(0.0, |p| p.0))
            .collect();
        if series_mm.is_empty() {
            return None;
        }
        let s = self.f.strings;
        let text = nowcast_sentence(&self.f, self.snap, self.now, span).unwrap_or_else(|| {
            if hours == 1 {
                s.get(Text::NowcastDryHour).to_string()
            } else {
                s.fmt(Text::NowcastDry, &[("n", &hours.to_string())])
            }
        });
        // A tick every half hour up to an hour, every hour up to four, else
        // every two.
        let every_minutes: usize = match hours {
            1 => 30,
            2..=4 => 60,
            _ => 120,
        };
        let ticks = (0..=series_mm.len())
            .step_by(every_minutes / usize::from(step_minutes))
            .map(|step| {
                let minutes = step * usize::from(step_minutes);
                let label = if step == 0 {
                    s.get(Text::Now).to_string()
                } else if minutes < 60 {
                    s.fmt(Text::MinutesShort, &[("n", &minutes.to_string())])
                } else {
                    s.fmt(Text::HoursShort, &[("n", &(minutes / 60).to_string())])
                };
                NowcastTickVm { step, label }
            })
            .collect();
        let per_hour = 60.0 / f32::from(step_minutes);
        Some(NowcastVm {
            text,
            intensity: series_mm
                .iter()
                .map(|mm| nowcast_intensity(mm * per_hour))
                .collect(),
            wet: series_mm.iter().any(|mm| mm * per_hour >= NOWCAST_MIN_RATE),
            series_mm,
            levels: [
                Text::IntensityLight,
                Text::IntensityModerate,
                Text::IntensityHeavy,
            ]
            .map(|t| s.get(t).to_string())
            .to_vec(),
            ticks,
            hours,
            step_minutes,
        })
    }

    fn alerts(&self) -> Vec<AlertVm> {
        let s = self.f.strings;
        let stamp = |t: UnixTime| {
            let offset = self.at(t);
            let l = t.to_local(offset);
            format!(
                "{} {}",
                s.get(weekday_text(l.weekday)),
                self.f.clock(t, offset)
            )
        };
        self.snap
            .alerts
            .iter()
            .filter(|a| a.end > self.now)
            .map(|a| AlertVm {
                id: a.id.clone(),
                title: a.title.clone(),
                body: a.description.clone(),
                level: a.severity,
                // Apple gives no period: say when it answered.
                period: if a.is_apple() {
                    s.fmt(Text::AlertAsOf, &[("time", &stamp(a.start))])
                } else {
                    format!("{} – {}", stamp(a.start), stamp(a.end))
                },
                end: a.end,
                source: Some(a.source.trim().to_string()).filter(|s| !s.is_empty()),
                url: a.url.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::moon_illumination;

    #[test]
    fn the_moon_is_dark_when_new_and_lit_when_full() {
        assert!(moon_illumination(0.0) < 0.01);
        assert!((moon_illumination(0.5) - 100.0).abs() < 0.01);
        assert!(
            (moon_illumination(0.25) - 50.0).abs() < 0.01,
            "first quarter"
        );
        assert!(moon_illumination(0.99) < 1.0);
    }
}
