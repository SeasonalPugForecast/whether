//! The radar loop's labels.

use alloc::string::String;
use alloc::vec::Vec;

use crate::presentation::PresentationConfig;
use crate::strings::Strings;
use crate::time::{UnixTime, UtcOffset};
use crate::view::RadarVm;
use crate::view::format::Formatter;

/// Labels for frames valid at `times` (oldest first), as local clock times; `offset`
/// gives each time's UTC offset.
pub fn radar_vm(
    times: &[UnixTime],
    offset: impl Fn(UnixTime) -> UtcOffset,
    attribution: &str,
    presentation: &PresentationConfig,
    strings: &Strings,
) -> RadarVm {
    let f = Formatter::new(presentation, strings);
    RadarVm {
        frames: times
            .iter()
            .map(|t| f.clock(*t, offset(*t)))
            .collect::<Vec<_>>(),
        attribution: String::from(attribution),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::TimeFormat;

    #[test]
    fn frames_are_local_clock_times() {
        let edt = UtcOffset(-14_400);
        // 2026-10-10 19:50 and 20:15 UTC.
        let times = [UnixTime(1_791_661_800), UnixTime(1_791_663_300)];
        let p = PresentationConfig::default();
        let vm = radar_vm(&times, |_| edt, "credit", &p, &Strings::default());
        assert_eq!(vm.frames, ["15:50", "16:15"]);
        assert_eq!(vm.attribution, "credit");
        let twelve = PresentationConfig {
            time_format: TimeFormat::H12,
            ..PresentationConfig::default()
        };
        let vm = radar_vm(&times, |_| edt, "credit", &twelve, &Strings::default());
        assert_eq!(vm.frames, ["3:50 PM", "4:15 PM"]);
    }
}
