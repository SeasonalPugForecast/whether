//! The Settings screen's key check and call count.

use std::sync::Arc;

use whether_core::model::ProviderId;
use whether_core::time::UnixTime;
use whether_providers::UsageLedger;
use whether_providers::openweather::{KeyStatus, key_check_request, parse_key_check};

use crate::fetch::{FetchHost, host_request};

/// What OpenWeather said about a key, in words for the Settings screen.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct KeyCheck {
    /// Whether the key works.
    pub accepted: bool,
    /// "Key accepted", or why not.
    pub message: String,
}

impl KeyCheck {
    fn new(status: &KeyStatus) -> Self {
        let message = match status {
            KeyStatus::Accepted => "Key accepted".to_string(),
            KeyStatus::Rejected => {
                "OpenWeather doesn't know this key. New keys can take a few hours to activate."
                    .to_string()
            }
            KeyStatus::Blocked => {
                "OpenWeather is refusing calls with this key for now: too many calls.".to_string()
            }
            KeyStatus::Unavailable(why) => format!("OpenWeather didn't answer ({why})."),
        };
        Self {
            accepted: *status == KeyStatus::Accepted,
            message,
        }
    }
}

/// Asks OpenWeather whether `key` works, through `host`: one call to the free
/// current-weather endpoint, never One Call, so nothing is metered.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn check_openweather_key(key: String, host: Arc<dyn FetchHost>) -> KeyCheck {
    let key = key.trim();
    if key.is_empty() {
        return KeyCheck {
            accepted: false,
            message: "Enter a key first.".to_string(),
        };
    }
    let status = match host.send(host_request(&key_check_request(key))).await {
        Ok(resp) => parse_key_check(resp.status, &resp.body),
        // The system's message may quote the URL, and with it the key.
        Err(e) => KeyStatus::Unavailable(e.to_string().replace(key, "***")),
    };
    KeyCheck::new(&status)
}

/// Calls `provider` ("openweather") has made today by the ledger the app saved
/// (`FetchHost::save_usage`); 0 without a ledger or for an unknown provider.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn calls_today(usage_json: Option<String>, provider: String, now: i64) -> u32 {
    let Ok(provider) = serde_json::from_value::<ProviderId>(serde_json::Value::String(provider))
    else {
        return 0;
    };
    usage_json
        .and_then(|json| serde_json::from_str::<UsageLedger>(&json).ok())
        .map_or(0, |ledger| ledger.used(provider, UnixTime(now)))
}

/// "12 of 500 One Call calls used today", the counts written as the core writes
/// numbers (`view::format::number`), for the Settings and Sources screens.
#[uniffi::export]
pub fn one_call_usage_text(used: u32, limit: u32) -> String {
    use whether_core::view::format::number;
    // Counts are small (a day's share of 1,000): exact in `f32`.
    #[allow(clippy::cast_precision_loss)]
    let (used, limit) = (number(used as f32, 0), number(limit as f32, 0));
    format!("{used} of {limit} One Call calls used today")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::fetch::{HostError, HostRequest, HostResponse};

    /// Answers every request with `status` and `body`, or fails with `error`.
    struct Fake {
        status: u16,
        body: &'static str,
        error: Option<String>,
        asked: Mutex<Vec<String>>,
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
            self.asked.lock().unwrap().push(request.url);
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
            panic!("the key check is never metered")
        }
    }

    fn check(host: &Arc<Fake>, key: &str) -> KeyCheck {
        pollster::block_on(check_openweather_key(key.into(), host.clone()))
    }

    #[test]
    fn an_accepted_key() {
        let host = Fake::answering(
            200,
            include_str!("../../whether-providers/tests/fixtures/openweather/weather_10001.json"),
        );
        let result = check(&host, " KEY1 ");
        assert_eq!(
            result,
            KeyCheck {
                accepted: true,
                message: "Key accepted".into()
            }
        );
        let asked = host.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert!(asked[0].contains("/data/2.5/weather?") && asked[0].contains("appid=KEY1"));
    }

    #[test]
    fn rejected_blocked_and_unavailable() {
        let rejected = Fake::answering(
            401,
            include_str!(
                "../../whether-providers/tests/fixtures/openweather/key_rejected.synthetic.json"
            ),
        );
        let result = check(&rejected, "KEY1");
        assert!(!result.accepted);
        assert!(
            result.message.contains("doesn't know this key"),
            "{result:?}"
        );
        assert!(
            check(&Fake::answering(429, ""), "KEY1")
                .message
                .contains("too many calls")
        );
        assert_eq!(
            check(&Fake::answering(502, ""), "KEY1").message,
            "OpenWeather didn't answer (HTTP 502)."
        );
    }

    #[test]
    fn an_offline_phone_never_shows_the_key() {
        let host = Arc::new(Fake {
            status: 0,
            body: "",
            error: Some("could not connect to https://x/?appid=KEY1".into()),
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
    fn the_usage_line_is_written_by_the_core() {
        assert_eq!(
            one_call_usage_text(12, 500),
            "12 of 500 One Call calls used today"
        );
        assert_eq!(
            one_call_usage_text(0, 1000),
            "0 of 1000 One Call calls used today"
        );
    }

    #[test]
    fn calls_today_reads_the_ledger() {
        // 2026-10-24 12:00 UTC: outside the reset window around midnight.
        let now = 1_792_843_200;
        let mut ledger = UsageLedger::new();
        for _ in 0..3 {
            ledger
                .try_spend(
                    ProviderId::OpenWeather,
                    whether_providers::Budget::daily(900),
                    1,
                    UnixTime(now),
                )
                .unwrap();
        }
        let json = serde_json::to_string(&ledger).unwrap();
        assert_eq!(
            calls_today(Some(json.clone()), "openweather".into(), now),
            3
        );
        assert_eq!(calls_today(Some(json.clone()), "open-meteo".into(), now), 0);
        assert_eq!(
            calls_today(Some(json), "openweather".into(), now + 86_400),
            0,
            "a new UTC day starts at 0"
        );
        assert_eq!(calls_today(None, "openweather".into(), now), 0);
        assert_eq!(calls_today(Some("{".into()), "openweather".into(), now), 0);
    }

    #[test]
    fn calls_today_reads_ledgers_saved_before_periods() {
        // The format the app and the desktop saved before this change: one UTC day
        // (20750 is 2026-10-24) shared by every provider.
        let now = 1_792_843_200;
        assert_eq!(now / 86_400, 20_750);
        let old = r#"{"day":20750,"used":{"openweather":42}}"#.to_string();
        assert_eq!(
            calls_today(Some(old.clone()), "openweather".into(), now),
            42
        );
        assert_eq!(calls_today(Some(old.clone()), "open-meteo".into(), now), 0);
        assert_eq!(
            calls_today(Some(old), "openweather".into(), now + 86_400),
            0
        );
        assert_eq!(calls_today(None, "nobody".into(), now), 0);
    }
}
