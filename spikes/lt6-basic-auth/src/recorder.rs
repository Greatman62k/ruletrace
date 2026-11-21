//! Raw recorder: serves a page with a cross-origin form and logs every request verbatim.
//!
//! The log shows whether a browser sent an `Authorization` header to a server
//! on another port of `127.0.0.1` after the user logged in to the Laboratory.

use crate::http::{bind_local, spawn_server, Handler, Request, Response};
use crate::util::iso_utc;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// The page served by the recorder: a button that POSTs to the Laboratory.
pub fn attack_page(lab_port: u16) -> String {
    attack_page_with_probe(lab_port, None)
}

/// True for a token as the Laboratory makes it: 32 lowercase hex characters.
pub fn valid_probe_token(token: &str) -> bool {
    token.len() == 32
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// As `attack_page`; with a probe token the page also requests
/// `/s/<token>/probe` on the recorder's own origin, the control that shows
/// the browser sends the binding cookie to a server that knows the token.
pub fn attack_page_with_probe(lab_port: u16, probe_token: Option<&str>) -> String {
    let probe = probe_token
        .map(|t| format!("<img src=\"/s/{t}/probe\" alt=\"probe\">"))
        .unwrap_or_default();
    format!(
        "<!doctype html><title>Recorder</title>\
         <h1>Recorder page (other origin)</h1>\
         <form method=\"post\" action=\"http://127.0.0.1:{lab_port}/submit\">\
         <input type=\"hidden\" name=\"csrf\" value=\"guessed-by-the-page\">\
         <input type=\"hidden\" name=\"note\" value=\"cross-origin\">\
         <button type=\"submit\">POST to the Laboratory</button></form>{probe}\n"
    )
}

/// Formats the verbatim log block of one request: one line per head line.
pub fn format_record(req: &Request, peer: SocketAddr, now: SystemTime) -> String {
    let ts = iso_utc(now);
    let mut out = format!("{ts} peer={peer} BEGIN\n");
    for line in req.raw_head.split("\r\n") {
        out.push_str(&format!("{ts} | {line}\n"));
    }
    out.push_str(&format!(
        "{ts} peer={peer} END body_bytes={}\n",
        req.body.len()
    ));
    out
}

/// Starts the recorder on `127.0.0.1:port`; every request is written to
/// standard output and appended to `log_path`.
pub fn start(port: u16, lab_port: u16, log_path: PathBuf) -> io::Result<SocketAddr> {
    start_with_probe(port, lab_port, log_path, None)
}

/// As `start`; `probe_token` adds the probe request to the page.
pub fn start_with_probe(
    port: u16,
    lab_port: u16,
    log_path: PathBuf,
    probe_token: Option<String>,
) -> io::Result<SocketAddr> {
    if probe_token
        .as_deref()
        .is_some_and(|t| !valid_probe_token(t))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "probe token must be 32 lowercase hex characters",
        ));
    }
    let listener = bind_local(port)?;
    let addr = listener.local_addr()?;
    let file = Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    );
    let handler: Handler = Arc::new(move |req, peer| {
        let record = format_record(req, peer, SystemTime::now());
        print!("{record}");
        let _ = io::stdout().flush();
        if let Ok(mut f) = file.lock() {
            let _ = f.write_all(record.as_bytes());
            let _ = f.flush();
        }
        if req.method == "GET" && req.target == "/" {
            Response::new(
                200,
                "text/html; charset=utf-8",
                attack_page_with_probe(lab_port, probe_token.as_deref()),
            )
        } else {
            Response::text(404, "not found\n")
        }
    });
    spawn_server(listener, handler);
    Ok(addr)
}
