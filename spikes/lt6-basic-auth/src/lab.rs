//! Stand-in Laboratory: HTTP Basic authentication on every request, no cookie.
//!
//! Check order for every request: Host, then (POST only) Origin, then
//! Authorization, then (POST only) the CSRF token. Host and Origin come first
//! so that a cross-origin POST is a 403 whether or not the browser attached
//! credentials, and never triggers a second credential prompt.

use crate::http::{bind_local, spawn_server, Handler, Request, Response};
use crate::util::{base64_decode, ct_eq, iso_utc, random_hex};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;

/// The fixed user name of the Basic credentials.
pub const USER: &str = "lab";
const REALM: &str = "RuleTrace Laboratory spike";

/// Why a request got its response; logged for the manual protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Ok,
    BadHost,
    BadOrigin,
    Unauthorized,
    BadCsrf,
    Other,
}

/// Stand-in Laboratory state: the launch secret, the CSRF token and the port.
pub struct Lab {
    secret: String,
    csrf: String,
    port: u16,
}

impl Lab {
    pub fn new(secret: String, csrf: String, port: u16) -> Self {
        Lab { secret, csrf, port }
    }

    fn expected_host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn expected_origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn authorized(&self, req: &Request) -> bool {
        let Some(value) = req.single_header("Authorization") else {
            return false;
        };
        let Some((scheme, encoded)) = value.split_once(' ') else {
            return false;
        };
        if !scheme.eq_ignore_ascii_case("Basic") {
            return false;
        }
        let Some(decoded) = base64_decode(encoded.trim()) else {
            return false;
        };
        let Some(split) = decoded.iter().position(|&b| b == b':') else {
            return false;
        };
        let (user, password) = (&decoded[..split], &decoded[split + 1..]);
        // Both comparisons always run so that a wrong user name is not faster.
        let user_ok = ct_eq(user, USER.as_bytes());
        let password_ok = ct_eq(password, self.secret.as_bytes());
        user_ok & password_ok
    }

    fn csrf_ok(&self, req: &Request) -> bool {
        let body = String::from_utf8_lossy(&req.body);
        body.split('&')
            .filter_map(|pair| pair.split_once('='))
            .any(|(k, v)| k == "csrf" && ct_eq(v.as_bytes(), self.csrf.as_bytes()))
    }

    fn page(&self) -> String {
        format!(
            "<!doctype html><title>Stand-in Laboratory</title>\
             <h1>Stand-in Laboratory</h1>\
             <form method=\"post\" action=\"/submit\">\
             <input type=\"hidden\" name=\"csrf\" value=\"{}\">\
             <input type=\"text\" name=\"note\" value=\"hello\">\
             <button type=\"submit\">Submit</button></form>\n",
            self.csrf
        )
    }

    /// Decides one request. Never sets a cookie and ignores any `Cookie` header.
    pub fn handle(&self, req: &Request) -> (Response, Decision) {
        let is_post = req.method == "POST";
        if req.single_header("Host") != Some(self.expected_host().as_str()) {
            return (Response::text(403, "forbidden: host\n"), Decision::BadHost);
        }
        if is_post && req.single_header("Origin") != Some(self.expected_origin().as_str()) {
            return (
                Response::text(403, "forbidden: origin\n"),
                Decision::BadOrigin,
            );
        }
        if !self.authorized(req) {
            let challenge = format!("Basic realm=\"{REALM}\", charset=\"UTF-8\"");
            let response =
                Response::text(401, "unauthorized\n").with_header("WWW-Authenticate", &challenge);
            return (response, Decision::Unauthorized);
        }
        if is_post && !self.csrf_ok(req) {
            return (Response::text(403, "forbidden: csrf\n"), Decision::BadCsrf);
        }
        match (req.method.as_str(), req.target.as_str()) {
            ("GET", "/") => (
                Response::new(200, "text/html; charset=utf-8", self.page()),
                Decision::Ok,
            ),
            ("POST", "/submit") => (Response::text(200, "accepted\n"), Decision::Ok),
            ("GET" | "POST", _) => (Response::text(404, "not found\n"), Decision::Other),
            _ => (Response::text(405, "method not allowed\n"), Decision::Other),
        }
    }
}

/// One log line per request. It never contains the secret or the header value.
fn log_line(req: &Request, peer: SocketAddr, decision: Decision, status: u16) -> String {
    let show = |name: &str| match req.header_values(name).as_slice() {
        [] => "-".to_string(),
        [one] => (*one).to_string(),
        many => format!("{} values", many.len()),
    };
    let auth = match req.header_values("Authorization").len() {
        0 => "absent",
        _ => "present",
    };
    format!(
        "{} lab peer={peer} {} {} host={} origin={} authorization={auth} cookie={} -> {status} {decision:?}",
        iso_utc(SystemTime::now()),
        req.method,
        req.target,
        show("Host"),
        show("Origin"),
        if req.header_values("Cookie").is_empty() { "absent" } else { "present" },
    )
}

/// Starts the stand-in Laboratory on `127.0.0.1:port` (0 for a free port).
///
/// When `log` is true each request is logged to standard error.
pub fn start(secret: String, csrf: String, port: u16, log: bool) -> io::Result<SocketAddr> {
    let listener = bind_local(port)?;
    let addr = listener.local_addr()?;
    let lab = Arc::new(Lab::new(secret, csrf, addr.port()));
    let handler: Handler = Arc::new(move |req, peer| {
        let (response, decision) = lab.handle(req);
        if log {
            eprintln!("{}", log_line(req, peer, decision, response.status));
        }
        response
    });
    spawn_server(listener, handler);
    Ok(addr)
}

/// Generates a fresh secret and CSRF token and starts the Laboratory.
///
/// The secret is returned so that the caller can print it once.
pub fn start_with_fresh_secret(port: u16) -> io::Result<(SocketAddr, String)> {
    let secret = random_hex(32)?;
    let csrf = random_hex(16)?;
    let addr = start(secret.clone(), csrf, port, true)?;
    Ok((addr, secret))
}
