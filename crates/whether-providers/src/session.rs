//! The sans-IO fetch state machine shared by the Linux runtime and the firmware.
//!
//! The caller loops: take [`FetchSession::next_request`], perform it, and report the
//! result with [`FetchSession::on_response`] or [`FetchSession::on_transport_error`].
//! The session walks the provider chain, falls back when a required request fails,
//! then runs supplements that fill gaps in the primary snapshot.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use whether_core::builder::{ModelError, SnapshotBuilder};
use whether_core::derive::fill_derived;
use whether_core::model::{Alert, ProviderId, ResolvedLocation, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::time::UnixTime;

use crate::error::{ProviderError, check_status};
use crate::http::{Endpoint, FetchContext, HttpRequest, HttpResponse, Provider};
use crate::url::Coordinates;
use crate::usage::{QuotaError, UsageLedger};

/// One request's outcome, kept so displays can show partial failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// The provider asked.
    pub provider: ProviderId,
    /// The resource requested.
    pub endpoint: Endpoint,
    /// `None` on success.
    pub error: Option<ProviderError>,
}

/// Alert detail requests one fetch sends at most; further new alerts wait for a
/// later fetch.
pub const MAX_ALERT_DETAILS: usize = 5;

/// Follow-up requests ([`Provider::follow_up`]) one provider may add per fetch;
/// more are refused, so a misbehaving response cannot loop.
pub const MAX_FOLLOW_UPS: usize = 4;

/// Known alert details older than this are requested again while the alert lasts,
/// so updates (a later end, a stronger wording) reach the app.
pub const ALERT_RECHECK_S: i64 = 3_600;

/// Why a fetch produced no snapshot.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// Every provider in the chain failed.
    #[error("all providers failed")]
    AllProvidersFailed(Vec<Attempt>),
    /// The server reported no change; keep the cached snapshot.
    #[error("not modified")]
    NotModified,
    /// The collected data could not form a snapshot.
    #[error(transparent)]
    Model(#[from] ModelError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Trying `chain[i]`.
    Primary(usize),
    /// Running `supplements[i]`.
    Supplement(usize),
    Done,
}

/// Drives the requests of one location's fetch.
pub struct FetchSession<'p> {
    chain: &'p [&'p dyn Provider],
    supplements: &'p [&'p dyn Provider],
    loc: Coordinates,
    req: DataRequirements,
    ctx: FetchContext,
    phase: Phase,
    /// Whether the current provider has been planned.
    planned: bool,
    pending: VecDeque<HttpRequest>,
    in_flight: Option<HttpRequest>,
    builder: SnapshotBuilder,
    primary: Option<WeatherSnapshot>,
    /// The provider that produced `primary`.
    primary_provider: Option<&'p dyn Provider>,
    attempts: Vec<Attempt>,
    not_modified: bool,
    /// Daily call counts; metered requests are refused without one.
    usage: Option<UsageLedger>,
    /// Alerts the caller already has, reused instead of requesting their details.
    known_alerts: Vec<Alert>,
    /// Alert ids already handled by the current provider.
    alert_ids: Vec<String>,
    /// Alert detail requests queued by the current provider.
    alert_requests: usize,
    /// Follow-up requests queued by the current provider.
    follow_ups: usize,
}

impl core::fmt::Debug for FetchSession<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FetchSession")
            .field("phase", &self.phase)
            .field("pending", &self.pending.len())
            .field("attempts", &self.attempts)
            .finish_non_exhaustive()
    }
}

impl<'p> FetchSession<'p> {
    /// A session over `chain` (in fallback order) and `supplements`.
    pub fn new(
        chain: &'p [&'p dyn Provider],
        supplements: &'p [&'p dyn Provider],
        loc: Coordinates,
        req: DataRequirements,
        ctx: FetchContext,
    ) -> Self {
        let first = chain.first().map_or(ProviderId::OpenMeteo, |p| p.id());
        Self {
            chain,
            supplements,
            loc,
            req,
            ctx,
            phase: if chain.is_empty() {
                Phase::Done
            } else {
                Phase::Primary(0)
            },
            planned: false,
            pending: VecDeque::new(),
            in_flight: None,
            builder: SnapshotBuilder::new(first),
            primary: None,
            primary_provider: None,
            attempts: Vec::new(),
            not_modified: false,
            usage: None,
            known_alerts: Vec::new(),
            alert_ids: Vec::new(),
            alert_requests: 0,
            follow_ups: 0,
        }
    }

    /// Alerts from earlier fetches (normally the cached snapshots). A response that
    /// names one of them by id reuses it, if it has not ended, instead of
    /// requesting its details again.
    #[must_use]
    pub fn with_known_alerts(mut self, alerts: Vec<Alert>) -> Self {
        self.known_alerts = alerts.into_iter().filter(|a| a.id.is_some()).collect();
        self
    }

    /// Attaches the persisted call counts. Without a ledger, metered requests are
    /// never sent. The caller must persist [`Self::usage`] after each
    /// [`Self::next_request`] that returns a metered request and before sending it.
    #[must_use]
    pub fn with_usage(mut self, ledger: UsageLedger) -> Self {
        self.usage = Some(ledger);
        self
    }

    /// The call counts, including every metered request handed out so far.
    pub fn usage(&self) -> Option<&UsageLedger> {
        self.usage.as_ref()
    }

    /// Counts one metered call for `provider`, or says why it may not be sent.
    fn spend(&mut self, provider: &dyn Provider, cost: u32) -> Result<(), QuotaError> {
        let budget = provider.budget().ok_or(QuotaError::NoLimit)?;
        let ledger = self.usage.as_mut().ok_or(QuotaError::NoLedger)?;
        ledger.try_spend(provider.id(), budget, cost, self.ctx.now)
    }

    fn provider(&self) -> Option<&'p dyn Provider> {
        match self.phase {
            Phase::Primary(i) => self.chain.get(i).copied(),
            Phase::Supplement(i) => self.supplements.get(i).copied(),
            Phase::Done => None,
        }
    }

    /// The next request to perform, or `None` when the session is finished.
    pub fn next_request(&mut self) -> Option<&HttpRequest> {
        loop {
            let provider = self.provider()?;
            if !self.planned {
                self.planned = true;
                self.builder = SnapshotBuilder::new(provider.id());
                if let (Phase::Supplement(_), Some(primary)) = (self.phase, &self.primary) {
                    // The requested place at the primary's offset, for supplements
                    // whose responses carry no location (alerts only); providers
                    // that report one replace it.
                    self.builder.set_location(ResolvedLocation {
                        latitude: self.loc.latitude,
                        longitude: self.loc.longitude,
                        utc_offset: primary.location.utc_offset,
                        ..ResolvedLocation::default()
                    });
                }
                self.alert_ids.clear();
                self.alert_requests = 0;
                self.follow_ups = 0;
                match provider.plan(&self.loc, &self.req, &self.ctx) {
                    Ok(requests) => self.pending = requests.into(),
                    Err(error) => {
                        self.record(provider.id(), Endpoint::Forecast, Some(error));
                        self.advance();
                        continue;
                    }
                }
            }
            if let Some(next) = self.pending.pop_front() {
                if next.metered
                    && let Err(quota) = self.spend(provider, provider.request_cost(&next))
                {
                    // Not sent: a required request falls back to the next provider.
                    self.fail(provider.id(), &next, ProviderError::Quota(quota));
                    continue;
                }
                self.in_flight = Some(next);
                return self.in_flight.as_ref();
            }
            self.complete_provider();
        }
    }

    /// Reports the response to the request last returned by [`Self::next_request`].
    pub fn on_response(&mut self, resp: &HttpResponse<'_>) {
        let (Some(request), Some(provider)) = (self.in_flight.take(), self.provider()) else {
            return;
        };
        let result = check_status(resp)
            .map_err(|e| provider.explain(e, resp))
            .and_then(|()| provider.parse(request.endpoint, resp, &self.ctx, &mut self.builder))
            .and_then(|()| provider.follow_up(request.endpoint, resp, &self.ctx));
        match result {
            Ok(follow_ups) => {
                self.record(provider.id(), request.endpoint, None);
                self.follow_alerts(provider);
                self.queue_follow_ups(provider, &request, follow_ups);
            }
            Err(error) => {
                self.builder.take_alert_refs();
                self.fail(provider.id(), &request, error);
            }
        }
    }

    /// Handles the alert ids the last response named and queues detail requests
    /// right after it, within [`MAX_ALERT_DETAILS`]: new alerts first, then known
    /// alerts checked more than [`ALERT_RECHECK_S`] ago. Known alerts are kept in the
    /// snapshot meanwhile; fresh details replace them.
    fn follow_alerts(&mut self, provider: &dyn Provider) {
        let refs = self.builder.take_alert_refs();
        if !self.req.alerts {
            return;
        }
        let now = self.ctx.now;
        let (mut new, mut recheck) = (Vec::new(), Vec::new());
        for id in refs {
            if self.alert_ids.contains(&id) {
                continue;
            }
            self.alert_ids.push(id.clone());
            let known = self
                .known_alerts
                .iter()
                .find(|a| a.id.as_deref() == Some(id.as_str()) && a.end > now);
            match known {
                Some(alert) => {
                    let fresh = alert
                        .checked_at
                        .is_some_and(|t| now.0 - t.0 < ALERT_RECHECK_S);
                    self.builder.push_alert(alert.clone());
                    if !fresh {
                        recheck.push(id);
                    }
                }
                None => new.push(id),
            }
        }
        new.append(&mut recheck);
        let mut queued = Vec::new();
        for id in new {
            if self.alert_requests >= MAX_ALERT_DETAILS {
                break;
            }
            if let Some(mut request) = provider.alert_request(&id, &self.ctx) {
                // A missing alert never costs the provider its other data.
                request.required = false;
                self.alert_requests += 1;
                queued.push(request);
            }
        }
        for request in queued.into_iter().rev() {
            self.pending.push_front(request);
        }
    }

    /// Queues the requests `parent`'s response called for ([`Provider::follow_up`]),
    /// right after it and in order, within [`MAX_FOLLOW_UPS`]. A follow-up of a
    /// required request is required.
    fn queue_follow_ups(
        &mut self,
        provider: &dyn Provider,
        parent: &HttpRequest,
        requests: Vec<HttpRequest>,
    ) {
        let mut queued = Vec::new();
        for mut request in requests {
            if self.follow_ups >= MAX_FOLLOW_UPS {
                self.record(
                    provider.id(),
                    request.endpoint,
                    Some(ProviderError::Malformed("too many follow-ups".into())),
                );
                continue;
            }
            self.follow_ups += 1;
            request.required |= parent.required;
            queued.push(request);
        }
        for request in queued.into_iter().rev() {
            self.pending.push_front(request);
        }
    }

    /// Reports that the request last returned by [`Self::next_request`] got no response.
    pub fn on_transport_error(&mut self, message: &str) {
        let (Some(request), Some(provider)) = (self.in_flight.take(), self.provider()) else {
            return;
        };
        self.fail(
            provider.id(),
            &request,
            ProviderError::Transport(message.to_string()),
        );
    }

    fn fail(&mut self, id: ProviderId, request: &HttpRequest, error: ProviderError) {
        let not_modified = error == ProviderError::NotModified;
        self.record(id, request.endpoint, Some(error));
        if !request.required {
            return;
        }
        if not_modified && matches!(self.phase, Phase::Primary(_)) {
            self.not_modified = true;
            self.phase = Phase::Done;
            self.pending.clear();
            return;
        }
        self.advance();
    }

    fn record(&mut self, provider: ProviderId, endpoint: Endpoint, error: Option<ProviderError>) {
        self.attempts.push(Attempt {
            provider,
            endpoint,
            error,
        });
    }

    /// All of the current provider's requests ran; keep its data.
    fn complete_provider(&mut self) {
        let Some(provider) = self.provider() else {
            return;
        };
        let builder = core::mem::replace(&mut self.builder, SnapshotBuilder::new(provider.id()));
        match (builder.finish(self.ctx.now), self.phase) {
            (Ok(snapshot), Phase::Primary(i)) => {
                self.primary = Some(snapshot);
                self.primary_provider = self.chain.get(i).copied();
                self.start_supplements();
            }
            (Ok(snapshot), Phase::Supplement(_)) => {
                if let Some(primary) = self.primary.as_mut() {
                    primary.fill_gaps_from(&snapshot);
                }
                self.advance();
            }
            (Err(e), _) => {
                self.record(
                    provider.id(),
                    Endpoint::Forecast,
                    Some(ProviderError::Malformed(e.to_string())),
                );
                self.advance();
            }
            (Ok(_), Phase::Done) => {}
        }
    }

    fn start_supplements(&mut self) {
        self.phase = Phase::Supplement(0);
        self.planned = false;
        self.pending.clear();
        self.skip_redundant_supplements();
    }

    /// Moves to the next provider in the current phase.
    fn advance(&mut self) {
        self.planned = false;
        self.pending.clear();
        self.phase = match self.phase {
            Phase::Primary(i) if i + 1 < self.chain.len() => Phase::Primary(i + 1),
            Phase::Primary(_) | Phase::Done => Phase::Done,
            Phase::Supplement(i) => Phase::Supplement(i + 1),
        };
        self.skip_redundant_supplements();
    }

    /// Skips supplements that are the primary's own provider with the same
    /// settings (another Open-Meteo model may fill what the primary's lacks), and
    /// ends the session after the last supplement.
    fn skip_redundant_supplements(&mut self) {
        let primary = self.primary_provider.map(|p| (p.id(), p.variant()));
        while let Phase::Supplement(i) = self.phase {
            match self.supplements.get(i) {
                None => self.phase = Phase::Done,
                Some(p) if Some((p.id(), p.variant())) == primary => {
                    self.phase = Phase::Supplement(i + 1);
                }
                Some(_) => break,
            }
        }
    }

    /// The snapshot (with every attempt) or why there is none.
    pub fn finish(
        self,
        fetched_at: UnixTime,
    ) -> Result<(WeatherSnapshot, Vec<Attempt>), FetchError> {
        if self.not_modified {
            return Err(FetchError::NotModified);
        }
        match self.primary {
            Some(mut snapshot) => {
                snapshot.fetched_at = fetched_at;
                fill_derived(&mut snapshot);
                Ok((snapshot, self.attempts))
            }
            None => Err(FetchError::AllProvidersFailed(self.attempts)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Attribution, Capabilities, Method};
    use crate::usage::{Budget, Period};
    use alloc::string::String;
    use alloc::vec;
    use whether_core::model::{AirQuality, Current, ResolvedLocation};
    use whether_core::units::Celsius;

    /// A fake provider: each planned endpoint's response body is either a number
    /// (parsed into the snapshot) or "bad" (malformed).
    struct Fake {
        id: ProviderId,
        plan: Vec<(Endpoint, bool)>,
        plan_error: bool,
        hint: Option<&'static str>,
        /// Requests at these indices are metered.
        metered: Vec<usize>,
        /// The budget's limit; a month budget if `month`, else a day's.
        limit: Option<u32>,
        month: bool,
        /// What each metered request costs.
        cost: u32,
    }

    impl Fake {
        fn new(id: ProviderId, plan: &[(Endpoint, bool)]) -> Self {
            Self {
                id,
                plan: plan.to_vec(),
                plan_error: false,
                hint: None,
                metered: vec![],
                limit: None,
                month: false,
                cost: 1,
            }
        }
    }

    impl Provider for Fake {
        fn id(&self) -> ProviderId {
            self.id
        }
        fn attribution(&self) -> Attribution {
            Attribution {
                text: "fake",
                url: "https://example.com",
                license: "none",
            }
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::CURRENT
        }
        fn budget(&self) -> Option<Budget> {
            self.limit.map(if self.month {
                Budget::monthly
            } else {
                Budget::daily
            })
        }
        fn request_cost(&self, _: &HttpRequest) -> u32 {
            self.cost
        }
        fn explain(&self, error: ProviderError, _: &HttpResponse<'_>) -> ProviderError {
            match (error, self.hint) {
                (ProviderError::Unauthorized(_), Some(hint)) => {
                    ProviderError::Unauthorized(hint.into())
                }
                (error, _) => error,
            }
        }
        fn plan(
            &self,
            _: &Coordinates,
            _: &DataRequirements,
            _: &FetchContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            if self.plan_error {
                return Err(ProviderError::Config("no key".into()));
            }
            Ok(self
                .plan
                .iter()
                .enumerate()
                .map(|(i, &(endpoint, required))| HttpRequest {
                    url: alloc::format!("https://{}/{endpoint:?}", self.id.id()),
                    headers: vec![],
                    endpoint,
                    required,
                    metered: self.metered.contains(&i),
                    method: Method::Get,
                    body: None,
                })
                .collect())
        }
        fn parse(
            &self,
            endpoint: Endpoint,
            resp: &HttpResponse<'_>,
            _: &FetchContext,
            out: &mut SnapshotBuilder,
        ) -> Result<(), ProviderError> {
            let text = core::str::from_utf8(resp.body).unwrap_or("bad");
            let value: f32 = text
                .parse()
                .map_err(|_| ProviderError::Malformed(text.into()))?;
            match endpoint {
                Endpoint::Forecast | Endpoint::Current => {
                    out.set_location(ResolvedLocation {
                        name: "x".into(),
                        ..Default::default()
                    });
                    out.set_current(Current {
                        temperature: Some(Celsius(value)),
                        ..Default::default()
                    });
                }
                Endpoint::AirQuality => out.set_air_quality(AirQuality {
                    us_aqi: Some(value as u16),
                    ..Default::default()
                }),
                Endpoint::Hourly
                | Endpoint::Daily
                | Endpoint::Nowcast
                | Endpoint::Sun
                | Endpoint::Geocode
                | Endpoint::Alert
                | Endpoint::Radar
                | Endpoint::Point
                | Endpoint::Stations
                | Endpoint::Observation
                | Endpoint::Climate => {}
            }
            Ok(())
        }
    }

    fn ctx() -> FetchContext {
        FetchContext {
            now: UnixTime(1_000),
            user_agent: "test".into(),
            utc_offset_hint: None,
            if_modified_since: None,
            language: "en".into(),
        }
    }

    type Reply = Result<(u16, &'static str), &'static str>;
    type Outcome = Result<(WeatherSnapshot, Vec<Attempt>), FetchError>;

    /// Drives a session; `reply(url)` answers each request. Returns the result and the
    /// URLs requested.
    fn drive(
        chain: &[&dyn Provider],
        supplements: &[&dyn Provider],
        reply: impl Fn(&str) -> Reply,
    ) -> (Outcome, Vec<String>) {
        let (outcome, urls, _) = drive_metered(chain, supplements, None, reply);
        (outcome, urls)
    }

    /// Like [`drive`], with an optional usage ledger, which is returned updated.
    fn drive_metered(
        chain: &[&dyn Provider],
        supplements: &[&dyn Provider],
        ledger: Option<UsageLedger>,
        reply: impl Fn(&str) -> Reply,
    ) -> (Outcome, Vec<String>, Option<UsageLedger>) {
        let loc = Coordinates {
            latitude: 40.7,
            longitude: -74.0,
        };
        let mut s = FetchSession::new(chain, supplements, loc, DataRequirements::full(), ctx());
        if let Some(ledger) = ledger {
            s = s.with_usage(ledger);
        }
        let mut urls = Vec::new();
        while let Some(req) = s.next_request() {
            let url = req.url.clone();
            urls.push(url.clone());
            match reply(&url) {
                Ok((status, body)) => s.on_response(&HttpResponse::new(status, body.as_bytes())),
                Err(msg) => s.on_transport_error(msg),
            }
        }
        let ledger = s.usage().cloned();
        (s.finish(UnixTime(2_000)), urls, ledger)
    }

    const OWM: ProviderId = ProviderId::OpenWeather;
    const OM: ProviderId = ProviderId::OpenMeteo;

    #[test]
    fn all_requests_succeed() {
        let a = Fake::new(
            OWM,
            &[(Endpoint::Current, true), (Endpoint::AirQuality, false)],
        );
        let (result, urls) = drive(&[&a], &[], |_| Ok((200, "21")));
        let (snap, attempts) = result.unwrap();
        assert_eq!(snap.provider, OWM);
        assert_eq!(snap.fetched_at, UnixTime(2_000));
        assert_eq!(snap.air_quality.unwrap().us_aqi, Some(21));
        assert_eq!(urls.len(), 2);
        assert!(attempts.iter().all(|a| a.error.is_none()));
    }

    #[test]
    fn a_failed_optional_request_is_skipped() {
        let a = Fake::new(
            OWM,
            &[(Endpoint::Current, true), (Endpoint::AirQuality, false)],
        );
        let (result, _) = drive(&[&a], &[], |url| {
            if url.ends_with("AirQuality") {
                Ok((500, ""))
            } else {
                Ok((200, "21"))
            }
        });
        let (snap, attempts) = result.unwrap();
        assert!(snap.air_quality.is_none());
        assert_eq!(
            attempts[1].error,
            Some(ProviderError::Http {
                status: 500,
                message: None
            })
        );
    }

    #[test]
    fn a_failed_required_request_falls_back() {
        let a = Fake::new(
            OWM,
            &[(Endpoint::Current, true), (Endpoint::Forecast, true)],
        );
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, urls) = drive(&[&a, &b], &[], |url| {
            if url.starts_with("https://openweather") {
                Ok((401, r#"{"message":"Invalid API key"}"#))
            } else {
                Ok((200, "15"))
            }
        });
        let (snap, attempts) = result.unwrap();
        assert_eq!(snap.provider, OM);
        assert_eq!(urls.len(), 2, "the second OpenWeather request is skipped");
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Unauthorized("Invalid API key".into()))
        );
    }

    #[test]
    fn providers_can_explain_status_errors() {
        let mut a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        a.hint = Some("new keys take a few hours to activate");
        let (result, _) = drive(&[&a], &[], |_| Ok((401, "")));
        let Err(FetchError::AllProvidersFailed(attempts)) = result else {
            panic!("expected failure")
        };
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Unauthorized(
                "new keys take a few hours to activate".into()
            ))
        );
    }

    #[test]
    fn all_providers_failing() {
        let a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, _) = drive(&[&a, &b], &[], |_| Ok((503, "")));
        let Err(FetchError::AllProvidersFailed(attempts)) = result else {
            panic!("expected failure")
        };
        assert_eq!(attempts.len(), 2);
    }

    #[test]
    fn not_modified_ends_the_session() {
        let a = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let b = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let (result, urls) = drive(&[&a, &b], &[], |_| Ok((304, "")));
        assert_eq!(result.unwrap_err(), FetchError::NotModified);
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn a_transport_error_mid_chain_falls_back() {
        let a = Fake::new(
            OWM,
            &[(Endpoint::Current, true), (Endpoint::Forecast, true)],
        );
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, _) = drive(&[&a, &b], &[], |url| match url {
            "https://openweather/Forecast" => Err("connection reset"),
            _ => Ok((200, "12")),
        });
        let (snap, attempts) = result.unwrap();
        assert_eq!(snap.provider, OM);
        assert_eq!(
            attempts[1].error,
            Some(ProviderError::Transport("connection reset".into()))
        );
    }

    #[test]
    fn plan_errors_fall_back() {
        let mut a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        a.plan_error = true;
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, urls) = drive(&[&a, &b], &[], |_| Ok((200, "9")));
        let (snap, attempts) = result.unwrap();
        assert_eq!((snap.provider, urls.len()), (OM, 1));
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Config("no key".into()))
        );
    }

    #[test]
    fn malformed_required_data_falls_back() {
        let a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, _) = drive(&[&a, &b], &[], |url| {
            if url.starts_with("https://openweather") {
                Ok((200, "bad"))
            } else {
                Ok((200, "9"))
            }
        });
        assert_eq!(result.unwrap().0.provider, OM);
    }

    #[test]
    fn a_supplement_fills_gaps() {
        let a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let b = Fake::new(
            OM,
            &[(Endpoint::Forecast, true), (Endpoint::AirQuality, false)],
        );
        let (result, urls) = drive(&[&a], &[&b], |url| {
            if url.starts_with("https://openweather") {
                Ok((200, "20"))
            } else {
                Ok((200, "35"))
            }
        });
        let (snap, _) = result.unwrap();
        assert_eq!(urls.len(), 3);
        assert_eq!(snap.provider, OWM);
        assert_eq!(
            snap.current.unwrap().temperature,
            Some(Celsius(20.0)),
            "primary wins"
        );
        assert_eq!(snap.air_quality.unwrap().us_aqi, Some(35));
        assert_eq!(snap.supplemented_by, [OM]);
    }

    #[test]
    fn a_failing_supplement_does_not_fail_the_fetch() {
        let a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (result, _) = drive(&[&a], &[&b], |url| {
            if url.starts_with("https://openweather") {
                Ok((200, "20"))
            } else {
                Err("timeout")
            }
        });
        let (snap, attempts) = result.unwrap();
        assert_eq!(snap.supplemented_by, Vec::<ProviderId>::new());
        assert_eq!(
            attempts.last().unwrap().error,
            Some(ProviderError::Transport("timeout".into()))
        );
    }

    #[test]
    fn a_supplement_equal_to_the_primary_is_skipped() {
        let a = Fake::new(OWM, &[(Endpoint::Current, true)]);
        let b = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        // OpenWeather fails, Open-Meteo becomes primary, so the Open-Meteo supplement
        // is skipped.
        let (result, urls) = drive(&[&a, &b], &[&b], |url| {
            if url.starts_with("https://openweather") {
                Ok((500, ""))
            } else {
                Ok((200, "7"))
            }
        });
        assert_eq!(result.unwrap().0.provider, OM);
        assert_eq!(
            urls,
            ["https://openweather/Current", "https://open-meteo/Forecast"]
        );
    }

    #[test]
    fn an_empty_chain_fails_immediately() {
        let (result, urls) = drive(&[], &[], |_| Ok((200, "1")));
        assert_eq!(urls, Vec::<String>::new());
        assert_eq!(result.unwrap_err(), FetchError::AllProvidersFailed(vec![]));
    }

    #[test]
    fn derived_values_are_filled() {
        struct Humid;
        impl Provider for Humid {
            fn id(&self) -> ProviderId {
                OM
            }
            fn attribution(&self) -> Attribution {
                Attribution {
                    text: "",
                    url: "",
                    license: "",
                }
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::CURRENT
            }
            fn plan(
                &self,
                _: &Coordinates,
                _: &DataRequirements,
                _: &FetchContext,
            ) -> Result<Vec<HttpRequest>, ProviderError> {
                Ok(vec![HttpRequest {
                    url: "u".into(),
                    headers: vec![],
                    endpoint: Endpoint::Forecast,
                    required: true,
                    metered: false,
                    method: Method::Get,
                    body: None,
                }])
            }
            fn parse(
                &self,
                _: Endpoint,
                _: &HttpResponse<'_>,
                _: &FetchContext,
                out: &mut SnapshotBuilder,
            ) -> Result<(), ProviderError> {
                out.set_location(ResolvedLocation::default());
                out.set_current(Current {
                    temperature: Some(Celsius(20.0)),
                    humidity: Some(whether_core::units::Percent(50.0)),
                    ..Default::default()
                });
                Ok(())
            }
        }
        let (result, _) = drive(&[&Humid], &[], |_| Ok((200, "")));
        assert!(result.unwrap().0.current.unwrap().dew_point.is_some());
    }

    /// OpenWeather with a metered current request and an unmetered air-quality one.
    fn metered_owm(limit: Option<u32>) -> Fake {
        Fake {
            metered: vec![0],
            limit,
            ..Fake::new(
                OWM,
                &[(Endpoint::Current, true), (Endpoint::AirQuality, false)],
            )
        }
    }

    /// Answers every request with a temperature.
    const ANSWER_ALL: fn(&str) -> Reply = |_| Ok((200, "20"));

    #[test]
    fn metered_requests_count_against_the_ledger() {
        let owm = metered_owm(Some(2));
        let (outcome, urls, ledger) =
            drive_metered(&[&owm], &[], Some(UsageLedger::new()), ANSWER_ALL);
        assert!(outcome.is_ok());
        assert_eq!(urls.len(), 2);
        assert_eq!(
            ledger.unwrap().used(OWM, ctx().now),
            1,
            "only the metered request counts"
        );
    }

    #[test]
    fn metered_requests_are_never_sent_without_a_ledger() {
        let owm = metered_owm(Some(1000));
        let om = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (outcome, urls, _) = drive_metered(&[&owm, &om], &[], None, ANSWER_ALL);
        assert_eq!(
            urls,
            vec!["https://open-meteo/Forecast"],
            "OpenWeather was skipped"
        );
        let (snapshot, attempts) = outcome.unwrap();
        assert_eq!(snapshot.provider, OM);
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Quota(QuotaError::NoLedger))
        );
    }

    #[test]
    fn metered_requests_need_a_limit() {
        let owm = metered_owm(None);
        let (outcome, urls, _) = drive_metered(&[&owm], &[], Some(UsageLedger::new()), ANSWER_ALL);
        assert_eq!(urls, Vec::<String>::new());
        let Err(FetchError::AllProvidersFailed(attempts)) = outcome else {
            panic!()
        };
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Quota(QuotaError::NoLimit))
        );
    }

    #[test]
    fn an_exhausted_budget_falls_back_without_sending() {
        let owm = metered_owm(Some(3));
        let om = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let mut ledger = Some(UsageLedger::new());
        let mut sent = 0;
        for _ in 0..5 {
            let (outcome, urls, after) = drive_metered(&[&owm, &om], &[], ledger, ANSWER_ALL);
            assert!(outcome.is_ok());
            sent += urls.iter().filter(|u| u.ends_with("/Current")).count();
            ledger = after;
        }
        assert_eq!(
            sent, 3,
            "no metered request beyond the limit was handed out"
        );
        assert_eq!(ledger.unwrap().used(OWM, ctx().now), 3);
    }

    #[test]
    fn requests_are_charged_their_cost_against_a_month_budget() {
        // Three requests at 4 units each against 10 a month: two fit.
        let weighted = Fake {
            metered: vec![0, 1, 2],
            limit: Some(10),
            month: true,
            cost: 4,
            ..Fake::new(
                OWM,
                &[
                    (Endpoint::Current, true),
                    (Endpoint::AirQuality, false),
                    (Endpoint::Hourly, false),
                ],
            )
        };
        let (outcome, urls, ledger) =
            drive_metered(&[&weighted], &[], Some(UsageLedger::new()), ANSWER_ALL);
        assert_eq!(urls.len(), 2);
        let (_, attempts) = outcome.unwrap();
        assert_eq!(
            attempts[2].error,
            Some(ProviderError::Quota(QuotaError::Exhausted {
                limit: 10,
                period: Period::Month
            }))
        );
        assert_eq!(ledger.unwrap().used(OWM, ctx().now), 8);
    }

    #[test]
    fn optional_metered_requests_are_skipped_when_over_budget() {
        let owm = Fake {
            metered: vec![0, 1],
            limit: Some(1),
            ..Fake::new(
                OWM,
                &[(Endpoint::Current, true), (Endpoint::AirQuality, false)],
            )
        };
        let (outcome, urls, _) = drive_metered(&[&owm], &[], Some(UsageLedger::new()), ANSWER_ALL);
        assert_eq!(urls, vec!["https://openweather/Current"]);
        let (snapshot, attempts) = outcome.unwrap();
        assert!(snapshot.air_quality.is_none());
        assert_eq!(
            attempts[1].error,
            Some(ProviderError::Quota(QuotaError::Exhausted {
                limit: 1,
                period: Period::Day
            }))
        );
    }

    /// A fake whose `Point` responses call for follow-ups: `(endpoint, required,
    /// metered)`, requested at `https://<id>/follow<i>`.
    struct Chained {
        inner: Fake,
        follow: Vec<(Endpoint, bool, bool)>,
    }

    impl Provider for Chained {
        fn id(&self) -> ProviderId {
            self.inner.id()
        }
        fn attribution(&self) -> Attribution {
            self.inner.attribution()
        }
        fn capabilities(&self) -> Capabilities {
            self.inner.capabilities()
        }
        fn budget(&self) -> Option<Budget> {
            self.inner.budget()
        }
        fn plan(
            &self,
            loc: &Coordinates,
            req: &DataRequirements,
            ctx: &FetchContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            self.inner.plan(loc, req, ctx)
        }
        fn follow_up(
            &self,
            endpoint: Endpoint,
            resp: &HttpResponse<'_>,
            _: &FetchContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            if endpoint != Endpoint::Point {
                return Ok(vec![]);
            }
            if resp.body == b"0" {
                return Err(ProviderError::Malformed("no grid".into()));
            }
            Ok(self
                .follow
                .iter()
                .enumerate()
                .map(|(i, &(endpoint, required, metered))| HttpRequest {
                    url: alloc::format!("https://{}/follow{i}", self.id().id()),
                    headers: vec![],
                    endpoint,
                    required,
                    metered,
                    method: Method::Get,
                    body: None,
                })
                .collect())
        }
        fn parse(
            &self,
            endpoint: Endpoint,
            resp: &HttpResponse<'_>,
            ctx: &FetchContext,
            out: &mut SnapshotBuilder,
        ) -> Result<(), ProviderError> {
            self.inner.parse(endpoint, resp, ctx, out)
        }
    }

    /// NWS-like: a point lookup (required unless `point_required` is false), then
    /// the current conditions; the lookup calls for `follow`.
    fn chained(point_required: bool, follow: &[(Endpoint, bool, bool)]) -> Chained {
        Chained {
            inner: Fake::new(
                ProviderId::Nws,
                &[(Endpoint::Point, point_required), (Endpoint::Current, true)],
            ),
            follow: follow.to_vec(),
        }
    }

    #[test]
    fn follow_ups_run_right_after_their_parent() {
        let nws = chained(
            true,
            &[
                (Endpoint::Stations, false, false),
                (Endpoint::Observation, false, false),
            ],
        );
        let (outcome, urls) = drive(&[&nws], &[], ANSWER_ALL);
        assert_eq!(
            urls,
            [
                "https://nws/Point",
                "https://nws/follow0",
                "https://nws/follow1",
                "https://nws/Current"
            ]
        );
        let (_, attempts) = outcome.unwrap();
        let endpoints: Vec<_> = attempts.iter().map(|a| a.endpoint).collect();
        assert_eq!(
            endpoints,
            [
                Endpoint::Point,
                Endpoint::Stations,
                Endpoint::Observation,
                Endpoint::Current
            ]
        );
    }

    #[test]
    fn a_failed_follow_up_of_a_required_request_falls_back() {
        let nws = chained(true, &[(Endpoint::Stations, false, false)]);
        let om = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (outcome, urls) = drive(&[&nws, &om], &[], |url| {
            if url.ends_with("follow0") {
                Ok((500, ""))
            } else {
                Ok((200, "20"))
            }
        });
        assert_eq!(
            urls,
            [
                "https://nws/Point",
                "https://nws/follow0",
                "https://open-meteo/Forecast"
            ],
            "required because its parent is"
        );
        assert_eq!(outcome.unwrap().0.provider, OM);
    }

    #[test]
    fn a_failed_optional_follow_up_is_skipped() {
        let nws = chained(false, &[(Endpoint::Stations, false, false)]);
        let (outcome, urls) = drive(&[&nws], &[], |url| {
            if url.ends_with("follow0") {
                Ok((500, ""))
            } else {
                Ok((200, "20"))
            }
        });
        assert_eq!(urls.last().map(String::as_str), Some("https://nws/Current"));
        let (snapshot, attempts) = outcome.unwrap();
        assert_eq!(snapshot.provider, ProviderId::Nws);
        assert!(matches!(
            attempts[1].error,
            Some(ProviderError::Http { status: 500, .. })
        ));
    }

    #[test]
    fn a_response_that_cannot_name_its_follow_ups_fails() {
        let nws = chained(true, &[(Endpoint::Stations, false, false)]);
        let om = Fake::new(OM, &[(Endpoint::Forecast, true)]);
        let (outcome, urls) = drive(&[&nws, &om], &[], |url| {
            Ok((200, if url.ends_with("Point") { "0" } else { "20" }))
        });
        assert_eq!(urls, ["https://nws/Point", "https://open-meteo/Forecast"]);
        let (_, attempts) = outcome.unwrap();
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Malformed("no grid".into())),
            "recorded once, as the parent's failure"
        );
    }

    #[test]
    fn the_fifth_follow_up_is_refused() {
        let five = [(Endpoint::Stations, false, false); 5];
        let nws = chained(false, &five);
        let (outcome, urls) = drive(&[&nws], &[], ANSWER_ALL);
        assert_eq!(urls.len(), 1 + MAX_FOLLOW_UPS + 1, "{urls:?}");
        assert!(!urls.iter().any(|u| u.ends_with("follow4")));
        let (_, attempts) = outcome.unwrap();
        assert!(
            attempts
                .iter()
                .any(|a| a.error == Some(ProviderError::Malformed("too many follow-ups".into())))
        );
    }

    #[test]
    fn metered_follow_ups_over_budget_are_not_sent() {
        let nws = Chained {
            inner: Fake {
                metered: vec![0],
                limit: Some(1),
                ..Fake::new(
                    ProviderId::Nws,
                    &[(Endpoint::Point, false), (Endpoint::Current, true)],
                )
            },
            follow: vec![(Endpoint::Stations, false, true)],
        };
        let (outcome, urls, ledger) =
            drive_metered(&[&nws], &[], Some(UsageLedger::new()), ANSWER_ALL);
        assert_eq!(urls, ["https://nws/Point", "https://nws/Current"]);
        let (_, attempts) = outcome.unwrap();
        assert_eq!(
            attempts[1].error,
            Some(ProviderError::Quota(QuotaError::Exhausted {
                limit: 1,
                period: Period::Day
            }))
        );
        assert_eq!(ledger.unwrap().used(ProviderId::Nws, UnixTime(1_000)), 1);
    }

    #[test]
    fn failed_metered_requests_still_count() {
        let owm = metered_owm(Some(5));
        let (_, _, ledger) =
            drive_metered(&[&owm], &[], Some(UsageLedger::new()), |_| Err("timeout"));
        assert_eq!(ledger.unwrap().used(OWM, ctx().now), 1);
    }

    /// A provider whose current and daily responses name alerts by id. An alert's
    /// body is its title, or "bad".
    struct Alerting {
        current_ids: Vec<&'static str>,
        daily_ids: Vec<&'static str>,
        metered: bool,
    }

    impl Provider for Alerting {
        fn id(&self) -> ProviderId {
            OWM
        }
        fn attribution(&self) -> Attribution {
            Attribution {
                text: "fake",
                url: "https://example.com",
                license: "none",
            }
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::CURRENT | Capabilities::ALERTS
        }
        fn budget(&self) -> Option<Budget> {
            self.metered.then_some(Budget::daily(3))
        }
        fn plan(
            &self,
            _: &Coordinates,
            _: &DataRequirements,
            _: &FetchContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            let call = |endpoint, required| HttpRequest {
                url: alloc::format!("https://owm/{endpoint:?}"),
                headers: vec![],
                endpoint,
                required,
                metered: false,
                method: Method::Get,
                body: None,
            };
            Ok(vec![
                call(Endpoint::Current, true),
                call(Endpoint::Daily, false),
            ])
        }
        fn alert_request(&self, id: &str, _: &FetchContext) -> Option<HttpRequest> {
            Some(HttpRequest {
                url: alloc::format!("https://owm/alert/{id}"),
                headers: vec![],
                endpoint: Endpoint::Alert,
                required: true,
                metered: self.metered,
                method: Method::Get,
                body: None,
            })
        }
        fn parse(
            &self,
            endpoint: Endpoint,
            resp: &HttpResponse<'_>,
            _: &FetchContext,
            out: &mut SnapshotBuilder,
        ) -> Result<(), ProviderError> {
            let text = core::str::from_utf8(resp.body).unwrap_or("bad");
            match endpoint {
                Endpoint::Current => {
                    out.set_location(ResolvedLocation {
                        name: "x".into(),
                        ..Default::default()
                    });
                    self.current_ids.iter().for_each(|id| out.refer_alert(id));
                }
                Endpoint::Daily => self.daily_ids.iter().for_each(|id| out.refer_alert(id)),
                Endpoint::Alert if text == "bad" => {
                    return Err(ProviderError::Malformed(text.into()));
                }
                Endpoint::Alert => {
                    let (id, title) = text.split_once(':').unwrap_or((text, text));
                    out.push_alert(alert(id, title, 5_000));
                }
                _ => {}
            }
            Ok(())
        }
    }

    fn alert(id: &str, title: &str, end: i64) -> Alert {
        Alert {
            id: Some(id.into()),
            title: title.into(),
            description: String::new(),
            severity: whether_core::scales::Level::Poor,
            start: UnixTime(0),
            end: UnixTime(end),
            source: "test".into(),
            checked_at: Some(UnixTime(1_000)),
            url: None,
        }
    }

    fn alerting(current_ids: &[&'static str], daily_ids: &[&'static str]) -> Alerting {
        Alerting {
            current_ids: current_ids.to_vec(),
            daily_ids: daily_ids.to_vec(),
            metered: false,
        }
    }

    /// Answers an alert request with "<id>:Alert <id>".
    #[allow(
        clippy::unnecessary_wraps,
        reason = "it is passed where a fallible reply is expected"
    )]
    fn alert_reply(url: &str) -> Reply {
        match url.rsplit_once("/alert/") {
            Some((_, id)) => Ok((
                200,
                alloc::boxed::Box::leak(alloc::format!("{id}:Alert {id}").into_boxed_str()),
            )),
            None => Ok((200, "")),
        }
    }

    /// Drives one provider with alerts required and `known` alerts attached.
    fn drive_alerts(
        p: &dyn Provider,
        known: Vec<Alert>,
        req: DataRequirements,
        ledger: Option<UsageLedger>,
        reply: impl Fn(&str) -> Reply,
    ) -> (Outcome, Vec<String>) {
        let loc = Coordinates {
            latitude: 40.7,
            longitude: -74.0,
        };
        let chain = [p];
        let mut s = FetchSession::new(&chain, &[], loc, req, ctx()).with_known_alerts(known);
        if let Some(ledger) = ledger {
            s = s.with_usage(ledger);
        }
        let mut urls = Vec::new();
        while let Some(r) = s.next_request() {
            let url = r.url.clone();
            urls.push(url.clone());
            match reply(&url) {
                Ok((status, body)) => s.on_response(&HttpResponse::new(status, body.as_bytes())),
                Err(msg) => s.on_transport_error(msg),
            }
        }
        (s.finish(UnixTime(2_000)), urls)
    }

    fn ids(snapshot: &WeatherSnapshot) -> Vec<&str> {
        snapshot
            .alerts
            .iter()
            .filter_map(|a| a.id.as_deref())
            .collect()
    }

    #[test]
    fn alert_details_follow_the_response_that_names_them() {
        let p = alerting(&["a", "b"], &["b", "c"]);
        let (outcome, urls) = drive_alerts(&p, vec![], DataRequirements::full(), None, alert_reply);
        assert_eq!(
            urls,
            [
                "https://owm/Current",
                "https://owm/alert/a",
                "https://owm/alert/b",
                "https://owm/Daily",
                "https://owm/alert/c",
            ]
        );
        let (snapshot, _) = outcome.unwrap();
        assert_eq!(ids(&snapshot), ["a", "b", "c"]);
        assert_eq!(snapshot.alerts[0].title, "Alert a");
    }

    #[test]
    fn known_alerts_are_reused_until_they_end() {
        let p = alerting(&["a", "b"], &[]);
        let known = vec![alert("a", "cached", 5_000), alert("b", "ended", 1_000)];
        let (outcome, urls) = drive_alerts(&p, known, DataRequirements::full(), None, alert_reply);
        assert_eq!(
            urls,
            [
                "https://owm/Current",
                "https://owm/alert/b",
                "https://owm/Daily"
            ]
        );
        let (snapshot, _) = outcome.unwrap();
        let titles: Vec<&str> = snapshot.alerts.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["cached", "Alert b"]);
    }

    #[test]
    fn known_alerts_are_rechecked_after_an_hour() {
        let p = alerting(&["new", "stale", "legacy", "fresh"], &[]);
        let checked = |id: &str, at: Option<i64>| Alert {
            checked_at: at.map(UnixTime),
            ..alert(id, "cached", 9_000)
        };
        // The session's clock is 1 000.
        let known = vec![
            checked("stale", Some(1_000 - ALERT_RECHECK_S)),
            checked("legacy", None),
            checked("fresh", Some(1_000 - ALERT_RECHECK_S + 1)),
        ];
        let reply = |url: &str| {
            if url.ends_with("/legacy") {
                Ok((500, ""))
            } else {
                alert_reply(url)
            }
        };
        let (outcome, urls) = drive_alerts(&p, known, DataRequirements::full(), None, reply);
        assert_eq!(
            urls,
            [
                "https://owm/Current",
                "https://owm/alert/new",
                "https://owm/alert/stale",
                "https://owm/alert/legacy",
                "https://owm/Daily",
            ],
            "new alerts first, then the stale ones"
        );
        let (snapshot, _) = outcome.unwrap();
        let shown: Vec<(&str, &str)> = snapshot
            .alerts
            .iter()
            .map(|a| (a.id.as_deref().unwrap(), a.title.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("stale", "Alert stale"),
                ("legacy", "cached"),
                ("fresh", "cached"),
                ("new", "Alert new"),
            ],
            "fresh details replace the cached copy; a failed check keeps it"
        );
    }

    #[test]
    fn alerts_are_followed_only_when_required() {
        let p = alerting(&["a"], &[]);
        let req = DataRequirements {
            alerts: false,
            ..DataRequirements::full()
        };
        let known = vec![alert("a", "cached", 5_000)];
        let (outcome, urls) = drive_alerts(&p, known, req, None, alert_reply);
        assert_eq!(urls, ["https://owm/Current", "https://owm/Daily"]);
        assert_eq!(outcome.unwrap().0.alerts, vec![]);
    }

    #[test]
    fn alert_details_per_fetch_are_capped() {
        let p = alerting(&["1", "2", "3", "4", "5", "6", "7"], &[]);
        let (outcome, urls) = drive_alerts(&p, vec![], DataRequirements::full(), None, alert_reply);
        let details = urls.iter().filter(|u| u.contains("/alert/")).count();
        assert_eq!(details, MAX_ALERT_DETAILS);
        assert_eq!(outcome.unwrap().0.alerts.len(), MAX_ALERT_DETAILS);
    }

    #[test]
    fn a_failed_alert_detail_keeps_the_provider() {
        let p = alerting(&["a", "b"], &[]);
        let reply = |url: &str| {
            if url.ends_with("/a") {
                Ok((500, ""))
            } else if url.ends_with("/b") {
                Ok((200, "bad"))
            } else {
                Ok((200, ""))
            }
        };
        let (outcome, urls) = drive_alerts(&p, vec![], DataRequirements::full(), None, reply);
        assert_eq!(urls.len(), 4, "the daily request still runs");
        let (snapshot, attempts) = outcome.unwrap();
        assert_eq!(snapshot.provider, OWM);
        assert_eq!(snapshot.alerts, vec![]);
        let failed: Vec<Endpoint> = attempts
            .iter()
            .filter(|a| a.error.is_some())
            .map(|a| a.endpoint)
            .collect();
        assert_eq!(failed, [Endpoint::Alert, Endpoint::Alert]);
    }

    #[test]
    fn metered_alert_details_respect_the_budget() {
        let p = Alerting {
            metered: true,
            ..alerting(&["a", "b", "c", "d"], &[])
        };
        let (outcome, urls) = drive_alerts(
            &p,
            vec![],
            DataRequirements::full(),
            Some(UsageLedger::new()),
            alert_reply,
        );
        let details = urls.iter().filter(|u| u.contains("/alert/")).count();
        assert_eq!(details, 3, "the limit is 3 calls");
        let (_, attempts) = outcome.unwrap();
        assert_eq!(
            attempts
                .iter()
                .filter(|a| matches!(a.error, Some(ProviderError::Quota(_))))
                .count(),
            1
        );
    }
}
