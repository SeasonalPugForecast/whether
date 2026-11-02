//! The wall clock: set from HTTP `Date` headers and advanced by
//! [`embassy_time::Instant`].

use core::cell::Cell;

use critical_section::Mutex;
use embassy_time::Instant;
use whether_core::time::UnixTime;

/// The Unix time last learned and the uptime (seconds) when it was learned.
static BASE: Mutex<Cell<Option<(i64, u64)>>> = Mutex::new(Cell::new(None));

/// Sets the clock to `now`, e.g. from a response's `Date` header.
pub fn set(now: UnixTime) {
    let uptime = Instant::now().as_secs();
    critical_section::with(|cs| BASE.borrow(cs).set(Some((now.0, uptime))));
}

/// The current Unix time, once a response has set it.
pub fn now() -> Option<UnixTime> {
    let (unix, at) = critical_section::with(|cs| BASE.borrow(cs).get())?;
    let elapsed = Instant::now().as_secs().saturating_sub(at);
    Some(UnixTime(
        unix + i64::try_from(elapsed).unwrap_or(i64::MAX - unix),
    ))
}

/// The clock embedded-tls checks certificate validity against. Until a response
/// has set the clock (the first handshake after boot), the chain and the host name
/// are verified but the validity dates are not.
#[derive(Debug)]
pub struct TlsTime;

impl embedded_tls::TlsClock for TlsTime {
    fn now() -> Option<u64> {
        now().and_then(|t| u64::try_from(t.0).ok())
    }
}
