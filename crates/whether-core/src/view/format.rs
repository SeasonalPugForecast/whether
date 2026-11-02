//! Formatting of every user-visible value. Displays never convert or round; they
//! show what these functions produce.

use alloc::format;
use alloc::string::{String, ToString};

use crate::presentation::{DateFormat, PresentationConfig, TemperatureBasis, TimeFormat};
use crate::strings::{Strings, Text, month_text};
use crate::time::{LocalDate, UnixTime, UtcOffset};
use crate::units::{
    Celsius, Hpa, Meters, Mm, Mps, Percent, PrecipUnit, PressureUnit, TemperatureUnit, WindUnit,
};

/// A formatted value: the number in display units (for charts and gauges), the text
/// to show, and the unit symbol.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Quantity {
    /// The value in display units, if known.
    pub value: Option<f32>,
    /// The display text, e.g. "21°", "13 km/h" or the missing placeholder.
    pub text: String,
    /// The unit symbol, e.g. "°C" or "km/h".
    pub unit: &'static str,
}

/// Formats `x` with `decimals` places, rounding half away from zero, without "-0".
pub fn number(x: f32, decimals: u8) -> String {
    let scale = libm::powf(10.0, f32::from(decimals.min(6)));
    let mut r = libm::roundf(x * scale) / scale;
    if r == 0.0 {
        r = 0.0; // turns -0.0 into 0.0
    }
    format!("{r:.prec$}", prec = usize::from(decimals.min(6)))
}

/// A chance in percent, "80%", formatted like [`Formatter::percent`]. Hourly and
/// daily rows carry the chance as a number; displays format it with this.
pub fn chance(p: u8) -> String {
    format!("{p}%")
}

/// Formats values according to the presentation settings.
#[derive(Clone, Copy, Debug)]
pub struct Formatter<'a> {
    /// Presentation settings (units, decimals, clock style, placeholder).
    pub presentation: &'a PresentationConfig,
    /// Active strings.
    pub strings: &'a Strings,
}

impl<'a> Formatter<'a> {
    /// A formatter for the given settings.
    pub fn new(presentation: &'a PresentationConfig, strings: &'a Strings) -> Self {
        Self {
            presentation,
            strings,
        }
    }

    fn missing(&self, unit: &'static str) -> Quantity {
        Quantity {
            value: None,
            text: self.presentation.missing.clone(),
            unit,
        }
    }

    fn quantity(value: f32, decimals: u8, unit: &'static str) -> Quantity {
        Quantity {
            value: Some(value),
            text: format!("{} {unit}", number(value, decimals)),
            unit,
        }
    }

    /// The temperature to show: the "feels like" one when the presentation leads
    /// with it and it is known, else the actual one.
    pub fn lead(&self, actual: Option<Celsius>, feels_like: Option<Celsius>) -> Option<Celsius> {
        match self.presentation.temperature_basis {
            TemperatureBasis::FeelsLike => feels_like.or(actual),
            TemperatureBasis::Actual => actual,
        }
    }

    /// "21°" (or "21 °C" with `show_unit_letter`; Kelvin is always "294 K").
    pub fn temperature(&self, c: Option<Celsius>) -> Quantity {
        let u = self.presentation.units.temperature;
        let unit = u.symbol();
        let Some(c) = c else {
            return self.missing(unit);
        };
        let value = c.to(u);
        let n = number(value, self.presentation.temperature_decimals);
        let text = match u {
            TemperatureUnit::Kelvin => format!("{n} K"),
            _ if self.presentation.show_unit_letter => format!("{n} {unit}"),
            _ => format!("{n}°"),
        };
        Quantity {
            value: Some(value),
            text,
            unit,
        }
    }

    /// "13 km/h"; one decimal for m/s; "4 Bft" for Beaufort.
    pub fn wind(&self, v: Option<Mps>) -> Quantity {
        let u = self.presentation.units.wind;
        let decimals = u8::from(u == WindUnit::Mps);
        let unit = u.symbol();
        match v {
            Some(v) => Self::quantity(v.to(u), decimals, unit),
            None => self.missing(unit),
        }
    }

    /// "1020 hPa", "30.12 inHg", "765 mmHg" or "102.0 kPa".
    pub fn pressure(&self, v: Option<Hpa>) -> Quantity {
        let u = self.presentation.units.pressure;
        let decimals = match u {
            PressureUnit::Hpa | PressureUnit::MmHg => 0,
            PressureUnit::InHg => 2,
            PressureUnit::Kpa => 1,
        };
        let unit = u.symbol();
        match v {
            Some(v) => Self::quantity(v.to(u), decimals, unit),
            None => self.missing(unit),
        }
    }

    /// "2.5 mm" or "0.10 in".
    pub fn precipitation(&self, v: Option<Mm>) -> Quantity {
        let u = self.presentation.units.precipitation;
        let decimals = if u == PrecipUnit::Mm { 1 } else { 2 };
        let unit = u.symbol();
        match v {
            Some(v) => Self::quantity(v.to(u), decimals, unit),
            None => self.missing(unit),
        }
    }

    /// Visibility: one decimal below 10, whole numbers up to 50, then "> 50 km".
    pub fn distance(&self, v: Option<Meters>) -> Quantity {
        let u = self.presentation.units.distance;
        let unit = u.symbol();
        let Some(v) = v else {
            return self.missing(unit);
        };
        let value = v.to(u);
        if value > 50.0 {
            return Quantity {
                value: Some(value),
                text: format!("> 50 {unit}"),
                unit,
            };
        }
        Self::quantity(value, u8::from(value < 10.0), unit)
    }

    /// "62%".
    pub fn percent(&self, v: Option<Percent>) -> Quantity {
        match v {
            Some(v) => Quantity {
                value: Some(v.0),
                text: format!("{}%", number(v.0, 0)),
                unit: "%",
            },
            None => self.missing("%"),
        }
    }

    /// A time of day: "07:03" or "7:03 AM".
    pub fn clock(&self, t: UnixTime, offset: UtcOffset) -> String {
        let l = t.to_local(offset);
        match self.presentation.time_format {
            TimeFormat::H24 => format!("{:02}:{:02}", l.hour, l.minute),
            TimeFormat::H12 => {
                let (h, ampm) = twelve_hour(l.hour);
                format!("{h}:{:02} {ampm}", l.minute)
            }
        }
    }

    /// An hourly column label: "14:00" or "2 PM".
    pub fn hour_label(&self, t: UnixTime, offset: UtcOffset) -> String {
        let l = t.to_local(offset);
        match self.presentation.time_format {
            TimeFormat::H24 => format!("{:02}:00", l.hour),
            TimeFormat::H12 => {
                let (h, ampm) = twelve_hour(l.hour);
                format!("{h} {ampm}")
            }
        }
    }

    /// A calendar date in the chosen style: "Nov 1", "NOV1", "11/01", …
    pub fn date(&self, d: LocalDate) -> String {
        let month = || self.strings.get(month_text(d.month));
        let (m, day) = (d.month, d.day);
        match self.presentation.date_format {
            DateFormat::MonthNameDay => format!("{} {day}", month()),
            DateFormat::DayMonthName => format!("{day} {}", month()),
            DateFormat::Compact => format!("{}{day}", month().to_uppercase()),
            DateFormat::MonthDay => format!("{m}/{day}"),
            DateFormat::MonthDayPadded => format!("{m:02}/{day:02}"),
            DateFormat::DayMonth => format!("{day}/{m}"),
            DateFormat::DayMonthPadded => format!("{day:02}/{m:02}"),
            DateFormat::DayMonthDots => format!("{day:02}.{m:02}."),
            DateFormat::Iso => format!("{:04}-{m:02}-{day:02}", d.year),
        }
    }

    /// "Updated just now", "Updated 3 min ago" or "Updated 2 h ago".
    pub fn updated(&self, last: UnixTime, now: UnixTime) -> String {
        let secs = (now.0 - last.0).max(0);
        if secs < 60 {
            self.strings.get(Text::UpdatedJustNow).to_string()
        } else if secs < 3_600 {
            self.strings
                .fmt(Text::UpdatedMinutesAgo, &[("n", &(secs / 60).to_string())])
        } else {
            self.strings
                .fmt(Text::UpdatedHoursAgo, &[("n", &(secs / 3_600).to_string())])
        }
    }
}

fn twelve_hour(hour: u8) -> (u8, &'static str) {
    let ampm = if hour < 12 { "AM" } else { "PM" };
    let h = match hour % 12 {
        0 => 12,
        h => h,
    };
    (h, ampm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::UnitPrefs;

    #[test]
    fn chances_match_percentages() {
        let s = crate::strings::Strings::default();
        let p = crate::presentation::PresentationConfig::default();
        let f = Formatter::new(&p, &s);
        assert_eq!(
            chance(80),
            f.percent(Some(crate::units::Percent(80.0))).text
        );
        assert_eq!(chance(0), "0%");
    }

    fn fmt_with(p: &PresentationConfig, f: impl FnOnce(Formatter<'_>)) {
        let s = Strings::default();
        f(Formatter::new(p, &s));
    }

    #[test]
    fn numbers_round_half_away_from_zero_without_negative_zero() {
        assert_eq!(number(20.5, 0), "21");
        assert_eq!(number(-20.5, 0), "-21");
        assert_eq!(number(-0.4, 0), "0");
        assert_eq!(number(-0.04, 1), "0.0");
        assert_eq!(number(29.921, 2), "29.92");
    }

    #[test]
    fn temperatures() {
        let p = PresentationConfig::default();
        fmt_with(&p, |f| {
            assert_eq!(f.temperature(Some(Celsius(15.3))).text, "15°");
            assert_eq!(f.temperature(Some(Celsius(-0.3))).text, "0°");
            assert_eq!(f.temperature(None).text, "—");
        });
        let p = PresentationConfig {
            units: UnitPrefs::IMPERIAL,
            temperature_decimals: 1,
            show_unit_letter: true,
            ..Default::default()
        };
        fmt_with(&p, |f| {
            let q = f.temperature(Some(Celsius(15.3)));
            assert_eq!(q.text, "59.5 °F");
            assert!((q.value.unwrap() - 59.54).abs() < 1e-3);
        });
    }

    #[test]
    fn wind_pressure_precipitation_distance() {
        let p = PresentationConfig::default();
        fmt_with(&p, |f| {
            assert_eq!(f.wind(Some(Mps(3.6))).text, "13 km/h");
            assert_eq!(f.pressure(Some(Hpa(1_019.8))).text, "1020 hPa");
            assert_eq!(f.precipitation(Some(Mm(2.0))).text, "2.0 mm");
            assert_eq!(f.distance(Some(Meters(8_000.0))).text, "8.0 km");
            assert_eq!(f.distance(Some(Meters(24_140.0))).text, "24 km");
            assert_eq!(f.distance(Some(Meters(60_000.0))).text, "> 50 km");
            assert_eq!(f.percent(Some(Percent(62.4))).text, "62%");
            assert_eq!(f.wind(None).text, "—");
        });
        let p = PresentationConfig {
            units: UnitPrefs::IMPERIAL,
            ..Default::default()
        };
        fmt_with(&p, |f| {
            assert_eq!(f.wind(Some(Mps(3.6))).text, "8 mph");
            assert_eq!(f.pressure(Some(Hpa(1_019.8))).text, "30.11 inHg");
            assert_eq!(f.precipitation(Some(Mm(2.0))).text, "0.08 in");
            assert_eq!(f.distance(Some(Meters(24_140.0))).text, "15 mi");
        });
        let mut units = UnitPrefs::METRIC;
        units.wind = WindUnit::Beaufort;
        units.pressure = PressureUnit::Kpa;
        let p = PresentationConfig {
            units,
            ..Default::default()
        };
        fmt_with(&p, |f| {
            assert_eq!(f.wind(Some(Mps(10.0))).text, "5 Bft");
            assert_eq!(f.pressure(Some(Hpa(1_013.25))).text, "101.3 kPa");
        });
    }

    #[test]
    fn clock_and_hour_labels() {
        let edt = UtcOffset(-14_400);
        let sunrise = UnixTime(1_791_198_192); // 07:03 EDT
        let afternoon = UnixTime(1_791_230_400); // 16:00 EDT
        let midnight = UnixTime(1_791_172_800); // 00:00 EDT
        let p = PresentationConfig::default();
        fmt_with(&p, |f| {
            assert_eq!(f.clock(sunrise, edt), "07:03");
            assert_eq!(f.hour_label(afternoon, edt), "16:00");
        });
        let p = PresentationConfig {
            time_format: TimeFormat::H12,
            ..Default::default()
        };
        fmt_with(&p, |f| {
            assert_eq!(f.clock(sunrise, edt), "7:03 AM");
            assert_eq!(f.hour_label(afternoon, edt), "4 PM");
            assert_eq!(f.hour_label(midnight, edt), "12 AM");
        });
    }

    #[test]
    fn dates_in_each_style() {
        let nov1 = LocalDate {
            year: 2026,
            month: 11,
            day: 1,
        };
        let feb26 = LocalDate {
            year: 2027,
            month: 2,
            day: 26,
        };
        let cases = [
            (DateFormat::MonthNameDay, "Nov 1", "Feb 26"),
            (DateFormat::DayMonthName, "1 Nov", "26 Feb"),
            (DateFormat::Compact, "NOV1", "FEB26"),
            (DateFormat::MonthDay, "11/1", "2/26"),
            (DateFormat::MonthDayPadded, "11/01", "02/26"),
            (DateFormat::DayMonth, "1/11", "26/2"),
            (DateFormat::DayMonthPadded, "01/11", "26/02"),
            (DateFormat::DayMonthDots, "01.11.", "26.02."),
            (DateFormat::Iso, "2026-11-01", "2027-02-26"),
        ];
        for (style, a, b) in cases {
            let p = PresentationConfig {
                date_format: style,
                ..Default::default()
            };
            fmt_with(&p, |f| {
                assert_eq!((f.date(nov1).as_str(), f.date(feb26).as_str()), (a, b));
            });
        }
    }

    #[test]
    fn relative_update_times() {
        let p = PresentationConfig::default();
        fmt_with(&p, |f| {
            assert_eq!(
                f.updated(UnixTime(1_000), UnixTime(1_030)),
                "Updated just now"
            );
            assert_eq!(
                f.updated(UnixTime(1_000), UnixTime(1_000 + 180)),
                "Updated 3 min ago"
            );
            assert_eq!(f.updated(UnixTime(0), UnixTime(7_300)), "Updated 2 h ago");
            assert_eq!(f.updated(UnixTime(100), UnixTime(0)), "Updated just now");
        });
    }
}
