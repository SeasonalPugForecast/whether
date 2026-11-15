//! NHC: parsing the recorded active storms summaries and Josephine's (AL10
//! 2026) a-deck and b-deck, plus truncated and garbled input.

use proptest::prelude::*;
use whether_core::storm::hurricane::{DRAWN_AIDS, cone, ring_contains};
use whether_core::storm::{Basin, TrackFix};
use whether_core::time::{UnixTime, parse_rfc3339};
use whether_providers::nhc::{
    ADECK_BASE_URL, CURRENT_STORMS_URL, ForecastTracks, MAX_DECK_BYTES, adeck_request,
    bdeck_request, current_storms_request, gunzip, is_gzip, parse_adeck, parse_bdeck,
    parse_current_storms,
};
use whether_providers::{Endpoint, ProviderError};

/// `CurrentStorms.json` on 2026-11-14, outside the season.
const CURRENT: &str = include_str!("fixtures/nhc/current_storms.json");
/// NHC's sample summary: Gert, Franklin and Nine (Atlantic) and Hilary
/// (eastern Pacific) on 2023-08-22.
const SAMPLE: &str = include_str!("fixtures/nhc/current_storms_sample.json");
/// Hand-made: the sample's first entry with every field in place, its
/// values replaced by Josephine's last advisory (subtropical storm, 55 kt,
/// 988 hPa at 39.5° N 66.8° W, moving 55° at 11 kt, from the a-deck's last
/// official forecast at 2026-10-22 18 UTC). The bin number, file times and
/// product URLs follow the sample's pattern and are not NHC's.
const JOSEPHINE: &str = include_str!("fixtures/nhc/current_storms_josephine.synthetic.json");
const ADECK: &[u8] = include_bytes!("fixtures/nhc/aal102026.dat.gz");
const BDECK: &[u8] = include_bytes!("fixtures/nhc/bal102026.dat");

/// 2026-10-22 18:00 UTC: the last official forecast's start.
const LAST_INIT: UnixTime = UnixTime(1_792_692_000);
const HOUR: i64 = 3_600;

#[test]
fn requests_name_the_storm_files() {
    let current = current_storms_request();
    assert_eq!(current.url, CURRENT_STORMS_URL);
    assert_eq!(current.endpoint, Endpoint::Forecast);
    assert!(!current.required && !current.metered);
    assert_eq!(
        adeck_request("al102026").unwrap().url,
        format!("{ADECK_BASE_URL}/aal102026.dat.gz")
    );
    assert_eq!(
        bdeck_request("AL102026").unwrap().url,
        "https://ftp.nhc.noaa.gov/atcf/btk/bal102026.dat"
    );
    assert!(matches!(
        adeck_request("io012026"),
        Err(ProviderError::Config(_))
    ));
}

#[test]
fn an_empty_season_has_no_storms() {
    assert_eq!(parse_current_storms(CURRENT.as_bytes()), Ok(Vec::new()));
}

#[test]
fn the_sample_lists_four_storms() {
    let storms = parse_current_storms(SAMPLE.as_bytes()).unwrap();
    let summary: Vec<(&str, &str, &str, Basin)> = storms
        .iter()
        .map(|s| {
            (
                s.id.as_str(),
                s.name.as_str(),
                s.classification.as_str(),
                s.basin,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("al062023", "Gert", "TD", Basin::Atlantic),
            ("al082023", "Franklin", "TS", Basin::Atlantic),
            ("al092023", "Nine", "TD", Basin::Atlantic),
            ("ep092023", "Hilary", "PTC", Basin::EastPacific),
        ]
    );
    let gert = &storms[0];
    assert_eq!((gert.wind_kt, gert.pressure_hpa), (Some(25), Some(1008.0)));
    assert_eq!((gert.latitude, gert.longitude), (17.1, -58.4));
    assert_eq!(gert.movement_deg, Some(285.0));
    // NHC gives 9 mph; the advisory says "near 9 mph".
    assert!((gert.movement_kt.unwrap() - 7.82).abs() < 0.01);
    assert_eq!(gert.advisory_number.as_deref(), Some("011"));
    assert_eq!(
        gert.advisory_url.as_deref(),
        Some("https://www.nhc.noaa.gov/text/MIATCPAT1.shtml")
    );
    assert_eq!(gert.advisory_time, parse_rfc3339("2023-08-22T03:00:00Z"));
    assert_eq!(gert.forecast_advisory_time, gert.advisory_time);
    assert_eq!(gert.last_update, gert.advisory_time.unwrap());
    let hilary = &storms[3];
    assert_eq!((hilary.latitude, hilary.longitude), (44.6, -115.5));
    assert_eq!(
        hilary.last_update,
        parse_rfc3339("2023-08-21T21:00:00Z").unwrap()
    );
}

#[test]
fn josephine_parses_from_the_summary_and_decks() {
    let storms = parse_current_storms(JOSEPHINE.as_bytes()).unwrap();
    assert_eq!(storms.len(), 1);
    let storm = &storms[0];
    assert_eq!(
        (
            storm.id.as_str(),
            storm.name.as_str(),
            storm.classification.as_str()
        ),
        ("al102026", "Josephine", "STS")
    );
    let forecast = parse_adeck(ADECK, &DRAWN_AIDS).unwrap();
    let past = parse_bdeck(BDECK).unwrap();
    let h = storm.hurricane(forecast, past);
    assert_eq!(h.advisory_time, UnixTime(LAST_INIT.0 + 3 * HOUR));
    // Her advisory 5 is a forecast advisory too, and its track is the one
    // started three hours before.
    assert_eq!(h.forecast_advisory_time, Some(h.advisory_time));
    assert_eq!(h.official_init, Some(LAST_INIT));
    assert!(h.official_is_current());
    assert_eq!(h.official[0].position(), (h.latitude, h.longitude));
    assert_eq!(h.past.last().map(TrackFix::position), Some((39.5, -66.8)));
    let ring = cone(&h.official, h.basin);
    for fix in &h.official {
        assert!(ring_contains(&ring, fix.position()), "{fix:?}");
    }
    // New York is far behind the storm.
    assert!(!ring_contains(&ring, (40.7143, -74.005_97)));
}

#[test]
fn an_intermediate_advisory_keeps_the_forecast_advisory() {
    // Advisory 5A three hours on: a public advisory without a new forecast.
    let summary = JOSEPHINE.replacen("2026-10-22T21:00:00.000Z", "2026-10-23T00:00:00.000Z", 2);
    let storm = &parse_current_storms(summary.as_bytes()).unwrap()[0];
    let forecast_advisory = UnixTime(LAST_INIT.0 + 3 * HOUR);
    assert_eq!(storm.advisory_time, Some(UnixTime(LAST_INIT.0 + 6 * HOUR)));
    assert_eq!(storm.forecast_advisory_time, Some(forecast_advisory));
    assert_eq!(storm.forecast_time(), forecast_advisory);
    let h = storm.hurricane(parse_adeck(ADECK, &DRAWN_AIDS).unwrap(), Vec::new());
    assert_eq!(h.forecast_time(), forecast_advisory);
    assert!(h.official_is_current());
}

#[test]
fn the_adeck_keeps_each_aids_newest_run() {
    let ForecastTracks {
        official,
        official_init,
        aids,
    } = parse_adeck(ADECK, &DRAWN_AIDS).unwrap();
    assert_eq!(official_init, Some(LAST_INIT));
    let hours: Vec<i64> = official
        .iter()
        .map(|f| (f.time.0 - LAST_INIT.0) / HOUR)
        .collect();
    assert_eq!(hours, [0, 3, 12, 24, 36, 48, 60]);
    assert_eq!(official[0].position(), (39.5, -66.8));
    assert_eq!(official[0].wind_kt, Some(55));
    // The 3-hour position carries the advisory's pressure.
    assert_eq!(official[1].pressure_hpa, Some(988.0));
    assert_eq!(official[6].position(), (43.6, -48.0));
    // ECMWF is not in the 2026 public a-deck.
    let present: Vec<&str> = aids.keys().map(String::as_str).collect();
    assert_eq!(present, ["AVNI", "CMCI", "HFAI", "TVCN", "UKXI"]);
    for (aid, track) in &aids {
        assert!(!track.is_empty(), "{aid}");
        assert_eq!(track[0].time, LAST_INIT, "{aid}");
        assert!(track.windows(2).all(|w| w[0].time < w[1].time), "{aid}");
    }
    let gfs = &aids["AVNI"];
    assert_eq!(gfs[1].position(), (41.0, -62.8));
    assert_eq!(gfs[1].wind_kt, Some(63));
    assert_eq!(gfs[1].time, UnixTime(LAST_INIT.0 + 12 * HOUR));
    // Only the aids asked for.
    let only = parse_adeck(ADECK, &["tvcn"]).unwrap();
    assert_eq!(only.aids.keys().collect::<Vec<_>>(), ["tvcn"]);
    assert_eq!(only.official_init, Some(LAST_INIT));
}

#[test]
fn inflated_decks_parse_the_same() {
    let text = gunzip(ADECK, MAX_DECK_BYTES).unwrap();
    assert!(!is_gzip(&text));
    assert_eq!(text.len(), 2_149_786);
    assert_eq!(
        parse_adeck(&text, &DRAWN_AIDS).unwrap(),
        parse_adeck(ADECK, &DRAWN_AIDS).unwrap()
    );
    assert!(matches!(
        gunzip(ADECK, 1 << 20),
        Err(ProviderError::Malformed(_))
    ));
}

#[test]
fn the_bdeck_is_one_fix_per_time_in_order() {
    let past = parse_bdeck(BDECK).unwrap();
    // 23 rows: a 34 and a 50 kt row at some times, and the first three
    // written last.
    assert_eq!(past.len(), 17);
    assert!(
        past.windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 6 * HOUR)
    );
    assert_eq!(past[0].position(), (31.0, -71.4));
    assert_eq!(past[0].time, UnixTime(LAST_INIT.0 - 4 * 24 * HOUR));
    let last = &past[past.len() - 1];
    assert_eq!(
        (last.time, last.wind_kt, last.pressure_hpa),
        (LAST_INIT, Some(55), Some(988.0))
    );
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    for body in [
        &b"<html>Not Found</html>"[..],
        b"\x1f\x8b",
        b"\x1f\x8b\x08\x00garbage",
    ] {
        assert!(parse_adeck(body, &DRAWN_AIDS).is_err(), "{body:?}");
        assert!(parse_bdeck(body).is_err(), "{body:?}");
    }
    assert_eq!(parse_adeck(b"", &DRAWN_AIDS), Ok(ForecastTracks::default()));
    assert_eq!(parse_bdeck(b"\n"), Ok(Vec::new()));
    // A wrong checksum.
    let mut bad = ADECK.to_vec();
    let n = bad.len();
    bad[n - 6] ^= 0xFF;
    assert!(matches!(
        parse_adeck(&bad, &DRAWN_AIDS),
        Err(ProviderError::Malformed(_))
    ));
    assert!(parse_current_storms(b"<html>").is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A download cut short anywhere is an error, never a partial track.
    #[test]
    fn truncated_gzip_is_an_error(cut in 0..ADECK.len()) {
        prop_assert!(parse_adeck(&ADECK[..cut], &DRAWN_AIDS).is_err());
    }

    /// Cut plain text loses at most the rows after the cut.
    #[test]
    fn truncated_text_still_parses(cut in 0..BDECK.len()) {
        let past = parse_bdeck(&BDECK[..cut]);
        prop_assert!(past.is_ok() || cut < 80, "{cut}: {past:?}");
        let full = parse_bdeck(BDECK).unwrap();
        for fix in past.unwrap_or_default() {
            prop_assert!(full.contains(&fix), "{fix:?}");
        }
    }

    /// Bytes flipped anywhere in the text never panic the parsers.
    #[test]
    fn garbled_text_never_panics(at in 0..BDECK.len(), byte in any::<u8>()) {
        let mut garbled = BDECK.to_vec();
        garbled[at] = byte;
        let _ = parse_bdeck(&garbled);
        let _ = parse_adeck(&garbled, &DRAWN_AIDS);
        let _ = parse_current_storms(&garbled);
    }
}

/// The storms view of Josephine built from the recordings (and the
/// hand-made summary entry).
#[cfg(feature = "serde")]
mod view {
    use super::*;
    use whether_core::model::ResolvedLocation;
    use whether_core::presentation::{PresentationConfig, TimeFormat};
    use whether_core::storm::{Hurricane, StormOutlook};
    use whether_core::strings::Strings;
    use whether_core::time::UtcOffset;
    use whether_core::units::UnitPrefs;
    use whether_core::view::storms::{build_storms, storm_ahead};

    /// Half an hour after Josephine's last advisory (5:30 PM EDT).
    pub(super) const NOW: UnixTime = UnixTime(LAST_INIT.0 + 3 * HOUR + 1_800);

    pub(super) fn josephine() -> Hurricane {
        let storms = parse_current_storms(JOSEPHINE.as_bytes()).unwrap();
        let forecast = parse_adeck(ADECK, &DRAWN_AIDS).unwrap();
        storms[0].hurricane(forecast, parse_bdeck(BDECK).unwrap())
    }

    pub(super) fn place(name: &str, latitude: f64, longitude: f64) -> ResolvedLocation {
        ResolvedLocation {
            name: name.to_string(),
            latitude,
            longitude,
            utc_offset: UtcOffset(-4 * 3_600),
            ..ResolvedLocation::default()
        }
    }

    pub(super) fn outlook(place: ResolvedLocation) -> StormOutlook {
        StormOutlook {
            place,
            hurricanes: vec![josephine()],
            fetched_at: NOW,
            ..StormOutlook::default()
        }
    }

    pub(super) fn new_york() -> ResolvedLocation {
        place("New York", 40.7143, -74.005_97)
    }

    pub(super) fn imperial() -> PresentationConfig {
        PresentationConfig {
            units: UnitPrefs::IMPERIAL,
            time_format: TimeFormat::H12,
            ..PresentationConfig::default()
        }
    }

    pub(super) fn pretty<T: serde::Serialize>(vm: &T) -> String {
        serde_json::to_string_pretty(vm).unwrap()
    }

    #[test]
    fn josephine_is_a_subtropical_storm_heading_out_to_sea() {
        let vm = build_storms(
            &outlook(new_york()),
            &[],
            &imperial(),
            &Strings::default(),
            NOW,
        );
        let h = &vm.hurricanes[0];
        assert_eq!(h.title, "Subtropical Storm Josephine");
        assert_eq!(h.movement, "NE at 13 mph");
        assert_eq!(h.advisory, "Advisory Thu 5:00 PM");
        assert_eq!(h.reaches, None);
        let aids: Vec<&str> = h.aids.iter().map(|a| a.short.as_str()).collect();
        assert_eq!(aids, ["GFS", "GEM", "UKMET", "HAFS", "Consensus"]);
        assert_eq!(
            vm.attribution.last().map(String::as_str),
            Some("Hurricanes: NOAA National Hurricane Center")
        );
    }

    #[test]
    fn snapshot_josephine_imperial() {
        let vm = build_storms(
            &outlook(new_york()),
            &[],
            &imperial(),
            &Strings::default(),
            NOW,
        );
        insta::assert_snapshot!("josephine_imperial", pretty(&vm));
    }

    #[test]
    fn snapshot_josephine_metric() {
        let vm = build_storms(
            &outlook(new_york()),
            &[],
            &PresentationConfig::default(),
            &Strings::default(),
            NOW,
        );
        insta::assert_snapshot!("josephine_metric", pretty(&vm));
    }

    /// A made-up place on Josephine's official track, at its 36-hour
    /// position.
    fn on_the_track() -> ResolvedLocation {
        place("Test Point", 43.5, -55.9)
    }

    #[test]
    fn the_cone_warns_a_place_on_the_track() {
        let vm = storm_ahead(
            &outlook(on_the_track()),
            &imperial(),
            &Strings::default(),
            NOW,
        )
        .unwrap();
        assert_eq!(vm.title, "Subtropical Storm Josephine");
        assert_eq!(vm.line, "The forecast cone reaches Test Point Fri");
        assert_eq!(vm.hurricane.as_deref(), Some("al102026"));
        assert!(vm.start > NOW && vm.start.0 < LAST_INIT.0 + 36 * HOUR);
        let none = storm_ahead(&outlook(new_york()), &imperial(), &Strings::default(), NOW);
        assert_eq!(none, None);
    }

    #[test]
    fn snapshot_josephine_storm_ahead() {
        let vm = storm_ahead(
            &outlook(on_the_track()),
            &imperial(),
            &Strings::default(),
            NOW,
        );
        insta::assert_snapshot!("josephine_storm_ahead", pretty(&vm));
    }
}
