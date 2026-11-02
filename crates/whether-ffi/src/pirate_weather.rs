//! Pirate Weather on the Sources screen: the key test and the month's
//! call count, both as text the core wrote.

use std::sync::Arc;

use whether_core::model::ProviderId;
use whether_core::time::UnixTime;
use whether_providers::UsageLedger;
use whether_providers::pirate_weather::{KeyStatus, key_check_request, parse_key_check};

use crate::fetch::{FetchHost, host_request};
use crate::key::KeyCheck;

/// What Pirate Weather said about a key, in words for the Sources screen.
fn key_check(status: &KeyStatus) -> KeyCheck {
    let message = match status {
        KeyStatus::Accepted => "Key accepted".to_string(),
        KeyStatus::Rejected => "Pirate Weather doesn't know this key. Check it, and that it \
             is subscribed to the Forecast API (a new subscription can take 20 minutes to apply)."
            .to_string(),
        KeyStatus::Blocked => {
            "Pirate Weather is refusing calls with this key: its calls for the month are used up."
                .to_string()
        }
        KeyStatus::Unavailable(why) => format!("Pirate Weather didn't answer ({why})."),
    };
    KeyCheck {
        accepted: *status == KeyStatus::Accepted,
        message,
    }
}

/// Asks Pirate Weather whether `key` works, through `host`: one small forecast
/// request (every block but the current conditions left out). It counts as one call
/// of the key's month at Pirate Weather, but not in the app's ledger, which counts
/// forecasts.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn check_pirate_weather_key(key: String, host: Arc<dyn FetchHost>) -> KeyCheck {
    let key = key.trim();
    if key.is_empty() {
        return KeyCheck {
            accepted: false,
            message: "Enter a key first.".to_string(),
        };
    }
    let status = match host.send(host_request(&key_check_request(key))).await {
        Ok(resp) => parse_key_check(resp.status, &resp.body),
        // The system's message may quote what was sent; keep the key out of it.
        Err(e) => KeyStatus::Unavailable(e.to_string().replace(key, "***")),
    };
    key_check(&status)
}

/// "N of LIMIT calls used this month" for `provider` ("pirate-weather"), by the
/// ledger the app saved (`FetchHost::save_usage`); 0 used without a ledger. When
/// the limit is reached it says the next source serves until the month ends.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn month_usage_text(
    usage_json: Option<String>,
    provider: String,
    limit: u32,
    now: i64,
) -> String {
    let used = serde_json::from_value::<ProviderId>(serde_json::Value::String(provider))
        .ok()
        .zip(usage_json)
        .and_then(|(provider, json)| {
            serde_json::from_str::<UsageLedger>(&json)
                .ok()
                .map(|ledger| ledger.used(provider, UnixTime(now)))
        })
        .unwrap_or(0);
    if used >= limit {
        format!("All {limit} calls used this month: the next source serves until it ends")
    } else {
        format!("{used} of {limit} calls used this month")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::fetch::{HostError, HostRequest, HostResponse};
    use whether_providers::Budget;

    const FORECAST: &str =
        include_str!("../../whether-providers/tests/fixtures/pirate_weather/forecast_10001.json");
    const UNAUTHORIZED: &str =
        include_str!("../../whether-providers/tests/fixtures/pirate_weather/error_401.json");

    /// Answers every request with `status` and `body`, or fails with `error`.
    struct Fake {
        status: u16,
        body: &'static str,
        error: Option<String>,
        asked: Mutex<Vec<HostRequest>>,
    }

    impl Fake {
        fn answering(status: u16, body: &'static str) -> Arc<Self> {
            Arc::new(Self {
                status,
                body,
                error: None,
                asked: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait::async_trait]
    impl FetchHost for Fake {
        async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
            self.asked.lock().unwrap().push(request);
            if let Some(message) = &self.error {
                return Err(HostError::Failed {
                    message: message.clone(),
                });
            }
            Ok(HostResponse {
                status: self.status,
                body: self.body.as_bytes().to_vec(),
                date: None,
                expires: None,
                last_modified: None,
            })
        }

        fn save_usage(&self, _ledger_json: String) -> Result<(), HostError> {
            panic!("the key check never writes the ledger")
        }
    }

    fn check(host: &Arc<Fake>, key: &str) -> KeyCheck {
        pollster::block_on(check_pirate_weather_key(key.into(), host.clone()))
    }

    #[test]
    fn an_accepted_key_is_one_small_request_with_the_key_in_the_header() {
        let host = Fake::answering(200, FORECAST);
        assert_eq!(
            check(&host, " KEY1 "),
            KeyCheck {
                accepted: true,
                message: "Key accepted".into()
            }
        );
        let asked = host.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        let request = &asked[0];
        assert!(
            request.url.contains("/forecast/key/0,0?"),
            "{}",
            request.url
        );
        assert!(
            request
                .url
                .contains("exclude=minutely,hourly,daily,alerts,summary")
        );
        assert!(!request.url.contains("KEY1"), "never in the URL");
        let key = request.headers.iter().find(|h| h.name == "apikey");
        assert_eq!(key.map(|h| h.value.as_str()), Some("KEY1"));
        assert!(!format!("{request:?}").contains("KEY1"), "Debug masks it");
    }

    #[test]
    fn rejected_blocked_and_unavailable() {
        let rejected = check(&Fake::answering(401, UNAUTHORIZED), "KEY1");
        assert!(!rejected.accepted);
        assert!(
            rejected.message.contains("doesn't know this key"),
            "{rejected:?}"
        );
        assert!(rejected.message.contains("Forecast API"));
        assert!(
            check(&Fake::answering(429, ""), "KEY1")
                .message
                .contains("used up")
        );
        assert_eq!(
            check(&Fake::answering(502, ""), "KEY1").message,
            "Pirate Weather didn't answer (HTTP 502)."
        );
    }

    #[test]
    fn an_offline_phone_never_shows_the_key() {
        let host = Arc::new(Fake {
            status: 0,
            body: "",
            error: Some("could not connect (apikey: KEY1)".into()),
            asked: Mutex::new(Vec::new()),
        });
        let result = check(&host, "KEY1");
        assert!(!result.accepted);
        assert!(!result.message.contains("KEY1"), "{result:?}");
    }

    #[test]
    fn an_empty_key_sends_nothing() {
        let host = Fake::answering(200, "{}");
        assert!(!check(&host, "  ").accepted);
        assert!(host.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn the_month_text_counts_the_ledger() {
        // 2026-10-24 12:00 UTC.
        let now = 1_792_843_200;
        let mut ledger = UsageLedger::new();
        for _ in 0..12 {
            ledger
                .try_spend(
                    ProviderId::PirateWeather,
                    Budget::monthly(9000),
                    1,
                    UnixTime(now),
                )
                .unwrap();
        }
        let json = serde_json::to_string(&ledger).unwrap();
        let text = |json: Option<String>, provider: &str, now: i64| {
            month_usage_text(json, provider.into(), 9000, now)
        };
        assert_eq!(
            text(Some(json.clone()), "pirate-weather", now),
            "12 of 9000 calls used this month"
        );
        // Later the same month, and the next month, which starts at 0.
        assert_eq!(
            text(Some(json.clone()), "pirate-weather", now + 5 * 86_400),
            "12 of 9000 calls used this month"
        );
        assert_eq!(
            text(Some(json.clone()), "pirate-weather", now + 8 * 86_400),
            "0 of 9000 calls used this month",
            "November"
        );
        // Another provider, no ledger, a ledger that does not read.
        assert_eq!(
            text(Some(json.clone()), "openweather", now),
            "0 of 9000 calls used this month"
        );
        assert_eq!(
            text(None, "pirate-weather", now),
            "0 of 9000 calls used this month"
        );
        assert_eq!(
            text(Some("{".into()), "pirate-weather", now),
            "0 of 9000 calls used this month"
        );
        // The limit lowered below what is used, or reached.
        assert_eq!(
            month_usage_text(Some(json), "pirate-weather".into(), 12, now),
            "All 12 calls used this month: the next source serves until it ends"
        );
    }
}
