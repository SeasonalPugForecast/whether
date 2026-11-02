//! HTTPS GET for the providers' planned requests: DNS, TCP, TLS 1.3 with one
//! pinned root per host (embedded-tls), and HTTP/1.1 framing (reqwless), into
//! static buffers.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io_async::Write as _;
use embedded_tls::pki::CertVerifier;
use embedded_tls::{
    Aes128GcmSha256, Certificate, CryptoProvider, CryptoRngCore, TlsConfig, TlsConnection,
    TlsContext, TlsError, TlsVerifier,
};
use rand_chacha::ChaCha8Rng;
use reqwless::request::{Method, Request, RequestBuilder as _};
use reqwless::response::Response;
use static_cell::ConstStaticCell;
use whether_core::time::{UnixTime, parse_http_date};
use whether_providers::{HttpRequest, HttpResponse, Method as RequestMethod};

use crate::net::clock::{self, TlsTime};

/// One TLS record (16 KiB) plus the record overhead: servers do not honor a
/// smaller maximum fragment length.
const TLS_RX: usize = 16_640;
const TLS_TX: usize = 4096;
const TCP_RX: usize = 4096;
const TCP_TX: usize = 2048;
/// Headers plus body: `OpenWeather`'s free 5-day forecast is 16.2 KB, the One Call
/// 15-minute timeline 13.9 KB, an Open-Meteo device request about 2 KB.
const HTTP: usize = 20_480;
/// Room for the certificate chain a server sends (`OpenWeather`'s four
/// certificates are about 6.8 KB).
const CHAIN: usize = 8192;
/// TCP inactivity timeout.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The roots in `firmware/certs/` (fingerprints in its README).
const ROOTS: [(&str, &[u8]); 3] = [
    (
        "isrg-root-x1.der",
        include_bytes!("../../../certs/isrg-root-x1.der"),
    ),
    (
        "usertrust-rsa.der",
        include_bytes!("../../../certs/usertrust-rsa.der"),
    ),
    (
        "harica-rootca-2015.der",
        include_bytes!("../../../certs/harica-rootca-2015.der"),
    ),
];

type Verifier = CertVerifier<'static, Aes128GcmSha256, TlsTime, CHAIN>;

/// The transfer buffers, zero-initialized in `.bss` so they never pass through
/// the stack or the heap.
struct Buffers {
    tls_rx: [u8; TLS_RX],
    tls_tx: [u8; TLS_TX],
    tcp_rx: [u8; TCP_RX],
    tcp_tx: [u8; TCP_TX],
    http: [u8; HTTP],
}

static BUFFERS: ConstStaticCell<Buffers> = ConstStaticCell::new(Buffers {
    tls_rx: [0; TLS_RX],
    tls_tx: [0; TLS_TX],
    tcp_rx: [0; TCP_RX],
    tcp_tx: [0; TCP_TX],
    http: [0; HTTP],
});

/// Why a request got no response.
#[derive(Debug)]
pub enum NetError {
    /// The URL is not `https://host[:port]/path`.
    Url,
    /// No root certificate is configured for the host.
    NoRoot,
    /// The host name did not resolve.
    Dns,
    /// The TCP connection failed.
    Connect,
    /// The TLS handshake or record layer failed (`InvalidCertificate` when the
    /// chain does not lead to the pinned root).
    Tls(TlsError),
    /// The HTTP exchange failed (including a body larger than the buffer).
    Http(reqwless::Error),
    /// The request is a POST, which the device does not send yet (ACIS history,
    /// is host-only).
    Post,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url => f.write_str("unsupported URL"),
            Self::NoRoot => f.write_str("no root certificate for this host"),
            Self::Dns => f.write_str("DNS lookup failed"),
            Self::Connect => f.write_str("TCP connection failed"),
            Self::Tls(e) => write!(f, "TLS: {e:?}"),
            Self::Http(e) => write!(f, "HTTP: {e:?}"),
            Self::Post => f.write_str("POST requests are not supported on the device"),
        }
    }
}

/// A response whose body lives in the transport's buffer until the next request.
#[derive(Debug)]
pub struct Reply<'a> {
    /// Status code.
    pub status: u16,
    /// The body.
    pub body: &'a [u8],
    /// The `Date` header.
    pub date: Option<UnixTime>,
    /// The `Expires` header.
    pub expires: Option<UnixTime>,
    /// The `Last-Modified` header.
    pub last_modified: Option<String>,
}

impl Reply<'_> {
    /// The response as the providers' parsers see it.
    pub fn as_response(&self) -> HttpResponse<'_> {
        HttpResponse {
            status: self.status,
            body: self.body,
            date: self.date,
            expires: self.expires,
            last_modified: self.last_modified.as_deref(),
        }
    }
}

/// The parts of an `https://` URL.
struct Target<'u> {
    host: &'u str,
    port: u16,
    path: &'u str,
}

fn split_url(url: &str) -> Option<Target<'_>> {
    let rest = url.strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (authority, 443),
    };
    (!host.is_empty()).then_some(Target { host, port, path })
}

/// The crypto embedded-tls uses: a `ChaCha` stream seeded from the hardware RNG,
/// and the verifier for the current host.
struct Provider<'p> {
    rng: &'p mut ChaCha8Rng,
    verifier: &'p mut Verifier,
}

impl CryptoProvider for Provider<'_> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = &'static [u8];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut *self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut *self.verifier)
    }
}

/// The TLS stream, as reqwless reads and writes it.
struct Conn<'a>(TlsConnection<'a, TcpSocket<'a>, Aes128GcmSha256>);

impl embedded_io_async::ErrorType for Conn<'_> {
    type Error = TlsError;
}

impl embedded_io_async::Read for Conn<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, TlsError> {
        self.0.read(buf).await
    }
}

impl embedded_io_async::Write for Conn<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, TlsError> {
        self.0.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), TlsError> {
        self.0.flush().await
    }
}

impl reqwless::TryBufRead for Conn<'_> {}

/// HTTPS over the embassy-net stack, one request at a time.
pub struct EspTransport {
    stack: Stack<'static>,
    roots: Vec<(String, String)>,
    user_agent: String,
    rng: ChaCha8Rng,
    buffers: &'static mut Buffers,
    verifier: Verifier,
}

impl fmt::Debug for EspTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EspTransport")
            .field("roots", &self.roots)
            .finish_non_exhaustive()
    }
}

impl EspTransport {
    /// A transport with `roots` overrides (`[tls] roots`), the `User-Agent` the
    /// providers expect, and a hardware-RNG seed. Call it once: it takes the
    /// static buffers.
    ///
    /// # Panics
    /// When called twice.
    pub fn new(
        stack: Stack<'static>,
        roots: Vec<(String, String)>,
        user_agent: String,
        seed: [u8; 32],
    ) -> Self {
        use rand_core::SeedableRng as _;
        Self {
            stack,
            roots,
            user_agent,
            rng: ChaCha8Rng::from_seed(seed),
            buffers: BUFFERS.take(),
            verifier: CertVerifier::new(Certificate::X509(ROOTS[0].1)),
        }
    }

    /// The pinned root for `host`.
    fn root(&self, host: &str) -> Option<&'static [u8]> {
        let file = fw_config::root_for(host, &self.roots)?;
        ROOTS
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, der)| *der)
    }

    /// Performs `req`, verifying the host against its pinned root. A response
    /// with a `Date` header sets the wall clock.
    pub async fn get(&mut self, req: &HttpRequest) -> Result<Reply<'_>, NetError> {
        if req.method != RequestMethod::Get {
            return Err(NetError::Post);
        }
        let target = split_url(&req.url).ok_or(NetError::Url)?;
        let root = self.root(target.host).ok_or(NetError::NoRoot)?;
        self.get_with_root(&target, root, &req.headers, Method::GET)
            .await
    }

    /// Sends `HEAD https://host/`, verified against the host's pinned root, and
    /// returns the status, as a check that the root still matches the chain the
    /// host sends.
    pub async fn check_host(&mut self, host: &str) -> Result<u16, NetError> {
        let root = self.root(host).ok_or(NetError::NoRoot)?;
        let target = Target {
            host,
            port: 443,
            path: "/",
        };
        self.get_with_root(&target, root, &[], Method::HEAD)
            .await
            .map(|reply| reply.status)
    }

    /// Opens a TLS session to `host` with the wrong root and reports whether the
    /// handshake was refused, as a self-check of certificate verification.
    pub async fn wrong_root_is_refused(&mut self, host: &str) -> Result<bool, NetError> {
        let right = self.root(host).ok_or(NetError::NoRoot)?;
        let (_, wrong) = ROOTS
            .iter()
            .find(|(_, der)| !core::ptr::eq(*der, right))
            .ok_or(NetError::NoRoot)?;
        let target = Target {
            host,
            port: 443,
            path: "/",
        };
        match self.get_with_root(&target, wrong, &[], Method::HEAD).await {
            Err(NetError::Tls(TlsError::InvalidCertificate)) => Ok(true),
            Err(e) => Err(e),
            Ok(_) => Ok(false),
        }
    }

    async fn get_with_root(
        &mut self,
        target: &Target<'_>,
        root: &'static [u8],
        headers: &[(&'static str, String)],
        method: Method,
    ) -> Result<Reply<'_>, NetError> {
        let addresses = self
            .stack
            .dns_query(target.host, DnsQueryType::A)
            .await
            .map_err(|_| NetError::Dns)?;
        let address = *addresses.first().ok_or(NetError::Dns)?;
        let b = &mut *self.buffers;
        let mut socket = TcpSocket::new(self.stack, &mut b.tcp_rx, &mut b.tcp_tx);
        socket.set_timeout(Some(TIMEOUT));
        socket
            .connect((address, target.port))
            .await
            .map_err(|_| NetError::Connect)?;

        let mut tls = TlsConnection::new(socket, &mut b.tls_rx, &mut b.tls_tx);
        let config = TlsConfig::new().with_server_name(target.host);
        self.verifier = CertVerifier::new(Certificate::X509(root));
        let provider = Provider {
            rng: &mut self.rng,
            verifier: &mut self.verifier,
        };
        tls.open(TlsContext::new(&config, provider))
            .await
            .map_err(NetError::Tls)?;
        let mut conn = Conn(tls);

        let mut all: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        all.push(("User-Agent", self.user_agent.as_str()));
        all.push(("Connection", "close"));
        Request::new(method, target.path)
            .host(target.host)
            .headers(&all)
            .build()
            .write_header(&mut conn)
            .await
            .map_err(NetError::Http)?;
        conn.flush().await.map_err(NetError::Tls)?;

        let response = Response::read(&mut conn, method, &mut b.http)
            .await
            .map_err(NetError::Http)?;
        let status = response.status.0;
        let (mut date, mut expires, mut last_modified) = (None, None, None);
        for (name, value) in response.headers() {
            let Ok(value) = core::str::from_utf8(value) else {
                continue;
            };
            if name.eq_ignore_ascii_case("date") {
                date = parse_http_date(value);
            } else if name.eq_ignore_ascii_case("expires") {
                expires = parse_http_date(value);
            } else if name.eq_ignore_ascii_case("last-modified") {
                last_modified = Some(String::from(value));
            }
        }
        let body = response
            .body()
            .read_to_end()
            .await
            .map_err(NetError::Http)?;
        if let Some(date) = date {
            clock::set(date);
        }
        Ok(Reply {
            status,
            body,
            date,
            expires,
            last_modified,
        })
    }
}
