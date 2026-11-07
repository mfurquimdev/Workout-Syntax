//! A small HTTP/1.1 server for the sync API, built on the standard library
//! only. It is meant for `127.0.0.1` behind Caddy, or a VPN address.
//!
//! - `GET /api/log`: the whole log, for a device's first open
//! - `POST /api/sync`: see [`crate::sync`]
//! - `GET /api/health`: `ok`
//! - anything else: a file from the app directory, when one is given
//!
//! Every request must name an allowed `Host`. A request with an `Origin`
//! must come from an allowed host too. No CORS headers are sent, so other
//! web pages cannot call the API.

use crate::sync::{LogStore, SyncReply, SyncRequest};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Largest request body accepted: the log is about 11 KB today and grows
/// about 24 KB a year, and a sync carries two copies.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

pub struct Config {
    pub store: LogStore,
    /// `host` or `host:port` values a request's `Host` may have
    pub allowed_hosts: Vec<String>,
    /// Directory of the built web app, served for paths outside `/api/`
    pub app_dir: Option<PathBuf>,
}

/// Serves until the listener fails. One thread per connection; syncs are
/// serialized by the store's lock.
pub fn serve(listener: TcpListener, config: Config) -> std::io::Result<()> {
    let config = Arc::new(config);
    for stream in listener.incoming() {
        let stream = stream?;
        let config = Arc::clone(&config);
        std::thread::spawn(move || {
            let _ = handle(stream, &config);
        });
    }
    Ok(())
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn json(status: u16, value: serde_json::Value) -> Response {
    Response { status, content_type: "application/json; charset=utf-8", body: value.to_string().into_bytes() }
}

fn message(status: u16, text: &str) -> Response {
    json(status, serde_json::json!({ "status": "error", "message": text }))
}

fn handle(stream: TcpStream, config: &Config) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let response = match read_request(&mut reader) {
        Ok(req) => route(&req, config),
        Err(r) => r,
    };
    write_response(stream, response)
}

fn read_request(reader: &mut impl BufRead) -> Result<Request, Response> {
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|_| message(400, "cannot read the request"))?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return Err(message(400, "bad request line"));
    };
    let (method, path) = (method.to_string(), path.to_string());
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h).map_err(|_| message(400, "cannot read the headers"))?;
        let h = h.trim_end();
        if n == 0 || h.is_empty() {
            break;
        }
        if headers.len() > 100 {
            return Err(message(431, "too many headers"));
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.parse::<usize>().map_err(|_| message(400, "bad Content-Length")))
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err(message(413, "the request is too large"));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| message(400, "the body is shorter than Content-Length"))?;
    Ok(Request { method, path, headers, body })
}

fn route(req: &Request, config: &Config) -> Response {
    let Some(host) = req.header("host") else { return message(400, "missing Host") };
    if !allowed(host, &config.allowed_hosts) {
        return message(421, "this host name is not served here");
    }
    if let Some(origin) = req.header("origin") {
        let origin_host = origin.split_once("://").map_or(origin, |(_, h)| h);
        if !allowed(origin_host, &config.allowed_hosts) {
            return message(403, "requests from other sites are refused");
        }
    }
    let path = req.path.split('?').next().unwrap_or("/");
    match (req.method.as_str(), path) {
        ("GET", "/api/health") => Response { status: 200, content_type: "text/plain; charset=utf-8", body: b"ok".to_vec() },
        ("GET", "/api/log") => match config.store.read() {
            Ok(log) => json(200, serde_json::to_value(log).unwrap()),
            Err(e) => message(500, &e),
        },
        ("POST", "/api/sync") => {
            let sync: SyncRequest = match serde_json::from_slice(&req.body) {
                Ok(s) => s,
                Err(e) => return message(400, &format!("bad sync request: {e}")),
            };
            let reply = config.store.sync(&sync);
            let status = match &reply {
                SyncReply::Merged { .. } => 200,
                SyncReply::Confirm { .. } | SyncReply::Wait { .. } => 409,
                SyncReply::Error { http, .. } => *http,
            };
            json(status, serde_json::to_value(reply).unwrap())
        }
        (_, p) if p.starts_with("/api/") => message(404, "no such API"),
        ("GET" | "HEAD", p) => static_file(config.app_dir.as_deref(), p),
        _ => message(405, "method not allowed"),
    }
}

/// `host` matches an allowed entry with or without the port.
fn allowed(host: &str, allowed: &[String]) -> bool {
    let bare = host.rsplit_once(':').filter(|(_, p)| p.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(h, _)| h);
    allowed.iter().any(|a| a.eq_ignore_ascii_case(host) || a.eq_ignore_ascii_case(bare))
}

fn static_file(dir: Option<&Path>, path: &str) -> Response {
    let Some(dir) = dir else { return message(404, "no app is served here") };
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return message(404, "not found");
    }
    let mut file = dir.join(rel);
    if file.is_dir() {
        file = file.join("index.html");
    }
    if !file.is_file() {
        // a single page app: unknown paths get the page itself
        file = dir.join("index.html");
    }
    match std::fs::read(&file) {
        Ok(body) => Response { status: 200, content_type: content_type(&file), body },
        Err(_) => message(404, "not found"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn write_response(mut stream: TcpStream, r: Response) -> std::io::Result<()> {
    let reason = match r.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        r.status,
        r.content_type,
        r.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}
