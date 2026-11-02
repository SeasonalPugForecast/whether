//! A tiny HTTP/1.1 server on 127.0.0.1 that answers each request from a closure.

#![allow(dead_code, unreachable_pub)]

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// A request as the server read it.
#[derive(Clone, Debug, Default)]
pub struct Request {
    /// `GET`, `POST`, …
    pub method: String,
    /// Path and query.
    pub path: String,
    /// Header names (lower case) and values, in order.
    pub headers: Vec<(String, String)>,
    /// The body (`Content-Length` bytes).
    pub body: String,
}

impl Request {
    /// The first value of header `name` (lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A canned response: raw bytes written after the request is read.
pub type Responder = dyn Fn(&Request) -> Vec<u8> + Send + Sync;

/// A running server; requests are counted.
pub struct Server {
    pub base: String,
    pub hits: Arc<AtomicUsize>,
}

impl Server {
    /// Serves until the test process ends. `respond` gets the request path.
    pub fn start(respond: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) -> Self {
        Self::start_with(move |request| respond(&request.path))
    }

    /// Serves until the test process ends. `respond` gets the whole request.
    pub fn start_with(respond: impl Fn(&Request) -> Vec<u8> + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let respond: Arc<Responder> = Arc::new(respond);
        let counter = Arc::clone(&hits);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let respond = Arc::clone(&respond);
                counter.fetch_add(1, Ordering::SeqCst);
                thread::spawn(move || serve(stream, &*respond));
            }
        });
        Self { base, hits }
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn serve(stream: TcpStream, respond: &Responder) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut words = request_line.split_whitespace();
    let mut request = Request {
        method: words.next().unwrap_or("").to_string(),
        path: words.next().unwrap_or("/").to_string(),
        ..Request::default()
    };
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
        if let Some((name, value)) = line.trim_end().split_once(':') {
            request
                .headers
                .push((name.trim().to_lowercase(), value.trim().to_string()));
        }
        line.clear();
    }
    let length = request
        .header("content-length")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_ok() {
        request.body = String::from_utf8_lossy(&body).into_owned();
    }
    let mut stream = stream;
    let _ = stream.write_all(&respond(&request));
    let _ = stream.flush();
}

/// A complete response with `Content-Length`.
pub fn response(status: &str, headers: &[(&str, &str)], body: &str) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        let _ = write!(out, "{k}: {v}\r\n");
    }
    out.push_str("\r\n");
    out.push_str(body);
    out.into_bytes()
}

/// A chunked response.
pub fn chunked(chunks: &[&str]) -> Vec<u8> {
    let mut out =
        String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
    for chunk in chunks {
        let _ = write!(out, "{:x}\r\n{chunk}\r\n", chunk.len());
    }
    out.push_str("0\r\n\r\n");
    out.into_bytes()
}
