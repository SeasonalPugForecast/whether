//! Sans-IO history providers and the session that runs them: past
//! days, normals and records, fetched apart from the forecast. Like
//! [`crate::FetchSession`]: a fallback chain, then supplements that fill gaps,
//! with follow-up requests; no alerts, no call budget.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use whether_core::history::{ClimateQuery, History, HistoryBuilder, Normals};
use whether_core::model::{ProviderId, ResolvedLocation};
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset, civil_from_days};

use crate::error::{ProviderError, check_status};
use crate::http::{Attribution, Endpoint, HttpRequest, HttpResponse};
use crate::session::{Attempt, FetchError, MAX_FOLLOW_UPS};
use crate::url::Coordinates;

/// What a history fetch knows about now. The caller reads the clock.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryContext {
    /// The current time.
    pub now: UnixTime,
    /// The place's local date today.
    pub today: LocalDate,
    /// The place's UTC offset now.
    pub utc_offset: UtcOffset,
    /// `whether/<version>`.
    pub user_agent: String,
    /// Normals the caller keeps for the place (computed once): they
    /// fill what the providers leave empty, and a provider need not compute
    /// them again.
    pub normals: Option<Normals>,
}

impl HistoryContext {
    /// The date `days` after today (negative: before).
    pub fn day(&self, days: i64) -> LocalDate {
        let (year, month, day) = civil_from_days(self.today.days_since_epoch() + days);
        LocalDate { year, month, day }
    }

    /// The first date `req` needs: the oldest past day, or the 1st of the month
    /// for month-to-date totals.
    pub fn first_date(&self, req: &HistoryRequirements) -> LocalDate {
        let past = self.day(-i64::from(req.past_days));
        if req.month_to_date {
            past.min(LocalDate {
                day: 1,
                ..self.today
            })
        } else {
            past
        }
    }

    /// The last date `req` needs: the end of the climate window, or yesterday.
    pub fn last_date(&self, req: &HistoryRequirements) -> LocalDate {
        if req.climate_days > 0 {
            self.day(i64::from(req.climate_days) - 1)
        } else {
            self.day(-1)
        }
    }
}

/// Minutes after local midnight before yesterday counts as done: stations report
/// late.
pub const DAY_START_MINUTES: i64 = 15;

/// Whether a place's history should be fetched at `now`: nothing is kept
/// (`kept`: the place's date it was fetched on, and what it was fetched for),
/// it misses something `req` needs, or it is from an earlier local day and
/// today has begun (15 minutes after midnight).
pub fn history_due(
    kept: Option<(LocalDate, HistoryRequirements)>,
    req: &HistoryRequirements,
    now: UnixTime,
    offset: UtcOffset,
) -> bool {
    let Some((fetched_on, covered)) = kept else {
        return true;
    };
    if !covered.covers(req) {
        return true;
    }
    let local = now.to_local(offset);
    let minutes = i64::from(local.hour) * 60 + i64::from(local.minute);
    local.date > fetched_on && minutes >= DAY_START_MINUTES
}

/// A history source: plans requests and parses responses into a
/// [`HistoryBuilder`]. Implementations hold only settings.
pub trait HistoryProvider: Send + Sync {
    /// Which provider this is.
    fn id(&self) -> ProviderId;
    /// The required credit.
    fn attribution(&self) -> Attribution;
    /// The requests for `loc` and `req`, in order.
    fn plan(
        &self,
        loc: &Coordinates,
        req: &HistoryRequirements,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError>;
    /// Requests a successful response calls for next (ACIS: the chosen
    /// station's data after the station list). None by default.
    fn follow_up(
        &self,
        _endpoint: Endpoint,
        _resp: &HttpResponse<'_>,
        _loc: &Coordinates,
        _req: &HistoryRequirements,
        _ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        Ok(Vec::new())
    }
    /// Parses one response into `out`.
    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &HistoryContext,
        out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError>;
    /// The requests answering a climate query (the vacation planner): the
    /// climate of `query`'s dates over its years. Unsupported by default.
    fn plan_climate(
        &self,
        _loc: &Coordinates,
        _query: &ClimateQuery,
        _ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        Err(ProviderError::Config(alloc::format!(
            "{} answers no climate queries",
            self.id().display_name()
        )))
    }
    /// Requests a climate query's response calls for next. None by default.
    fn follow_up_climate(
        &self,
        _endpoint: Endpoint,
        _resp: &HttpResponse<'_>,
        _loc: &Coordinates,
        _query: &ClimateQuery,
        _ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        Ok(Vec::new())
    }
    /// Parses one response to a climate query into `out`: the query's dates in
    /// `climate`, and its years as the period.
    fn parse_climate(
        &self,
        _endpoint: Endpoint,
        _resp: &HttpResponse<'_>,
        _query: &ClimateQuery,
        _ctx: &HistoryContext,
        _out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Config("no climate queries".into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Primary(usize),
    Supplement(usize),
    Done,
}

/// Drives the requests of one location's history fetch, or of a climate query.
pub struct HistorySession<'p> {
    chain: &'p [&'p dyn HistoryProvider],
    supplements: &'p [&'p dyn HistoryProvider],
    loc: Coordinates,
    req: HistoryRequirements,
    query: Option<ClimateQuery>,
    ctx: HistoryContext,
    phase: Phase,
    planned: bool,
    pending: VecDeque<HttpRequest>,
    in_flight: Option<HttpRequest>,
    builder: HistoryBuilder,
    primary: Option<History>,
    primary_id: Option<ProviderId>,
    attempts: Vec<Attempt>,
    follow_ups: usize,
}

impl core::fmt::Debug for HistorySession<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HistorySession")
            .field("phase", &self.phase)
            .field("pending", &self.pending.len())
            .field("attempts", &self.attempts)
            .finish_non_exhaustive()
    }
}

impl<'p> HistorySession<'p> {
    /// A session over `chain` (in fallback order) and `supplements`.
    pub fn new(
        chain: &'p [&'p dyn HistoryProvider],
        supplements: &'p [&'p dyn HistoryProvider],
        loc: Coordinates,
        req: HistoryRequirements,
        ctx: HistoryContext,
    ) -> Self {
        let first = chain.first().map_or(ProviderId::Acis, |p| p.id());
        Self {
            chain,
            supplements,
            loc,
            req,
            query: None,
            ctx,
            phase: if chain.is_empty() || req.is_empty() {
                Phase::Done
            } else {
                Phase::Primary(0)
            },
            planned: false,
            pending: VecDeque::new(),
            in_flight: None,
            builder: HistoryBuilder::new(first),
            primary: None,
            primary_id: None,
            attempts: Vec::new(),
            follow_ups: 0,
        }
    }

    /// A session answering `query` over `chain`, in fallback order. Sources
    /// are not mixed: no supplements.
    pub fn climate(
        chain: &'p [&'p dyn HistoryProvider],
        loc: Coordinates,
        query: ClimateQuery,
        ctx: HistoryContext,
    ) -> Self {
        let mut session = Self::new(chain, &[], loc, HistoryRequirements::default(), ctx);
        session.query = Some(query);
        session.phase = if chain.is_empty() {
            Phase::Done
        } else {
            Phase::Primary(0)
        };
        session
    }

    fn provider(&self) -> Option<&'p dyn HistoryProvider> {
        match self.phase {
            Phase::Primary(i) => self.chain.get(i).copied(),
            Phase::Supplement(i) => self.supplements.get(i).copied(),
            Phase::Done => None,
        }
    }

    /// The next request to perform, or `None` when the session is over.
    pub fn next_request(&mut self) -> Option<&HttpRequest> {
        loop {
            let provider = self.provider()?;
            if !self.planned {
                self.planned = true;
                self.follow_ups = 0;
                self.builder = HistoryBuilder::new(provider.id());
                // The requested place; providers may refine it.
                self.builder.set_location(ResolvedLocation {
                    latitude: self.loc.latitude,
                    longitude: self.loc.longitude,
                    utc_offset: self.ctx.utc_offset,
                    ..ResolvedLocation::default()
                });
                let planned = match &self.query {
                    Some(q) => provider.plan_climate(&self.loc, q, &self.ctx),
                    None => provider.plan(&self.loc, &self.req, &self.ctx),
                };
                match planned {
                    Ok(requests) => self.pending = requests.into(),
                    Err(error) => {
                        self.record(provider.id(), Endpoint::Climate, Some(error));
                        self.advance();
                        continue;
                    }
                }
            }
            if let Some(next) = self.pending.pop_front() {
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
        let endpoint = request.endpoint;
        let result = check_status(resp).and_then(|()| match &self.query {
            Some(q) => provider
                .parse_climate(endpoint, resp, q, &self.ctx, &mut self.builder)
                .and_then(|()| provider.follow_up_climate(endpoint, resp, &self.loc, q, &self.ctx)),
            None => provider
                .parse(endpoint, resp, &self.ctx, &mut self.builder)
                .and_then(|()| provider.follow_up(endpoint, resp, &self.loc, &self.req, &self.ctx)),
        });
        match result {
            Ok(follow_ups) => {
                self.record(provider.id(), request.endpoint, None);
                let mut queued = Vec::new();
                for mut follow in follow_ups {
                    if self.follow_ups >= MAX_FOLLOW_UPS {
                        self.record(
                            provider.id(),
                            follow.endpoint,
                            Some(ProviderError::Malformed("too many follow-ups".into())),
                        );
                        continue;
                    }
                    self.follow_ups += 1;
                    follow.required |= request.required;
                    queued.push(follow);
                }
                for follow in queued.into_iter().rev() {
                    self.pending.push_front(follow);
                }
            }
            Err(error) => self.fail(provider.id(), &request, error),
        }
    }

    /// Reports that the request last returned by [`Self::next_request`] got no
    /// response.
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
        self.record(id, request.endpoint, Some(error));
        if request.required {
            self.advance();
        }
    }

    fn record(&mut self, provider: ProviderId, endpoint: Endpoint, error: Option<ProviderError>) {
        self.attempts.push(Attempt {
            provider,
            endpoint,
            error,
        });
    }

    fn complete_provider(&mut self) {
        let Some(provider) = self.provider() else {
            return;
        };
        let builder = core::mem::replace(&mut self.builder, HistoryBuilder::new(provider.id()));
        match (builder.finish(self.ctx.now), self.phase) {
            (Ok(history), Phase::Primary(_)) => {
                self.primary_id = Some(provider.id());
                self.primary = Some(history);
                self.phase = Phase::Supplement(0);
                self.planned = false;
                self.pending.clear();
                self.skip_redundant_supplements();
            }
            (Ok(history), Phase::Supplement(_)) => {
                if let Some(primary) = self.primary.as_mut() {
                    primary.fill_gaps_from(&history);
                }
                self.advance();
            }
            (Err(e), _) => {
                self.record(
                    provider.id(),
                    Endpoint::Climate,
                    Some(ProviderError::Malformed(e.to_string())),
                );
                self.advance();
            }
            (Ok(_), Phase::Done) => {}
        }
    }

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

    /// Skips supplements that are the primary's provider, and ends the session
    /// after the last supplement.
    fn skip_redundant_supplements(&mut self) {
        while let Phase::Supplement(i) = self.phase {
            match self.supplements.get(i) {
                None => self.phase = Phase::Done,
                Some(p) if Some(p.id()) == self.primary_id => self.phase = Phase::Supplement(i + 1),
                Some(_) => break,
            }
        }
    }

    /// The history (with every attempt) or why there is none.
    pub fn finish(self, fetched_at: UnixTime) -> Result<(History, Vec<Attempt>), FetchError> {
        match self.primary {
            Some(mut history) => {
                history.fetched_at = fetched_at;
                if let Some(q) = self.query {
                    history.period = history.period.or(Some(q.years));
                    return Ok((history, self.attempts));
                }
                // Normals computed now, else the caller's, fill what is missing.
                if let Some(normals) = history.normals.clone().or(self.ctx.normals.clone()) {
                    let (first, last) = (
                        self.ctx.first_date(&self.req),
                        self.ctx.last_date(&self.req),
                    );
                    let days = last.days_since_epoch() - first.days_since_epoch();
                    let dates = (0..=days).map(|i| {
                        let (year, month, day) = civil_from_days(first.days_since_epoch() + i);
                        LocalDate { year, month, day }
                    });
                    history.fill_normals(&normals, dates);
                }
                Ok((history, self.attempts))
            }
            None => Err(FetchError::AllProvidersFailed(self.attempts)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::vec;
    use whether_core::units::Celsius;

    use crate::http::Method;

    /// Each planned request's body is a high for yesterday, or "bad".
    struct Fake {
        id: ProviderId,
        plan: Vec<bool>,
        follow: usize,
    }

    impl HistoryProvider for Fake {
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
        fn plan(
            &self,
            _: &Coordinates,
            _: &HistoryRequirements,
            _: &HistoryContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            Ok(self
                .plan
                .iter()
                .enumerate()
                .map(|(i, &required)| request(&format!("https://{}/{i}", self.id.id()), required))
                .collect())
        }
        fn follow_up(
            &self,
            _: Endpoint,
            resp: &HttpResponse<'_>,
            _: &Coordinates,
            _: &HistoryRequirements,
            _: &HistoryContext,
        ) -> Result<Vec<HttpRequest>, ProviderError> {
            Ok(if resp.body == b"follow" {
                (0..self.follow)
                    .map(|i| request(&format!("https://{}/f{i}", self.id.id()), false))
                    .collect()
            } else {
                vec![]
            })
        }
        fn parse(
            &self,
            _: Endpoint,
            resp: &HttpResponse<'_>,
            ctx: &HistoryContext,
            out: &mut HistoryBuilder,
        ) -> Result<(), ProviderError> {
            let text = core::str::from_utf8(resp.body).unwrap_or("bad");
            if text == "follow" {
                return Ok(());
            }
            let high: f32 = text
                .parse()
                .map_err(|_| ProviderError::Malformed(text.into()))?;
            out.day_mut(ctx.day(-1)).temp_max = Some(Celsius(high));
            out.day_mut(ctx.day(-2)).temp_min = Some(Celsius(high - 10.0));
            Ok(())
        }
    }

    fn request(url: &str, required: bool) -> HttpRequest {
        HttpRequest {
            url: url.into(),
            headers: vec![],
            endpoint: Endpoint::Climate,
            required,
            metered: false,
            method: Method::Post,
            body: Some("{}".into()),
        }
    }

    fn ctx() -> HistoryContext {
        HistoryContext {
            now: UnixTime(1_792_245_600),
            today: LocalDate {
                year: 2026,
                month: 10,
                day: 17,
            },
            utc_offset: UtcOffset(-14_400),
            user_agent: "test".into(),
            normals: None,
        }
    }

    fn req() -> HistoryRequirements {
        HistoryRequirements {
            past_days: 7,
            climate_days: 7,
            records: true,
            month_to_date: true,
        }
    }

    type Outcome = Result<(History, Vec<Attempt>), FetchError>;

    fn drive(
        chain: &[&dyn HistoryProvider],
        supplements: &[&dyn HistoryProvider],
        reply: impl Fn(&str) -> (u16, &'static str),
    ) -> (Outcome, Vec<String>) {
        let loc = Coordinates {
            latitude: 40.7,
            longitude: -74.0,
        };
        let mut s = HistorySession::new(chain, supplements, loc, req(), ctx());
        let mut urls = Vec::new();
        while let Some(r) = s.next_request() {
            urls.push(r.url.clone());
            let (status, body) = reply(&r.url);
            s.on_response(&HttpResponse::new(status, body.as_bytes()));
        }
        (s.finish(UnixTime(2)), urls)
    }

    #[test]
    fn dates_cover_the_month_and_the_week_ahead() {
        let c = ctx();
        assert_eq!(c.first_date(&req()), LocalDate { day: 1, ..c.today });
        assert_eq!(c.last_date(&req()), LocalDate { day: 23, ..c.today });
        let short = HistoryRequirements {
            past_days: 3,
            climate_days: 0,
            records: false,
            month_to_date: false,
        };
        assert_eq!(c.first_date(&short), LocalDate { day: 14, ..c.today });
        assert_eq!(c.last_date(&short), LocalDate { day: 16, ..c.today });
    }

    #[test]
    fn falls_back_and_supplements() {
        let acis = Fake {
            id: ProviderId::Acis,
            plan: vec![true],
            follow: 0,
        };
        let om = Fake {
            id: ProviderId::OpenMeteo,
            plan: vec![true],
            follow: 0,
        };
        let (outcome, urls) = drive(&[&acis, &om], &[&om], |url| {
            if url.contains("acis") {
                (500, "")
            } else {
                (200, "21")
            }
        });
        assert_eq!(urls, ["https://acis/0", "https://open-meteo/0"]);
        let (history, attempts) = outcome.unwrap();
        assert_eq!(history.source, ProviderId::OpenMeteo);
        assert_eq!(history.days.len(), 2);
        assert_eq!(history.location.utc_offset, UtcOffset(-14_400));
        assert!(attempts[0].error.is_some());
        // ACIS answers; Open-Meteo fills nothing it lacks… but adds nothing new.
        let (outcome, urls) = drive(&[&acis], &[&om], |url| {
            (200, if url.contains("acis") { "20" } else { "25" })
        });
        assert_eq!(urls, ["https://acis/0", "https://open-meteo/0"]);
        let (history, _) = outcome.unwrap();
        assert_eq!(
            history.days[1].temp_max,
            Some(Celsius(20.0)),
            "the primary wins"
        );
        assert_eq!(history.supplemented_by, Vec::<ProviderId>::new());
    }

    #[test]
    fn follow_ups_run_after_their_parent_and_are_capped() {
        let acis = Fake {
            id: ProviderId::Acis,
            plan: vec![true, false],
            follow: 5,
        };
        let (outcome, urls) = drive(&[&acis], &[], |url| {
            (200, if url.ends_with("/0") { "follow" } else { "20" })
        });
        assert_eq!(
            urls,
            [
                "https://acis/0",
                "https://acis/f0",
                "https://acis/f1",
                "https://acis/f2",
                "https://acis/f3",
                "https://acis/1"
            ]
        );
        let (_, attempts) = outcome.unwrap();
        assert!(
            attempts
                .iter()
                .any(|a| a.error == Some(ProviderError::Malformed("too many follow-ups".into())))
        );
    }

    #[test]
    fn nothing_needed_sends_nothing() {
        let acis = Fake {
            id: ProviderId::Acis,
            plan: vec![true],
            follow: 0,
        };
        let loc = Coordinates {
            latitude: 40.7,
            longitude: -74.0,
        };
        let chain: [&dyn HistoryProvider; 1] = [&acis];
        let mut s = HistorySession::new(&chain, &[], loc, HistoryRequirements::default(), ctx());
        assert!(s.next_request().is_none());
        assert!(matches!(
            s.finish(UnixTime(0)),
            Err(FetchError::AllProvidersFailed(_))
        ));
    }
}
