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
    format!(
        "<!doctype html><title>Recorder</title>\
         <h1>Recorder page (other origin)</h1>\
         <form method=\"post\" action=\"http://127.0.0.1:{lab_port}/submit\">\
         <input type=\"hidden\" name=\"csrf\" value=\"guessed-by-the-page\">\
         <input type=\"hidden\" name=\"note\" value=\"cross-origin\">\
         <button type=\"submit\">POST to the Laboratory</button></form>\n"
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
            Response::new(200, "text/html; charset=utf-8", attack_page(lab_port))
        } else {
            Response::text(404, "not found\n")
        }
    });
    spawn_server(listener, handler);
    Ok(addr)
}
