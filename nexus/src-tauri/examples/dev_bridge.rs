//! Dev/test only: serve the real JSON-RPC router over localhost HTTP so the
//! React UI can be exercised in a plain browser (headless e2e, no WebView).
//!
//!   cargo run --example dev_bridge -- 7777
//!
//! POST /rpc      body = JSON-RPC 2.0 request → response
//! GET  /events?since=N   → {"next": M, "events": [[name, payload], ...]} (long-poll ≤1 s)
//!
//! Binds 127.0.0.1 only. Not part of the shipped app.

use nexus_lib::commands::router;
use nexus_lib::rpc::Request;
use nexus_lib::state::{AppState, EventSink};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<(String, Value)>>,
    cv: Condvar,
}

impl EventSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.events.lock().unwrap().push((event.to_owned(), payload));
        self.cv.notify_all();
    }
}

fn respond(mut s: TcpStream, status: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn handle(stream: TcpStream, state: Arc<AppState>, rec: Arc<Recorder>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_owned());
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;

    match (method, target.split('?').next().unwrap_or("")) {
        ("OPTIONS", _) => respond(stream, "204 No Content", ""),
        ("POST", "/rpc") => {
            let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let id = v.get("id").cloned().unwrap_or(Value::Null);
            let res = match serde_json::from_value::<Request>(v) {
                Ok(req) => serde_json::to_string(&router().dispatch(&state, req)).unwrap(),
                Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32600, "message": e.to_string()}}).to_string(),
            };
            respond(stream, "200 OK", &res);
        }
        ("GET", "/events") => {
            let since: usize = target.split("since=").nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let guard = rec.events.lock().unwrap();
            let (guard, _) = rec.cv.wait_timeout_while(guard, Duration::from_secs(1), |e| e.len() <= since).unwrap();
            let evs: Vec<&(String, Value)> = guard.iter().skip(since).collect();
            respond(stream, "200 OK", &json!({"next": guard.len(), "events": evs}).to_string());
        }
        _ => respond(stream, "404 Not Found", "{}"),
    }
    Ok(())
}

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "7777".into());
    let rec = Arc::new(Recorder::default());
    let state = Arc::new(AppState::new(rec.clone()));
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind");
    eprintln!("nexus dev bridge on http://127.0.0.1:{port}");
    for s in listener.incoming().flatten() {
        let (state, rec) = (state.clone(), rec.clone());
        std::thread::spawn(move || {
            if let Err(e) = handle(s, state, rec) {
                eprintln!("bridge: {e}");
            }
        });
    }
}
