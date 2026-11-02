//! `UreqTransport` against a local canned-response server.

mod support;

use std::time::{Duration, Instant};

use support::{Server, chunked, response};
use whether_core::time::UnixTime;
use whether_providers::{Endpoint, HttpRequest, Method};
use whether_runtime::transport::{Transport, TransportError, UreqTransport};

fn get(url: String) -> HttpRequest {
    HttpRequest {
        url,
        headers: vec![],
        endpoint: Endpoint::Forecast,
        required: true,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

fn transport() -> UreqTransport {
    UreqTransport::with_timeouts(
        "whether-test",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
}

#[test]
fn ok_json_with_headers() {
    let server = Server::start(|_| {
        response(
            "200 OK",
            &[
                ("Content-Type", "application/json"),
                ("Date", "Mon, 05 Oct 2026 17:00:00 GMT"),
                ("Expires", "Mon, 05 Oct 2026 17:30:00 GMT"),
                ("Last-Modified", "Mon, 05 Oct 2026 16:55:00 GMT"),
            ],
            r#"{"ok":true}"#,
        )
    });
    let resp = transport()
        .send(&get(format!("{}/forecast?x=1", server.base)))
        .unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, br#"{"ok":true}"#);
    assert_eq!(resp.date, Some(UnixTime(1_791_219_600)));
    assert_eq!(resp.expires, Some(UnixTime(1_791_221_400)));
    assert_eq!(
        resp.last_modified.as_deref(),
        Some("Mon, 05 Oct 2026 16:55:00 GMT")
    );
}

#[test]
fn error_statuses_keep_their_bodies() {
    let server = Server::start(|_| {
        response(
            "400 Bad Request",
            &[],
            r#"{"reason":"bad latitude","error":true}"#,
        )
    });
    let resp = transport()
        .send(&get(format!("{}/forecast", server.base)))
        .unwrap();
    assert_eq!(resp.status, 400);
    assert!(
        String::from_utf8(resp.body)
            .unwrap()
            .contains("bad latitude")
    );
}

#[test]
fn chunked_bodies_are_joined() {
    let server = Server::start(|_| chunked(&[r#"{"a":"#, "1}"]));
    let resp = transport()
        .send(&get(format!("{}/x", server.base)))
        .unwrap();
    assert_eq!(resp.body, br#"{"a":1}"#);
}

#[test]
fn not_modified() {
    let server =
        Server::start(|_| b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".to_vec());
    let resp = transport()
        .send(&get(format!("{}/x", server.base)))
        .unwrap();
    assert_eq!(resp.status, 304);
    assert_eq!(resp.body, [] as [u8; 0]);
}

#[test]
fn slow_servers_time_out() {
    let server = Server::start(|_| {
        std::thread::sleep(Duration::from_secs(3));
        response("200 OK", &[], "late")
    });
    let started = Instant::now();
    let err = transport()
        .send(&get(format!("{}/slow", server.base)))
        .unwrap_err();
    assert!(matches!(err, TransportError::Timeout(_)), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn connection_errors_never_show_the_key() {
    // Nothing listens on port 9 (discard) on loopback in the test environment.
    let err = transport()
        .send(&get("http://127.0.0.1:9/x?appid=SECRETKEY".into()))
        .unwrap_err();
    assert!(!err.to_string().contains("SECRETKEY"), "{err}");
}

#[test]
fn posts_send_their_json_body() {
    let server = Server::start_with(|request| {
        let echo = format!(
            "{} {} {} {} {}",
            request.method,
            request.path,
            request.header("content-type").unwrap_or("-"),
            request.header("x-test").unwrap_or("-"),
            request.body
        );
        response("200 OK", &[], &echo)
    });
    let post = HttpRequest {
        method: Method::Post,
        body: Some(r#"{"sid":"LGA","elems":"maxt"}"#.into()),
        headers: vec![("X-Test", "yes".into())],
        ..get(format!("{}/StnData", server.base))
    };
    let resp = transport().send(&post).unwrap();
    assert_eq!(
        String::from_utf8(resp.body).unwrap(),
        r#"POST /StnData application/json yes {"sid":"LGA","elems":"maxt"}"#
    );
    let plain = transport()
        .send(&get(format!("{}/x", server.base)))
        .unwrap();
    assert!(
        String::from_utf8(plain.body)
            .unwrap()
            .starts_with("GET /x - - ")
    );
}

#[test]
fn key_headers_reach_the_server_on_a_get() {
    // Pirate Weather's key travels in `apikey`.
    let server = Server::start_with(|request| {
        response("200 OK", &[], request.header("apikey").unwrap_or("-"))
    });
    let request = HttpRequest {
        headers: vec![("apikey", "HEADERKEY".into())],
        ..get(format!("{}/forecast/key/40,-75", server.base))
    };
    let resp = transport().send(&request).unwrap();
    assert_eq!(String::from_utf8(resp.body).unwrap(), "HEADERKEY");
}
