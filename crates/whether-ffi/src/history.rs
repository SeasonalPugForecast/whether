//! History for the app: past days, normals, records and the chance
//! of rain or snow, fetched about once a day with Swift doing the HTTP, and
//! climate queries for planning a trip.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use whether_core::history::{ClimateQuery, History, Normals, YearRange};
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::acis::Acis;
use whether_providers::open_meteo::history::{OpenMeteoHistory, OpenMeteoHistorySettings};
use whether_providers::{
    Attempt, FetchError, HistoryContext, HistoryProvider, HistorySession, HttpResponse,
};

use crate::fetch::{AttemptInfo, FetchHost, attempt_info, host_request, outgoing};
use crate::settings::{Place, Settings};
use crate::{WhetherError, from_json};

/// A place's history as the app keeps it: what it was fetched for and on
/// which of the place's dates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Kept {
    /// [`KEPT_VERSION`] when fetched (0 before versions).
    #[serde(default)]
    version: u32,
    /// The place's date it was fetched on.
    today: LocalDate,
    past_days: u8,
    climate_days: u8,
    records: bool,
    month_to_date: bool,
    /// The history.
    pub(crate) history: History,
}

impl Kept {
    fn requirements(&self) -> HistoryRequirements {
        HistoryRequirements {
            past_days: self.past_days,
            climate_days: self.climate_days,
            records: self.records,
            month_to_date: self.month_to_date,
        }
    }
}

/// The kept format; an older kept history is fetched again. 1: records reach
/// back to the past days.
const KEPT_VERSION: u32 = 1;

/// The kept history in `json`, if it reads.
pub(crate) fn kept(json: &str) -> Result<Kept, WhetherError> {
    from_json("kept history", json)
}

/// A fetched history.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HistoryOutcome {
    /// The history as the app keeps it: pass it back to `history_due` and in
    /// `LocationEntry.history_json`.
    pub history_json: String,
    /// Normals computed this time (Open-Meteo, from 30 years of ERA5): keep
    /// them for good and pass them to every later `fetch_history` of the place.
    pub normals_json: Option<String>,
    /// Every request's outcome, including fallbacks.
    pub attempts: Vec<AttemptInfo>,
}

fn offset(place: &Place) -> UtcOffset {
    UtcOffset(place.utc_offset_seconds.unwrap_or(0))
}

fn context(place: &Place, now: i64, normals: Option<Normals>) -> HistoryContext {
    let now = UnixTime(now);
    HistoryContext {
        now,
        today: now.local_date(offset(place)),
        utc_offset: offset(place),
        user_agent: crate::user_agent(),
        normals,
    }
}

/// The sources: ACIS for US stations, then Open-Meteo; Open-Meteo fills gaps
/// (without computing normals again).
fn sources(settings: &Settings, supplement: bool) -> (Acis, OpenMeteoHistory) {
    (
        Acis::default(),
        OpenMeteoHistory::new(OpenMeteoHistorySettings {
            models: settings.models.clone(),
            normals: !supplement,
            ..OpenMeteoHistorySettings::default()
        }),
    )
}

/// Runs `session` to its end over `host`.
async fn run(
    mut session: HistorySession<'_>,
    host: &Arc<dyn FetchHost>,
    now: i64,
) -> Result<(History, Vec<Attempt>), FetchError> {
    while let Some(request) = session.next_request() {
        let outgoing = host_request(request);
        match host.send(outgoing).await {
            Ok(resp) => session.on_response(&HttpResponse::new(resp.status, &resp.body)),
            Err(e) => session.on_transport_error(&e.to_string()),
        }
    }
    session.finish(UnixTime(now))
}

fn failed(attempts: &[Attempt]) -> WhetherError {
    WhetherError::NoHistory {
        summary: attempts
            .iter()
            .map(attempt_info)
            .map(|a| {
                format!(
                    "{} {}: {}",
                    a.provider,
                    a.endpoint,
                    a.error.unwrap_or_else(|| "ok".into())
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn to_json(what: &str, value: &impl Serialize) -> Result<String, WhetherError> {
    serde_json::to_string(value).map_err(|e| WhetherError::InvalidJson {
        what: what.into(),
        message: e.to_string(),
    })
}

/// What the settings show of history (none when history is off). A history
/// of one day (the phone's) shows no month so far, so none is fetched.
pub(crate) fn requirements(settings: &Settings, place: &Place) -> HistoryRequirements {
    let mut req = HistoryRequirements::from_presentation(&settings.presentation(place));
    if settings.history_days <= 1 {
        req.month_to_date = false;
    }
    req
}

/// Whether `place`'s history should be fetched now: the settings show some,
/// and nothing is kept, the kept one lacks what they show, or it is from an
/// earlier day there (after 00:15, as stations report late).
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn history_due(place: Place, settings: Settings, kept_json: Option<String>, now: i64) -> bool {
    let req = requirements(&settings, &place);
    if req.is_empty() {
        return false;
    }
    let kept = kept_json
        .as_deref()
        .and_then(|json| kept(json).ok())
        .filter(|k| k.version >= KEPT_VERSION);
    whether_providers::history_due(
        kept.as_ref().map(|k| (k.today, k.requirements())),
        &req,
        UnixTime(now),
        offset(&place),
    )
}

/// Fetches `place`'s history for what the settings show, through `host`.
/// `normals_json` is what an earlier fetch returned in
/// [`HistoryOutcome::normals_json`].
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn fetch_history(
    place: Place,
    settings: Settings,
    normals_json: Option<String>,
    now: i64,
    host: Arc<dyn FetchHost>,
) -> Result<HistoryOutcome, WhetherError> {
    let req = requirements(&settings, &place);
    let normals = normals_json
        .as_deref()
        .map(|json| from_json::<Normals>("normals", json))
        .transpose()?;
    let ctx = context(&place, now, normals);
    let today = ctx.today;
    let (acis, om) = sources(&settings, false);
    let (_, supplement) = sources(&settings, true);
    let chain: [&dyn HistoryProvider; 2] = [&acis, &om];
    let supplements: [&dyn HistoryProvider; 1] = [&supplement];
    let session = HistorySession::new(&chain, &supplements, outgoing(&place), req, ctx);
    let (mut history, attempts) = match run(session, &host, now).await {
        Ok(found) => found,
        Err(FetchError::AllProvidersFailed(attempts)) => return Err(failed(&attempts)),
        Err(e) => {
            return Err(WhetherError::NoHistory {
                summary: e.to_string(),
            });
        }
    };
    let normals_json = history
        .normals
        .take()
        .map(|n| to_json("normals", &n))
        .transpose()?;
    history.location.name.clone_from(&place.name);
    let kept = Kept {
        version: KEPT_VERSION,
        today,
        past_days: req.past_days,
        climate_days: req.climate_days,
        records: req.records,
        month_to_date: req.month_to_date,
        history,
    };
    Ok(HistoryOutcome {
        history_json: to_json("history", &kept)?,
        normals_json,
        attempts: attempts.iter().map(attempt_info).collect(),
    })
}

/// A trip to plan: its dates and the years to look back over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Trip {
    /// The first date's year, month and day.
    pub from_year: i32,
    /// Month, 1–12.
    pub from_month: u8,
    /// Day of the month.
    pub from_day: u8,
    /// The last date's year (at most 31 days after the first).
    pub to_year: i32,
    /// Month, 1–12.
    pub to_month: u8,
    /// Day of the month.
    pub to_day: u8,
    /// The first year looked back at.
    pub first_year: i32,
    /// The last year looked back at.
    pub last_year: i32,
}

impl Trip {
    fn query(self) -> Result<ClimateQuery, WhetherError> {
        ClimateQuery::new(
            LocalDate {
                year: self.from_year,
                month: self.from_month,
                day: self.from_day,
            },
            LocalDate {
                year: self.to_year,
                month: self.to_month,
                day: self.to_day,
            },
            YearRange {
                first: self.first_year,
                last: self.last_year,
            },
        )
        .map_err(|e| WhetherError::Trip {
            message: e.to_string(),
        })
    }
}

/// Checks a trip: `None` when it can be asked, else why not ("at most 31
/// days").
#[uniffi::export]
pub fn trip_problem(trip: Trip) -> Option<String> {
    trip.query().err().map(|e| e.to_string())
}

/// How `trip`'s dates were at `place` over its years (the vacation planner):
/// ACIS for a US station that covers them, else the Open-Meteo archive. The
/// answer is JSON for `plan_view`; past years never change, so keep it.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn fetch_climate(
    place: Place,
    settings: Settings,
    trip: Trip,
    now: i64,
    host: Arc<dyn FetchHost>,
) -> Result<String, WhetherError> {
    let query = trip.query()?;
    let (acis, om) = sources(&settings, false);
    let chain: [&dyn HistoryProvider; 2] = [&acis, &om];
    let session =
        HistorySession::climate(&chain, outgoing(&place), query, context(&place, now, None));
    match run(session, &host, now).await {
        Ok((mut history, _)) => {
            history.location.name.clone_from(&place.name);
            to_json("climate", &history)
        }
        Err(FetchError::AllProvidersFailed(attempts)) => Err(failed(&attempts)),
        Err(e) => Err(WhetherError::NoHistory {
            summary: e.to_string(),
        }),
    }
}

/// The planner's view (`PlanVm` as JSON) of `fetch_climate`'s answer for
/// `trip`, in the place's units and the settings' language.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn plan_view(
    place: Place,
    settings: Settings,
    trip: Trip,
    climate_json: String,
) -> Result<String, WhetherError> {
    let query = trip.query()?;
    let history: History = from_json("climate", &climate_json)?;
    let strings = whether_core::strings::Strings::localized(
        &settings.language,
        &std::collections::BTreeMap::default(),
    )
    .0;
    let plan =
        whether_core::view::plan_vm(&settings.presentation(&place), &strings, &history, &query);
    to_json("plan", &plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{HostError, HostRequest, HostResponse};
    use crate::settings::default_settings;
    use std::sync::Mutex;

    /// Answers ACIS and Open-Meteo from the recorded fixtures.
    struct Recorded {
        asked: Mutex<Vec<String>>,
        acis_up: bool,
    }

    const STATIONS: &str =
        include_str!("../../whether-providers/tests/fixtures/acis/stnmeta_10001.json");

    #[async_trait::async_trait]
    impl FetchHost for Recorded {
        async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
            self.asked.lock().unwrap().push(request.url.clone());
            let body = request.body.clone().unwrap_or_default();
            let text: &str = if !self.acis_up && request.url.contains("rcc-acis") {
                return Ok(HostResponse {
                    status: 503,
                    body: Vec::new(),
                    date: None,
                    expires: None,
                    last_modified: None,
                });
            } else if request.url.ends_with("StnMeta") {
                STATIONS
            } else if request.url.ends_with("StnData") {
                if body.contains("\"mean\"") {
                    include_str!(
                        "../../whether-providers/tests/fixtures/acis/climate_NYC_0712_0719.json"
                    )
                } else if body.contains("normal") {
                    include_str!("../../whether-providers/tests/fixtures/acis/stndata_NYC.json")
                } else if body.contains("cnt_ge") {
                    include_str!("../../whether-providers/tests/fixtures/acis/chance_NYC.json")
                } else if body.contains("NYCthr") {
                    include_str!("../../whether-providers/tests/fixtures/acis/records_NYCthr.json")
                } else {
                    include_str!("../../whether-providers/tests/fixtures/acis/records_NYC.json")
                }
            } else if request.url.contains("historical-forecast") {
                include_str!(
                    "../../whether-providers/tests/fixtures/open_meteo/history_past_10001.json"
                )
            } else if request.url.contains("archive-api") {
                include_str!(
                    "../../whether-providers/tests/fixtures/open_meteo/climate_10001_0712_0719.json"
                )
            } else {
                return Err(HostError::Failed {
                    message: "no route".into(),
                });
            };
            Ok(HostResponse {
                status: 200,
                body: text.as_bytes().to_vec(),
                date: None,
                expires: None,
                last_modified: None,
            })
        }

        fn save_usage(&self, _: String) -> Result<(), HostError> {
            Ok(())
        }
    }

    fn host(acis_up: bool) -> Arc<Recorded> {
        Arc::new(Recorded {
            asked: Mutex::new(Vec::new()),
            acis_up,
        })
    }

    fn place() -> Place {
        Place {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            units: None,
            utc_offset_seconds: Some(-5 * 3600),
            offset_changes: Vec::new(),
        }
    }

    /// 2026-11-02 10:00 EST, the day the fixtures were recorded.
    const NOW: i64 = 1_793_631_600;

    #[test]
    fn history_from_acis_is_kept_for_the_day() {
        let settings = default_settings();
        assert!(history_due(place(), settings.clone(), None, NOW));
        let net = host(true);
        let shared: Arc<dyn FetchHost> = net.clone();
        let outcome =
            pollster::block_on(fetch_history(place(), settings.clone(), None, NOW, shared))
                .unwrap();
        assert_eq!(outcome.normals_json, None, "ACIS has its own normals");
        let k = kept(&outcome.history_json).unwrap();
        assert_eq!(k.history.station.as_ref().unwrap().id, "NYC");
        assert_eq!(k.history.location.name, "New York");
        assert!(k.history.climate_on(k.today).unwrap().normal_max.is_some());
        let json = Some(outcome.history_json);
        assert!(!history_due(
            place(),
            settings.clone(),
            json.clone(),
            NOW + 3_600
        ));
        // The next day after 00:15 there.
        assert!(history_due(
            place(),
            settings.clone(),
            json.clone(),
            NOW + 15 * 3_600
        ));
        // A history kept before versions is fetched again.
        let mut old: serde_json::Value = serde_json::from_str(json.as_ref().unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("version");
        assert!(history_due(
            place(),
            settings.clone(),
            Some(old.to_string()),
            NOW + 3_600
        ));
        // One day of history: no month so far.
        let one = Settings {
            history_days: 1,
            ..settings.clone()
        };
        let req = requirements(&one, &place());
        assert_eq!((req.past_days, req.month_to_date), (1, false));
        // History off: never due.
        let off = Settings {
            history: false,
            ..settings
        };
        assert!(!history_due(place(), off, None, NOW));
    }

    #[test]
    fn without_acis_open_meteo_computes_normals_to_keep() {
        let net = host(false);
        let shared: Arc<dyn FetchHost> = net.clone();
        let outcome = pollster::block_on(fetch_history(
            place(),
            default_settings(),
            None,
            NOW,
            shared,
        ));
        // The recorded archive is the July query's, not 1991–2020: the normals
        // request still parses, so normals come back to keep.
        let outcome = outcome.unwrap();
        assert!(outcome.normals_json.is_some());
        let normals = outcome.normals_json.clone();
        let again = host(false);
        let shared: Arc<dyn FetchHost> = again.clone();
        pollster::block_on(fetch_history(
            place(),
            default_settings(),
            normals,
            NOW,
            shared,
        ))
        .unwrap();
        assert!(
            !again
                .asked
                .lock()
                .unwrap()
                .iter()
                .any(|u| u.contains("archive-api")),
            "kept normals spare the archive"
        );
    }

    fn july() -> Trip {
        Trip {
            from_year: 2027,
            from_month: 7,
            from_day: 12,
            to_year: 2027,
            to_month: 7,
            to_day: 19,
            first_year: 1995,
            last_year: 2024,
        }
    }

    #[test]
    fn a_trip_planned_on_the_phone() {
        assert_eq!(trip_problem(july()), None);
        let long = Trip {
            to_month: 8,
            to_day: 20,
            ..july()
        };
        assert_eq!(trip_problem(long).as_deref(), Some("at most 31 days"));
        let net = host(true);
        let shared: Arc<dyn FetchHost> = net.clone();
        let climate = pollster::block_on(fetch_climate(
            place(),
            default_settings(),
            july(),
            NOW,
            shared,
        ))
        .unwrap();
        let mut settings = default_settings();
        settings.units = crate::settings::Units::Imperial;
        let plan: serde_json::Value =
            serde_json::from_str(&plan_view(place(), settings, july(), climate).unwrap()).unwrap();
        assert_eq!(plan["title"], "Jul 12 – Jul 19, as in 1995–2024");
        assert_eq!(plan["days"][0]["high"]["text"], "84°");
        assert_eq!(plan["days"][0]["hottest"], "94° in 2010");
        assert!(
            plan["credit"]
                .as_str()
                .unwrap()
                .starts_with("NY City Central Park")
        );
        // ACIS down: the archive answers.
        let down = host(false);
        let shared: Arc<dyn FetchHost> = down.clone();
        let climate = pollster::block_on(fetch_climate(
            place(),
            default_settings(),
            july(),
            NOW,
            shared,
        ))
        .unwrap();
        let h: History = serde_json::from_str(&climate).unwrap();
        assert_eq!(h.kind, whether_core::history::HistoryKind::Reanalysis);
        assert_eq!(h.climate.len(), 8);
    }
}
