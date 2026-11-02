//! Networking: the Wi-Fi station, the embassy-net stack, the wall clock and
//! HTTPS (TLS 1.3 through embedded-tls, HTTP/1.1 through reqwless).

pub mod clock;
pub mod http;
pub mod wifi;
