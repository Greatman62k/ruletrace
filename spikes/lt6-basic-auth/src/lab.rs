//! Stand-in Laboratory for LT-6 option A++ (UD-48 section 3).
//!
//! Check order for every request: Host, request-target syntax, then (POST
//! only) Origin and `Sec-Fetch-Site`, Basic credentials, Session_Path with
//! the binding cookie, and (POST only) the CSRF token.
//!
//! The first `GET /` with valid credentials starts the one session in a single
//! compare-and-set under a mutex and answers 303 to `/s/<token>/` with a
//! binding cookie scoped to that path. Not implemented in this prototype and
//! owed by `ruletrace-lab` (task 13.4): the 30-minute inactivity rule and any
//! session end. Here a session lasts until the process stops.

use crate::http::{bind_local, spawn_server, Handler, Request, Response};
use crate::util::{base64_decode, ct_eq, iso_utc, random_hex};
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// The fixed user name of the Basic credentials.
pub const USER: &str = "lab";
const REALM: &str = "RuleTrace Laboratory spike";
const TOKEN_PREFIX: &str = "/s/";

/// Body of every 403 while a session exists (UD-48 section 3, item 4).
pub const SESSION_DENIED_BODY: &str = "forbidden\n\
    The session runs in the tab where it was started.\n\
    A new tab must use that tab's address.\n\
    Another browser needs a restart of the Laboratory.\n\
    If you did not start the session, restart the Laboratory at once.\n";

/// Receives one log line per request.
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Produces one random identifier (32 lowercase hex characters).
pub type IdSource = fn() -> io::Result<String>;

/// 16 random bytes from the operating system as lowercase hex.
pub fn os_id() -> io::Result<String> {
    random_hex(16)
}

/// Why a request got its response; logged for the manual protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Ok,
    Started,
    BadHost,
    BadTarget,
    BadOrigin,
    BadFetchSite,
    Unauthorized,
    NoSession,
    SessionDenied,
    BadCsrf,
    Other,
    Failed,
}

/// What the request carried for the binding cookie, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Match,
    NoMatch,
    Absent,
}

impl Binding {
    fn label(self) -> &'static str {
        match self {
            Binding::Match => "match",
            Binding::NoMatch => "nomatch",
            Binding::Absent => "absent",
        }
    }
}

/// Outcome of one request.
pub struct Handled {
    pub response: Response,
    pub decision: Decision,
    session_path: bool,
    binding: Binding,
}

#[derive(Clone)]
struct Session {
    token: String,
    identifier: String,
}

/// Stand-in Laboratory state. Holds at most one session.
pub struct Lab {
    secret: String,
    csrf: String,
    port: u16,
    ids: IdSource,
    session: Mutex<Option<Session>>,
}

fn target_syntax_ok(target: &str) -> bool {
    if target.contains('\\') {
        return false;
    }
    let lower = target.to_ascii_lowercase();
    if ["%2e", "%2f", "%5c"].iter().any(|p| lower.contains(p)) {
        return false;
    }
    let path = target.split('?').next().unwrap_or("");
    !path.split('/').any(|seg| seg == "." || seg == "..")
}

fn fetch_site_refused(req: &Request) -> bool {
    req.header_values("Sec-Fetch-Site").iter().any(|v| {
        let v = v.trim();
        v.eq_ignore_ascii_case("same-site") || v.eq_ignore_ascii_case("cross-site")
    })
}

/// Replaces control characters so that a request cannot forge log lines.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

impl Lab {
    pub fn new(secret: String, csrf: String, port: u16, ids: IdSource) -> Self {
        Lab {
            secret,
            csrf,
            port,
            ids,
            session: Mutex::new(None),
        }
    }

    fn expected_host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn expected_origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn cookie_name(&self) -> String {
        format!("rt_lab_{}", self.port)
    }

    fn current(&self) -> Option<Session> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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

    /// Byte-prefix match of the raw target against `/s/<token>/`.
    fn in_session_path(&self, target: &str, session: &Session) -> bool {
        let t = target.as_bytes();
        let end = TOKEN_PREFIX.len() + session.token.len();
        let prefix_ok = t.starts_with(TOKEN_PREFIX.as_bytes());
        let token_ok = ct_eq(
            t.get(TOKEN_PREFIX.len()..end).unwrap_or(&[]),
            session.token.as_bytes(),
        );
        let slash_ok = t.get(end) == Some(&b'/');
        prefix_ok & token_ok & slash_ok
    }

    fn binding(&self, req: &Request, session: Option<&Session>) -> Binding {
        let name = self.cookie_name();
        let mut seen = false;
        let mut matched = false;
        for line in req.header_values("Cookie") {
            for pair in line.split(';') {
                let Some((n, v)) = pair.trim().split_once('=') else {
                    continue;
                };
                if n == name {
                    seen = true;
                    // No early exit: every value is compared.
                    let id = session.map_or("", |s| s.identifier.as_str());
                    matched |= session.is_some() & ct_eq(v.as_bytes(), id.as_bytes());
                }
            }
        }
        match (seen, matched) {
            (_, true) => Binding::Match,
            (true, false) => Binding::NoMatch,
            (false, false) => Binding::Absent,
        }
    }

    fn page(&self) -> String {
        format!(
            "<!doctype html><title>Stand-in Laboratory</title>\
             <h1>Stand-in Laboratory</h1>\
             <form method=\"post\" action=\"submit\">\
             <input type=\"hidden\" name=\"csrf\" value=\"{}\">\
             <input type=\"text\" name=\"note\" value=\"hello\">\
             <button type=\"submit\">Submit</button></form>\n",
            self.csrf
        )
    }

    fn no_session_response(&self) -> Response {
        Response::text(
            403,
            &format!("forbidden: open http://127.0.0.1:{}/ to start\n", self.port),
        )
    }

    /// Handles `GET /` while no session is known to this request.
    fn start_or_refuse(&self, req: &Request) -> (Response, Decision) {
        let starts = req.method == "GET"
            && req.target == "/"
            && req.header_values("Sec-Purpose").is_empty()
            && req.header_values("Purpose").is_empty();
        if !starts {
            return (self.no_session_response(), Decision::NoSession);
        }
        let (Ok(token), Ok(identifier)) = ((self.ids)(), (self.ids)()) else {
            return (Response::text(500, "internal error\n"), Decision::Failed);
        };
        if token == identifier {
            return (Response::text(500, "internal error\n"), Decision::Failed);
        }
        let mut slot = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return (
                Response::text(403, SESSION_DENIED_BODY),
                Decision::SessionDenied,
            );
        }
        *slot = Some(Session {
            token: token.clone(),
            identifier: identifier.clone(),
        });
        drop(slot);
        let cookie = format!(
            "{}={identifier}; HttpOnly; SameSite=Strict; Path=/s/{token}/",
            self.cookie_name()
        );
        let response = Response::text(303, "")
            .with_header("Location", &format!("/s/{token}/"))
            .with_header("Set-Cookie", &cookie);
        (response, Decision::Started)
    }

    fn decide(&self, req: &Request, snapshot: Option<Session>) -> (Response, Decision) {
        let is_post = req.method == "POST";
        if req.single_header("Host") != Some(self.expected_host().as_str()) {
            return (Response::text(403, "forbidden: host\n"), Decision::BadHost);
        }
        if !target_syntax_ok(&req.target) {
            return (
                Response::text(400, "bad request: target\n"),
                Decision::BadTarget,
            );
        }
        if is_post && req.single_header("Origin") != Some(self.expected_origin().as_str()) {
            return (
                Response::text(403, "forbidden: origin\n"),
                Decision::BadOrigin,
            );
        }
        if fetch_site_refused(req) {
            return (
                Response::text(403, "forbidden: fetch site\n"),
                Decision::BadFetchSite,
            );
        }
        if !self.authorized(req) {
            let challenge = format!("Basic realm=\"{REALM}\", charset=\"UTF-8\"");
            let response =
                Response::text(401, "unauthorized\n").with_header("WWW-Authenticate", &challenge);
            return (response, Decision::Unauthorized);
        }
        let Some(session) = snapshot else {
            return self.start_or_refuse(req);
        };
        let path_ok = self.in_session_path(&req.target, &session);
        let cookie_ok = self.binding(req, Some(&session)) == Binding::Match;
        if !(path_ok && cookie_ok) {
            return (
                Response::text(403, SESSION_DENIED_BODY),
                Decision::SessionDenied,
            );
        }
        if is_post && !self.csrf_ok(req) {
            return (Response::text(403, "forbidden: csrf\n"), Decision::BadCsrf);
        }
        let rest = req
            .target
            .get(TOKEN_PREFIX.len() + session.token.len() + 1..)
            .unwrap_or("");
        match (req.method.as_str(), rest) {
            ("GET", "") => (
                Response::new(200, "text/html; charset=utf-8", self.page()),
                Decision::Ok,
            ),
            ("POST", "submit") => (Response::text(200, "accepted\n"), Decision::Ok),
            ("GET" | "POST", _) => (Response::text(404, "not found\n"), Decision::Other),
            _ => (Response::text(405, "method not allowed\n"), Decision::Other),
        }
    }

    /// Decides one request. The session seen by the checks is one snapshot.
    pub fn handle(&self, req: &Request) -> Handled {
        let snapshot = self.current();
        let session_path = snapshot
            .as_ref()
            .is_some_and(|s| self.in_session_path(&req.target, s));
        let binding = self.binding(req, snapshot.as_ref());
        let (response, decision) = self.decide(req, snapshot);
        Handled {
            response,
            decision,
            session_path,
            binding,
        }
    }

    /// The path for the log: token and identifier replaced, controls masked.
    fn redacted(&self, target: &str) -> String {
        let mut out = target.to_string();
        if let Some(s) = self.current() {
            out = out.replace(&s.token, "<session>");
            out = out.replace(&s.identifier, "<session>");
        }
        printable(&out)
    }

    /// One log line per request. Never holds the secret, token, identifier,
    /// Authorization value or CSRF token, and never the raw target.
    pub fn log_line(&self, req: &Request, peer: SocketAddr, handled: &Handled) -> String {
        let show = |name: &str| match req.header_values(name).as_slice() {
            [] => "-".to_string(),
            [one] => printable(one),
            many => format!("{} values", many.len()),
        };
        let present = |name: &str| {
            if req.header_values(name).is_empty() {
                "absent"
            } else {
                "present"
            }
        };
        format!(
            "{} lab peer={peer} {} {} host={} origin={} authorization={} cookie={} \
             session_path={} binding_cookie={} -> {} {:?}",
            iso_utc(SystemTime::now()),
            printable(&req.method),
            self.redacted(&req.target),
            show("Host"),
            show("Origin"),
            present("Authorization"),
            present("Cookie"),
            if handled.session_path { "yes" } else { "no" },
            handled.binding.label(),
            handled.response.status,
            handled.decision,
        )
    }
}

/// Starts the Laboratory on `127.0.0.1:port` (0 for a free port).
///
/// `log` receives one line per request; `ids` supplies the token and the
/// identifier of the session.
pub fn start_custom(
    secret: String,
    csrf: String,
    port: u16,
    log: Option<LogSink>,
    ids: IdSource,
) -> io::Result<SocketAddr> {
    let listener = bind_local(port)?;
    let addr = listener.local_addr()?;
    let lab = Arc::new(Lab::new(secret, csrf, addr.port(), ids));
    let handler: Handler = Arc::new(move |req, peer| {
        let handled = lab.handle(req);
        if let Some(sink) = &log {
            sink(&lab.log_line(req, peer, &handled));
        }
        handled.response
    });
    spawn_server(listener, handler);
    Ok(addr)
}

/// Starts the Laboratory with operating-system randomness.
///
/// When `log` is true each request is logged to standard error.
pub fn start(secret: String, csrf: String, port: u16, log: bool) -> io::Result<SocketAddr> {
    let sink: Option<LogSink> = log.then(|| Arc::new(|l: &str| eprintln!("{l}")) as LogSink);
    start_custom(secret, csrf, port, sink, os_id)
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
