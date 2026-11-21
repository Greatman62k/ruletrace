//! Server-side evidence for LT-6 option A++ (UD-48 section 3, evidence item 1).
//!
//! Real sockets on 127.0.0.1:0 and hand-written HTTP requests. Expected
//! statuses, header values and texts are literals taken from UD-48, not from
//! the server code. Base64 credentials are literals produced with the
//! coreutils `base64` tool: `lab:s3cret-launch-value`, and the wrong variants.

use lt6_basic_auth::{lab, recorder};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

const SECRET: &str = "s3cret-launch-value";
const CSRF: &str = "0123456789abcdef0123456789abcdef";
const GOOD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdWU=";
const WRONG_PASSWORD: &str = "bGFiOndyb25nLXBhc3N3b3Jk";
const WRONG_USER: &str = "b3RoZXI6czNjcmV0LWxhdW5jaC12YWx1ZQ==";
const SHORT_PASSWORD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdQ==";
const LONG_PASSWORD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdWVl";
const CHALLENGE: &str = "Basic realm=\"RuleTrace Laboratory spike\", charset=\"UTF-8\"";
const SESSION_TEXT: &str = "forbidden\n\
    The session runs in the tab where it was started.\n\
    A new tab must use that tab's address.\n\
    Another browser needs a restart of the Laboratory.\n\
    If you did not start the session, restart the Laboratory at once.\n";

struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn headers(&self, name: &str) -> Vec<String> {
        let prefix = format!("{}:", name.to_ascii_lowercase());
        self.head
            .lines()
            .skip(1)
            .filter(|l| l.to_ascii_lowercase().starts_with(&prefix))
            .map(|l| l[prefix.len()..].trim().to_string())
            .collect()
    }

    fn header(&self, name: &str) -> Option<String> {
        let all = self.headers(name);
        assert!(all.len() <= 1, "{name} repeated: {all:?}");
        all.into_iter().next()
    }

    fn whole(&self) -> String {
        format!("{}\r\n\r\n{}", self.head, self.body)
    }
}

#[cfg(unix)]
fn ids() -> lab::IdSource {
    lab::os_id
}

#[cfg(not(unix))]
fn ids() -> lab::IdSource {
    fn counter() -> std::io::Result<String> {
        static N: AtomicU32 = AtomicU32::new(0);
        Ok(format!("{:032x}", N.fetch_add(1, Ordering::SeqCst) + 1))
    }
    counter
}

type Logs = Arc<Mutex<Vec<String>>>;

fn start_lab() -> SocketAddr {
    lab::start_custom(SECRET.into(), CSRF.into(), 0, None, ids()).unwrap()
}

fn start_logged() -> (SocketAddr, Logs) {
    let logs: Logs = Arc::new(Mutex::new(Vec::new()));
    let sink_logs = Arc::clone(&logs);
    let sink: lab::LogSink = Arc::new(move |line: &str| {
        sink_logs.lock().unwrap().push(line.to_string());
    });
    let addr = lab::start_custom(SECRET.into(), CSRF.into(), 0, Some(sink), ids()).unwrap();
    (addr, logs)
}

fn send(addr: SocketAddr, raw: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Reply {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

fn basic(credentials: &str) -> String {
    format!("Authorization: Basic {credentials}")
}

fn origin_of(addr: SocketAddr) -> String {
    format!("Origin: http://127.0.0.1:{}", addr.port())
}

fn request(addr: SocketAddr, method: &str, target: &str, extra: &[String]) -> Reply {
    let mut raw = format!("{method} {target} HTTP/1.1\r\nHost: {}\r\n", host_of(addr));
    for h in extra {
        raw.push_str(h);
        raw.push_str("\r\n");
    }
    raw.push_str("\r\n");
    send(addr, &raw)
}

fn get_at(addr: SocketAddr, target: &str, extra: &[String]) -> Reply {
    request(addr, "GET", target, extra)
}

fn post_at(addr: SocketAddr, host: &str, target: &str, extra: &[String], body: &str) -> Reply {
    let mut raw = format!("POST {target} HTTP/1.1\r\nHost: {host}\r\n");
    for h in extra {
        raw.push_str(h);
        raw.push_str("\r\n");
    }
    raw.push_str(&format!(
        "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    send(addr, &raw)
}

fn good() -> Vec<String> {
    vec![basic(GOOD)]
}

fn before_session_text(addr: SocketAddr) -> String {
    format!(
        "forbidden: open http://127.0.0.1:{}/ to start\n",
        addr.port()
    )
}

struct Sess {
    token: String,
    identifier: String,
    cookie_pair: String,
}

impl Sess {
    fn path(&self) -> String {
        format!("/s/{}/", self.token)
    }

    /// Valid credentials and the binding cookie.
    fn full(&self) -> Vec<String> {
        vec![basic(GOOD), format!("Cookie: {}", self.cookie_pair)]
    }

    fn assert_hidden(&self, r: &Reply, label: &str) {
        let all = r.whole();
        assert!(!all.contains(&self.token), "{label}: token in {all}");
        assert!(
            !all.contains(&self.identifier),
            "{label}: identifier in {all}"
        );
    }
}

fn is_hex32(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Starts the session with a plain `GET /` and parses the 303.
fn start_session(addr: SocketAddr) -> Sess {
    let r = get_at(addr, "/", &good());
    assert_eq!(r.status, 303, "{}", r.whole());
    let location = r.header("location").unwrap();
    let token = location
        .strip_prefix("/s/")
        .and_then(|t| t.strip_suffix('/'))
        .unwrap()
        .to_string();
    let cookie = r.header("set-cookie").unwrap();
    let name = format!("rt_lab_{}=", addr.port());
    let identifier = cookie
        .strip_prefix(&name)
        .and_then(|c| c.split(';').next())
        .unwrap()
        .to_string();
    Sess {
        cookie_pair: format!("{name}{identifier}"),
        token,
        identifier,
    }
}

fn csrf_from_page(addr: SocketAddr, s: &Sess) -> String {
    let page = get_at(addr, &s.path(), &s.full());
    assert_eq!(page.status, 200);
    let marker = "name=\"csrf\" value=\"";
    let start = page.body.find(marker).unwrap() + marker.len();
    let end = start + page.body[start..].find('"').unwrap();
    page.body[start..end].to_string()
}

fn started_lab() -> (SocketAddr, Sess) {
    let addr = start_lab();
    let s = start_session(addr);
    (addr, s)
}

// Rule 2: credentials.

#[test]
fn no_authorization_is_401_with_basic_challenge() {
    let (addr, s) = started_lab();
    let fresh = start_lab();
    for (label, target, a) in [
        ("before session", "/".to_string(), fresh),
        ("after session, root", "/".to_string(), addr),
        ("after session, path", s.path(), addr),
    ] {
        let r = get_at(a, &target, &[]);
        assert_eq!(r.status, 401, "{label}");
        assert_eq!(r.headers("www-authenticate"), vec![CHALLENGE.to_string()]);
        assert_eq!(r.body, "unauthorized\n");
        s.assert_hidden(&r, label);
    }
}

#[test]
fn wrong_credentials_are_401_before_and_after_session() {
    let (addr, s) = started_lab();
    let fresh = start_lab();
    for (label, creds) in [
        ("wrong password", WRONG_PASSWORD),
        ("wrong user", WRONG_USER),
        ("password one char short", SHORT_PASSWORD),
        ("password one char long", LONG_PASSWORD),
    ] {
        let r = get_at(fresh, "/", &[basic(creds)]);
        assert_eq!(r.status, 401, "{label} before session");
        let extra = vec![basic(creds), format!("Cookie: {}", s.cookie_pair)];
        let r = get_at(addr, &s.path(), &extra);
        assert_eq!(r.status, 401, "{label} with right cookie");
        s.assert_hidden(&r, label);
    }
    // The failed attempts did not start a session on the fresh lab.
    assert_eq!(get_at(fresh, "/", &good()).status, 303);
}

#[test]
fn malformed_authorization_is_401() {
    let addr = start_lab();
    for header in [
        "Authorization: Basic",
        "Authorization: Basic !!!!",
        "Authorization: Bearer bGFiOnMzY3JldC1sYXVuY2gtdmFsdWU=",
        "Authorization: Basic bGFi",
    ] {
        assert_eq!(
            get_at(addr, "/", &[header.to_string()]).status,
            401,
            "{header}"
        );
    }
}

#[test]
fn two_authorization_headers_one_valid_is_401() {
    let addr = start_lab();
    for (label, first, second) in [
        ("valid first", GOOD, WRONG_PASSWORD),
        ("valid second", WRONG_PASSWORD, GOOD),
        ("valid twice", GOOD, GOOD),
    ] {
        let r = get_at(addr, "/", &[basic(first), basic(second)]);
        assert_eq!(r.status, 401, "{label}");
    }
    // None of them started a session.
    assert_eq!(get_at(addr, "/", &good()).status, 303);
}

#[test]
fn cookie_alone_never_replaces_credentials() {
    let (addr, s) = started_lab();
    let r = get_at(addr, &s.path(), &[format!("Cookie: {}", s.cookie_pair)]);
    assert_eq!(r.status, 401);
    assert_eq!(r.headers("www-authenticate").len(), 1);
    assert!(r.headers("set-cookie").is_empty());
    s.assert_hidden(&r, "cookie only");
    let r = post_at(
        addr,
        &host_of(addr),
        &format!("{}submit", s.path()),
        &[origin_of(addr), format!("Cookie: {}", s.cookie_pair)],
        &format!("csrf={CSRF}"),
    );
    assert_eq!(r.status, 401, "post with cookie only");
}

// Rule 3: session start.

#[test]
fn start_is_303_with_exact_location_and_set_cookie() {
    let addr = start_lab();
    let r = get_at(addr, "/", &good());
    assert_eq!(r.status, 303);
    assert!(r.head.starts_with("HTTP/1.1 303 "), "{}", r.head);
    let location = r.header("location").unwrap();
    let token = location
        .strip_prefix("/s/")
        .unwrap()
        .strip_suffix('/')
        .unwrap();
    assert!(is_hex32(token), "token {token}");
    let cookies = r.headers("set-cookie");
    assert_eq!(cookies.len(), 1);
    let rest = cookies[0]
        .strip_prefix(&format!("rt_lab_{}=", addr.port()))
        .unwrap();
    let (identifier, attributes) = rest.split_once(';').unwrap();
    assert!(is_hex32(identifier), "identifier {identifier}");
    assert_ne!(identifier, token);
    assert_eq!(
        attributes,
        format!(" HttpOnly; SameSite=Strict; Path=/s/{token}/")
    );
    assert_eq!(r.body, "");
}

#[cfg(unix)]
#[test]
fn two_laboratories_get_different_tokens_and_identifiers() {
    let a = start_session(start_lab());
    let b = start_session(start_lab());
    assert_ne!(a.token, b.token);
    assert_ne!(a.identifier, b.identifier);
}

#[test]
fn only_get_of_exactly_root_starts_the_session() {
    let addr = start_lab();
    let expected = before_session_text(addr);
    let get_cases = [
        "/?a=b",
        "/?",
        "/#x",
        "/other",
        "/index.html",
        "//",
        "/s/",
        "/submit",
    ];
    for target in get_cases {
        let r = get_at(addr, target, &good());
        assert_eq!(r.status, 403, "GET {target}");
        assert_eq!(r.body, expected, "GET {target}");
        assert!(r.headers("set-cookie").is_empty(), "GET {target}");
        assert!(r.headers("location").is_empty(), "GET {target}");
    }
    for method in ["HEAD", "DELETE", "PUT"] {
        let r = request(addr, method, "/", &good());
        assert_eq!(r.status, 403, "{method}");
        assert!(r.headers("set-cookie").is_empty(), "{method}");
    }
    let mut extra = good();
    extra.push(origin_of(addr));
    let r = post_at(addr, &host_of(addr), "/", &extra, &format!("csrf={CSRF}"));
    assert_eq!(r.status, 403, "POST /");
    assert_eq!(r.body, expected);
    // None of those requests started a session; a plain GET / still does.
    assert_eq!(get_at(addr, "/", &good()).status, 303);
}

#[test]
fn prefetch_and_prerender_requests_do_not_start_the_session() {
    let addr = start_lab();
    for header in [
        "Sec-Purpose: prefetch",
        "Sec-Purpose: prefetch;prerender",
        "Purpose: prefetch",
        "Purpose: prerender",
    ] {
        let mut extra = good();
        extra.push(header.to_string());
        let r = get_at(addr, "/", &extra);
        assert_eq!(r.status, 403, "{header}");
        assert_eq!(r.body, before_session_text(addr), "{header}");
        assert!(r.headers("set-cookie").is_empty(), "{header}");
    }
    assert_eq!(get_at(addr, "/", &good()).status, 303);
}

#[test]
fn other_path_with_credentials_before_session_is_403_with_open_text() {
    let addr = start_lab();
    let r = get_at(addr, "/s/00000000000000000000000000000000/", &good());
    assert_eq!(r.status, 403);
    assert_eq!(r.body, before_session_text(addr));
    assert!(r.headers("www-authenticate").is_empty());
}

#[test]
fn concurrent_starts_let_exactly_one_request_win() {
    const THREADS: usize = 16;
    for round in 0..8 {
        let addr = start_lab();
        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    get_at(addr, "/", &[basic(GOOD)])
                })
            })
            .collect();
        let replies: Vec<Reply> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners: Vec<&Reply> = replies.iter().filter(|r| r.status == 303).collect();
        assert_eq!(winners.len(), 1, "round {round}");
        let location = winners[0].header("location").unwrap();
        let token = location.strip_prefix("/s/").unwrap().trim_end_matches('/');
        for r in replies.iter().filter(|r| r.status != 303) {
            assert_eq!(r.status, 403, "round {round}");
            assert_eq!(r.body, SESSION_TEXT, "round {round}");
            assert!(r.headers("set-cookie").is_empty(), "round {round}");
            assert!(!r.whole().contains(token), "round {round}");
        }
    }
}

#[test]
fn stale_cookie_at_start_is_overwritten_by_a_new_set_cookie() {
    let addr = start_lab();
    let name = format!("rt_lab_{}", addr.port());
    let mut extra = good();
    extra.push(format!("Cookie: {name}=stale; other=1"));
    let r = get_at(addr, "/", &extra);
    assert_eq!(r.status, 303);
    let cookie = r.header("set-cookie").unwrap();
    assert!(cookie.starts_with(&format!("{name}=")), "{cookie}");
    assert!(!cookie.contains("stale"), "{cookie}");
}

// Rule 4: with a session.

#[test]
fn root_with_credentials_after_start_is_403_with_session_text_and_no_token() {
    let (addr, s) = started_lab();
    for extra in [good(), s.full()] {
        let r = get_at(addr, "/", &extra);
        assert_eq!(r.status, 403);
        assert_eq!(r.body, SESSION_TEXT);
        assert!(r.headers("www-authenticate").is_empty());
        assert!(r.headers("set-cookie").is_empty());
        assert!(r.headers("location").is_empty());
        s.assert_hidden(&r, "root after start");
    }
}

#[test]
fn right_path_credentials_and_cookie_get_the_page() {
    let (addr, s) = started_lab();
    let r = get_at(addr, &s.path(), &s.full());
    assert_eq!(r.status, 200);
    assert!(
        r.body.contains("<form method=\"post\" action=\"submit\">"),
        "{}",
        r.body
    );
    assert!(r.headers("set-cookie").is_empty());
    s.assert_hidden(&r, "page");
}

#[test]
fn right_path_and_credentials_without_a_matching_cookie_are_403() {
    let (addr, s) = started_lab();
    let name = format!("rt_lab_{}", addr.port());
    let mut wrong_same_length = s.identifier.clone();
    wrong_same_length.pop();
    wrong_same_length.push(if s.identifier.ends_with('0') {
        '1'
    } else {
        '0'
    });
    let cases: Vec<(&str, Vec<String>)> = vec![
        ("no cookie", vec![]),
        (
            "other name",
            vec![format!("Cookie: other={}", s.identifier)],
        ),
        (
            "other port name",
            vec![format!("Cookie: rt_lab_1={}", s.identifier)],
        ),
        (
            "name prefix",
            vec![format!("Cookie: x{name}={}", s.identifier)],
        ),
        (
            "name suffix",
            vec![format!("Cookie: {name}x={}", s.identifier)],
        ),
        (
            "wrong value",
            vec![format!("Cookie: {name}={wrong_same_length}")],
        ),
        (
            "short value",
            vec![format!("Cookie: {name}={}", &s.identifier[1..])],
        ),
        (
            "long value",
            vec![format!("Cookie: {name}={}0", s.identifier)],
        ),
        ("empty value", vec![format!("Cookie: {name}=")]),
        (
            "token as value",
            vec![format!("Cookie: {name}={}", s.token)],
        ),
    ];
    for (label, cookie) in cases {
        let mut extra = good();
        extra.extend(cookie);
        let r = get_at(addr, &s.path(), &extra);
        assert_eq!(r.status, 403, "{label}");
        assert_eq!(r.body, SESSION_TEXT, "{label}");
        s.assert_hidden(&r, label);
    }
}

#[test]
fn wrong_or_malformed_session_path_is_403_even_with_cookie() {
    let (addr, s) = started_lab();
    let mut flipped = s.token.clone();
    flipped.pop();
    flipped.push(if s.token.ends_with('0') { '1' } else { '0' });
    let cases = [
        ("wrong token", format!("/s/{flipped}/")),
        ("short token", format!("/s/{}/", &s.token[1..])),
        ("long token", format!("/s/{}0/", s.token)),
        (
            "upper-case token",
            format!("/s/{}/", s.token.to_ascii_uppercase()),
        ),
        ("no trailing slash", format!("/s/{}", s.token)),
        ("suffix instead of slash", format!("/s/{}x", s.token)),
        ("missing leading s", format!("/{}/", s.token)),
        ("prefix path", "/s/".to_string()),
        ("root", "/".to_string()),
        ("submit outside path", "/submit".to_string()),
        ("token inside query", format!("/?{}/", s.token)),
        ("token not at start", format!("/x/s/{}/", s.token)),
    ];
    for (label, target) in cases {
        let r = get_at(addr, &target, &s.full());
        assert_eq!(r.status, 403, "{label}");
        assert_eq!(r.body, SESSION_TEXT, "{label}");
        s.assert_hidden(&r, label);
    }
}

#[test]
fn unmatched_cookies_of_the_same_name_do_not_deny_access() {
    let (addr, s) = started_lab();
    let name = format!("rt_lab_{}", addr.port());
    let junk = format!("{name}=tossed");
    let valid = s.cookie_pair.clone();
    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "junk then valid, one line",
            vec![format!("Cookie: {junk}; {valid}")],
        ),
        (
            "valid then junk, one line",
            vec![format!("Cookie: {valid}; {junk}")],
        ),
        (
            "junk line then valid line",
            vec![format!("Cookie: {junk}"), format!("Cookie: {valid}")],
        ),
        (
            "valid line then junk line",
            vec![format!("Cookie: {valid}"), format!("Cookie: {junk}")],
        ),
        (
            "many junk values",
            vec![format!(
                "Cookie: {junk}; {name}=a; {name}=; other=1; {valid}"
            )],
        ),
    ];
    for (label, cookies) in cases {
        let mut extra = good();
        extra.extend(cookies);
        let r = get_at(addr, &s.path(), &extra);
        assert_eq!(r.status, 200, "{label}");
    }
}

// Rule 1 (criterion 4): Sec-Fetch-Site.

#[test]
fn same_site_and_cross_site_are_403_on_get_and_post() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    let fresh = start_lab();
    for value in ["same-site", "cross-site", "Same-Site", "CROSS-SITE"] {
        let header = format!("Sec-Fetch-Site: {value}");
        let mut extra = s.full();
        extra.push(header.clone());
        let r = get_at(addr, &s.path(), &extra);
        assert_eq!(r.status, 403, "GET {value}");
        assert_eq!(r.body, "forbidden: fetch site\n");
        s.assert_hidden(&r, value);
        let mut extra = s.full();
        extra.push(origin_of(addr));
        extra.push(header.clone());
        let r = post_at(
            addr,
            &host_of(addr),
            &format!("{}submit", s.path()),
            &extra,
            &format!("csrf={csrf}"),
        );
        assert_eq!(r.status, 403, "POST {value}");
        assert_eq!(r.body, "forbidden: fetch site\n");
        // Before a session it also refuses and does not start one.
        let mut extra = good();
        extra.push(header);
        assert_eq!(get_at(fresh, "/", &extra).status, 403, "start {value}");
    }
    assert_eq!(get_at(fresh, "/", &good()).status, 303);
}

#[test]
fn absent_none_and_same_origin_fetch_site_are_not_refused() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    for value in [None, Some("none"), Some("same-origin"), Some("unknown")] {
        let mut extra = s.full();
        if let Some(v) = value {
            extra.push(format!("Sec-Fetch-Site: {v}"));
        }
        assert_eq!(get_at(addr, &s.path(), &extra).status, 200, "GET {value:?}");
        extra.push(origin_of(addr));
        let r = post_at(
            addr,
            &host_of(addr),
            &format!("{}submit", s.path()),
            &extra,
            &format!("csrf={csrf}"),
        );
        assert_eq!(r.status, 200, "POST {value:?}");
    }
    let fresh = start_lab();
    let mut extra = good();
    extra.push("Sec-Fetch-Site: none".to_string());
    assert_eq!(get_at(fresh, "/", &extra).status, 303);
}

// Rule 1: Host and check order.

#[test]
fn wrong_host_is_403_before_and_after_session() {
    let (addr, s) = started_lab();
    let fresh = start_lab();
    for host in ["localhost:1", "127.0.0.1:1", "127.0.0.1", "evil.example"] {
        for (a, target, extra) in [(fresh, "/".to_string(), good()), (addr, s.path(), s.full())] {
            let mut raw = format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n");
            for h in &extra {
                raw.push_str(&format!("{h}\r\n"));
            }
            raw.push_str("\r\n");
            let r = send(a, &raw);
            assert_eq!(r.status, 403, "{host}");
            assert_eq!(r.body, "forbidden: host\n");
            s.assert_hidden(&r, host);
        }
    }
    let raw = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nHost: evil.example\r\n{}\r\n\r\n",
        host_of(fresh),
        basic(GOOD)
    );
    assert_eq!(send(fresh, &raw).status, 403, "repeated Host");
    assert_eq!(send(fresh, "GET / HTTP/1.1\r\n\r\n").status, 403, "no Host");
    // No refused Host started a session.
    assert_eq!(get_at(fresh, "/", &good()).status, 303);
}

#[test]
fn checks_run_in_the_order_host_target_origin_fetch_credentials() {
    let (addr, s) = started_lab();
    let bad_target = format!("{}../x", s.path());
    // Host before target syntax.
    let raw = format!("GET {bad_target} HTTP/1.1\r\nHost: evil.example\r\n\r\n");
    assert_eq!(send(addr, &raw).status, 403);
    // Target syntax before Origin, Sec-Fetch-Site and credentials.
    let r = post_at(
        addr,
        &host_of(addr),
        &bad_target,
        &["Origin: http://evil.example".to_string()],
        "x=1",
    );
    assert_eq!(r.status, 400, "target before origin");
    let r = get_at(
        addr,
        &bad_target,
        &["Sec-Fetch-Site: cross-site".to_string()],
    );
    assert_eq!(r.status, 400, "target before fetch site");
    assert_eq!(
        get_at(addr, &bad_target, &[]).status,
        400,
        "target before credentials"
    );
    // Origin and Sec-Fetch-Site before credentials.
    let r = post_at(addr, &host_of(addr), &s.path(), &[], "x=1");
    assert_eq!(r.status, 403, "origin before credentials");
    assert_eq!(r.body, "forbidden: origin\n");
    let r = get_at(addr, &s.path(), &["Sec-Fetch-Site: same-site".to_string()]);
    assert_eq!(r.status, 403, "fetch site before credentials");
    // Credentials before Session_Path and cookie.
    assert_eq!(get_at(addr, "/other", &[]).status, 401);
    // Session_Path and cookie before CSRF.
    let r = post_at(
        addr,
        &host_of(addr),
        &format!("{}submit", s.path()),
        &[basic(GOOD), origin_of(addr)],
        "csrf=wrong",
    );
    assert_eq!(r.status, 403);
    assert_eq!(r.body, SESSION_TEXT);
}

// Rule 5: forms.

#[test]
fn post_with_wrong_or_missing_origin_is_403() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    let target = format!("{}submit", s.path());
    let cases: [(&str, Option<&str>); 5] = [
        ("other port", Some("Origin: http://127.0.0.1:1")),
        ("other host", Some("Origin: http://localhost")),
        ("null", Some("Origin: null")),
        ("https", Some("Origin: https://127.0.0.1")),
        ("missing", None),
    ];
    for (label, origin) in cases {
        let mut extra = s.full();
        extra.extend(origin.map(str::to_string));
        let r = post_at(
            addr,
            &host_of(addr),
            &target,
            &extra,
            &format!("csrf={csrf}"),
        );
        assert_eq!(r.status, 403, "{label}");
        assert_eq!(r.body, "forbidden: origin\n", "{label}");
    }
}

#[test]
fn post_with_wrong_or_missing_csrf_is_403() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    let mut altered = csrf.clone();
    altered.pop();
    altered.push(if csrf.ends_with('0') { '1' } else { '0' });
    let mut extra = s.full();
    extra.push(origin_of(addr));
    let target = format!("{}submit", s.path());
    for (label, body) in [
        ("altered token", format!("csrf={altered}&note=x")),
        (
            "truncated token",
            format!("csrf={}&note=x", &csrf[..csrf.len() - 1]),
        ),
        ("empty token", "csrf=&note=x".to_string()),
        ("missing field", "note=x".to_string()),
        ("name only in value", format!("note=csrf={csrf}")),
    ] {
        let r = post_at(addr, &host_of(addr), &target, &extra, &body);
        assert_eq!(r.status, 403, "{label}");
        assert_eq!(r.body, "forbidden: csrf\n", "{label}");
    }
}

#[test]
fn post_with_everything_correct_is_200() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    let mut extra = s.full();
    extra.push(origin_of(addr));
    let r = post_at(
        addr,
        &host_of(addr),
        &format!("{}submit", s.path()),
        &extra,
        &format!("csrf={csrf}&note=x"),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.body, "accepted\n");
}

#[test]
fn post_missing_cookie_or_outside_session_path_is_403_even_with_csrf() {
    let (addr, s) = started_lab();
    let csrf = csrf_from_page(addr, &s);
    let body = format!("csrf={csrf}");
    let with_origin = |mut v: Vec<String>| {
        v.push(origin_of(addr));
        v
    };
    let r = post_at(
        addr,
        &host_of(addr),
        &format!("{}submit", s.path()),
        &with_origin(good()),
        &body,
    );
    assert_eq!(
        (r.status, r.body.as_str()),
        (403, SESSION_TEXT),
        "no cookie"
    );
    let r = post_at(
        addr,
        &host_of(addr),
        "/submit",
        &with_origin(s.full()),
        &body,
    );
    assert_eq!(
        (r.status, r.body.as_str()),
        (403, SESSION_TEXT),
        "outside path"
    );
}

#[test]
fn page_and_submit_use_relative_urls_and_carry_no_token() {
    let (addr, s) = started_lab();
    let page = get_at(addr, &s.path(), &s.full());
    assert_eq!(page.status, 200);
    s.assert_hidden(&page, "page");
    assert!(!page.body.contains("href=\"/") && !page.body.contains("action=\"/"));
    assert!(!page.body.contains("http://") && !page.body.contains("https://"));
    let csrf = csrf_from_page(addr, &s);
    let mut extra = s.full();
    extra.push(origin_of(addr));
    let r = post_at(
        addr,
        &host_of(addr),
        &format!("{}submit", s.path()),
        &extra,
        &format!("csrf={csrf}"),
    );
    s.assert_hidden(&r, "submit");
}

// Rule 4: request-target syntax (C5).

#[test]
fn dot_segments_backslashes_and_encoded_separators_are_400() {
    let (addr, s) = started_lab();
    let fresh = start_lab();
    let p = s.path();
    let targets = [
        format!("{p}../x"),
        format!("{p}.."),
        format!("{p}./x"),
        format!("{p}x/./y"),
        format!("{p}%2e%2e/"),
        format!("{p}%2E%2E/"),
        format!("{p}%2e/"),
        format!("{p}a%2Fb"),
        format!("{p}a%2fb"),
        format!("{p}a%5Cb"),
        format!("{p}a%5cb"),
        format!("{p}a\\b"),
        format!("{p}?q=%2f"),
        "/../".to_string(),
        "/%2e%2e/".to_string(),
        "/\\".to_string(),
        "/./".to_string(),
    ];
    for target in &targets {
        for (label, a, extra) in [
            ("full", addr, s.full()),
            ("no credentials", addr, vec![]),
            ("before session, credentials", fresh, good()),
        ] {
            let r = get_at(a, target, &extra);
            assert_eq!(r.status, 400, "{label}: {target}");
            assert_eq!(r.body, "bad request: target\n", "{label}: {target}");
            assert!(r.headers("set-cookie").is_empty());
            s.assert_hidden(&r, target);
        }
    }
    // The refusals did not start a session on the fresh laboratory.
    assert_eq!(get_at(fresh, "/", &good()).status, 303);
}

#[test]
fn near_miss_targets_are_not_syntax_errors() {
    let (addr, s) = started_lab();
    for tail in ["..x", "x..", "a.b", "%41", "%2"] {
        let r = get_at(addr, &format!("{}{tail}", s.path()), &s.full());
        assert_eq!(r.status, 404, "{tail}");
    }
}

// Rule 4 and 7: nothing secret in refusals or logs.

#[test]
fn no_refusal_contains_the_token_or_the_identifier() {
    let (addr, s) = started_lab();
    let p = s.path();
    let c = format!("Cookie: {}", s.cookie_pair);
    let fetch = "Sec-Fetch-Site: cross-site".to_string();
    let battery: Vec<(String, String, Vec<String>)> = vec![
        ("/".into(), "GET".into(), vec![]),
        ("/".into(), "GET".into(), good()),
        (p.clone(), "GET".into(), vec![]),
        (p.clone(), "GET".into(), vec![c.clone()]),
        (
            p.clone(),
            "GET".into(),
            vec![basic(WRONG_PASSWORD), c.clone()],
        ),
        (p.clone(), "GET".into(), good()),
        (
            p.clone(),
            "GET".into(),
            vec![basic(GOOD), fetch.clone(), c.clone()],
        ),
        (format!("{p}../x"), "GET".into(), s.full()),
        (format!("{p}%2e%2e/"), "GET".into(), vec![]),
        (format!("/x/{}", s.token), "GET".into(), s.full()),
        (format!("/?t={}", s.token), "GET".into(), s.full()),
        (format!("{}submit", p), "POST".into(), s.full()),
        (format!("{}submit", p), "POST".into(), vec![c.clone()]),
    ];
    for (target, method, extra) in battery {
        let r = request(addr, &method, &target, &extra);
        assert!(
            matches!(r.status, 400 | 401 | 403),
            "{method} {target}: {}",
            r.status
        );
        s.assert_hidden(&r, &format!("{method} {target}"));
    }
    let raw = format!("GET / HTTP/1.1\r\nHost: evil\r\nX: {}\r\n\r\n", s.token);
    let r = send(addr, &raw);
    assert_eq!(r.status, 403);
    s.assert_hidden(&r, "wrong host");
}

#[test]
fn log_lines_redact_the_session_and_report_the_binding_fields() {
    let (addr, logs) = start_logged();
    let s = start_session(addr);
    let csrf = csrf_from_page(addr, &s);
    let p = s.path();
    let name = format!("rt_lab_{}", addr.port());
    let steps: Vec<(String, Vec<String>)> = vec![
        (
            p.clone(),
            vec![basic(GOOD), format!("Cookie: {name}=bogus")],
        ),
        (p.clone(), good()),
        (format!("{p}../x"), s.full()),
        (format!("/x/{}/y", s.token), s.full()),
        (format!("/?t={}", s.token), s.full()),
        ("/".to_string(), vec![]),
        (format!("/?i={}", s.identifier), s.full()),
    ];
    for (target, extra) in &steps {
        let _ = get_at(addr, target, extra);
    }
    let lines = logs.lock().unwrap().clone();
    // start, csrf page, then one line per step.
    assert_eq!(lines.len(), 2 + steps.len(), "{lines:#?}");
    for line in &lines {
        for secret in [
            &s.token,
            &s.identifier,
            &csrf.to_string(),
            &SECRET.to_string(),
            &GOOD.to_string(),
        ] {
            assert!(!line.contains(secret.as_str()), "{secret} in {line}");
        }
    }
    let has = |line: &str, parts: &[&str]| parts.iter().all(|p| line.contains(p));
    assert!(
        has(&lines[0], &[" GET / host=", "authorization=present cookie=absent session_path=no binding_cookie=absent -> 303 Started"]),
        "{}",
        lines[0]
    );
    assert!(
        has(&lines[1], &[" GET /s/<session>/ host=", "authorization=present cookie=present session_path=yes binding_cookie=match -> 200 Ok"]),
        "{}",
        lines[1]
    );
    assert!(
        has(
            &lines[2],
            &[
                " GET /s/<session>/ host=",
                "session_path=yes binding_cookie=nomatch -> 403 SessionDenied"
            ]
        ),
        "{}",
        lines[2]
    );
    assert!(
        has(&lines[3], &[" GET /s/<session>/ host=", "authorization=present cookie=absent session_path=yes binding_cookie=absent -> 403 SessionDenied"]),
        "{}",
        lines[3]
    );
    assert!(
        has(
            &lines[4],
            &[
                " GET /s/<session>/../x host=",
                "session_path=yes binding_cookie=match -> 400 BadTarget"
            ]
        ),
        "{}",
        lines[4]
    );
    assert!(
        has(
            &lines[5],
            &[
                " GET /x/<session>/y host=",
                "session_path=no binding_cookie=match -> 403 SessionDenied"
            ]
        ),
        "{}",
        lines[5]
    );
    assert!(
        has(
            &lines[6],
            &[
                " GET /?t=<session> host=",
                "session_path=no binding_cookie=match -> 403 SessionDenied"
            ]
        ),
        "{}",
        lines[6]
    );
    assert!(
        has(&lines[7], &[" GET / host=", "authorization=absent cookie=absent session_path=no binding_cookie=absent -> 401 Unauthorized"]),
        "{}",
        lines[7]
    );
    assert!(
        has(
            &lines[8],
            &[
                " GET /?i=<session> host=",
                "session_path=no binding_cookie=match -> 403 SessionDenied"
            ]
        ),
        "{}",
        lines[8]
    );
}

#[test]
fn log_line_does_not_let_a_request_forge_extra_lines() {
    let (addr, logs) = start_logged();
    let raw = format!(
        "GET /a\u{1}b HTTP/1.1\r\nHost: {}\r\nOrigin: x\ny\r\n\r\n",
        host_of(addr)
    );
    let _ = send(addr, &raw);
    let lines = logs.lock().unwrap().clone();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(" GET /a?b host="), "{}", lines[0]);
    assert!(!lines[0].contains('\n'));
}

#[test]
fn every_response_carries_the_security_headers() {
    let (addr, s) = started_lab();
    let fresh = start_lab();
    let replies = vec![
        ("200", get_at(addr, &s.path(), &s.full())),
        ("303", get_at(fresh, "/", &good())),
        ("400", get_at(addr, &format!("{}../", s.path()), &[])),
        ("401", get_at(addr, "/", &[])),
        ("403", get_at(addr, "/", &good())),
        ("404", get_at(addr, &format!("{}nope", s.path()), &s.full())),
        ("malformed", send(addr, "GARBAGE\r\n\r\n")),
    ];
    for (label, r) in replies {
        assert_eq!(
            r.header("referrer-policy").as_deref(),
            Some("no-referrer"),
            "{label}"
        );
        assert_eq!(
            r.header("x-frame-options").as_deref(),
            Some("DENY"),
            "{label}"
        );
        assert_eq!(
            r.header("x-content-type-options").as_deref(),
            Some("nosniff"),
            "{label}"
        );
        assert_eq!(
            r.header("cross-origin-resource-policy").as_deref(),
            Some("same-origin"),
            "{label}"
        );
    }
}

// Recorder.

fn temp_log() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("lt6-recorder-{}-{n}.log", std::process::id()))
}

fn start_recorder(lab_port: u16) -> (SocketAddr, PathBuf) {
    start_recorder_with(lab_port, None)
}

fn start_recorder_with(lab_port: u16, probe: Option<&str>) -> (SocketAddr, PathBuf) {
    let log = temp_log();
    let _ = std::fs::remove_file(&log);
    let addr =
        recorder::start_with_probe(0, lab_port, log.clone(), probe.map(str::to_string)).unwrap();
    (addr, log)
}

/// The recorder writes the log after it has answered; poll the file briefly.
fn read_log_until(log: &PathBuf, needle: &str) -> String {
    for _ in 0..50 {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if text.contains(needle) {
            return text;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::read_to_string(log).unwrap_or_default()
}

fn plain_get(addr: SocketAddr, target: &str, extra: &[&str]) -> Reply {
    let extra: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
    get_at(addr, target, &extra)
}

#[test]
fn recorder_logs_an_authorization_header_verbatim() {
    let (addr, log) = start_recorder(1);
    let line = "Authorization: Basic bGFiOnMzY3JldC1sYXVuY2gtdmFsdWU=";
    let r = plain_get(addr, "/", &[line]);
    assert_eq!(r.status, 200);
    let text = read_log_until(&log, "END");
    assert!(text.contains(&format!("| {line}\n")), "{text}");
    assert!(text.contains("| GET / HTTP/1.1\n"), "{text}");
    assert!(
        text.contains(&format!("| Host: 127.0.0.1:{}\n", addr.port())),
        "{text}"
    );
    let _ = std::fs::remove_file(&log);
}

#[test]
fn recorder_log_shows_no_authorization_when_none_was_sent() {
    let (addr, log) = start_recorder(1);
    let r = plain_get(addr, "/", &["Cookie: a=b"]);
    assert_eq!(r.status, 200);
    let text = read_log_until(&log, "END");
    assert!(
        text.contains("| Cookie: a=b\n"),
        "request was recorded: {text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("authorization"),
        "{text}"
    );
    let _ = std::fs::remove_file(&log);
}

#[test]
fn recorder_page_has_cross_origin_post_form_and_no_probe_by_default() {
    let (addr, log) = start_recorder(4321);
    let r = plain_get(addr, "/", &[]);
    assert_eq!(r.status, 200);
    assert!(
        r.body
            .contains("<form method=\"post\" action=\"http://127.0.0.1:4321/submit\">"),
        "{}",
        r.body
    );
    assert!(!r.body.contains("<img"), "{}", r.body);
    assert!(!r.body.contains("/probe"), "{}", r.body);
    let _ = std::fs::remove_file(&log);
}

#[test]
fn recorder_probe_page_requests_the_token_path_on_its_own_origin() {
    let token = "0123456789abcdef0123456789abcdef";
    let (addr, log) = start_recorder_with(4321, Some(token));
    let r = plain_get(addr, "/", &[]);
    assert_eq!(r.status, 200);
    assert!(
        r.body
            .contains(&format!("<img src=\"/s/{token}/probe\" alt=\"probe\">")),
        "{}",
        r.body
    );
    assert!(!r.body.contains("http://127.0.0.1:4321/s/"), "{}", r.body);
    assert!(
        r.body
            .contains("<form method=\"post\" action=\"http://127.0.0.1:4321/submit\">"),
        "{}",
        r.body
    );
    let _ = std::fs::remove_file(&log);
}

#[test]
fn recorder_logs_the_cookie_of_a_probe_request_verbatim() {
    let token = "0123456789abcdef0123456789abcdef";
    let (addr, log) = start_recorder_with(1, Some(token));
    let cookie = "Cookie: rt_lab_1=aaaabbbbccccddddeeeeffff00001111";
    let r = plain_get(addr, &format!("/s/{token}/probe"), &[cookie]);
    assert_eq!(r.status, 404);
    let text = read_log_until(&log, "END");
    assert!(
        text.contains(&format!("| GET /s/{token}/probe HTTP/1.1\n")),
        "{text}"
    );
    assert!(text.contains(&format!("| {cookie}\n")), "{text}");
    let _ = std::fs::remove_file(&log);
}

#[test]
fn recorder_rejects_a_probe_token_that_is_not_32_lower_hex() {
    for bad in [
        "",
        "short",
        "0123456789ABCDEF0123456789ABCDEF",
        "0123456789abcdef0123456789abcde\"",
        "\"><script>alert(1)</script>0123456",
    ] {
        let err = recorder::start_with_probe(0, 1, temp_log(), Some(bad.to_string()))
            .err()
            .unwrap_or_else(|| panic!("accepted {bad:?}"));
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
    }
}

#[test]
fn cross_origin_post_with_cached_credentials_is_refused_by_origin() {
    // What a browser would send from the recorder page if it attached the
    // cached credentials: right credentials, Host and a guessed CSRF value.
    let (addr, s) = started_lab();
    let (rec, log) = start_recorder(addr.port());
    for target in [format!("{}submit", s.path()), "/submit".to_string()] {
        let extra = vec![
            basic(GOOD),
            format!("Origin: http://127.0.0.1:{}", rec.port()),
        ];
        let r = post_at(
            addr,
            &host_of(addr),
            &target,
            &extra,
            "csrf=guessed-by-the-page&note=cross-origin",
        );
        assert_eq!(r.status, 403, "{target}");
        assert_eq!(r.body, "forbidden: origin\n", "{target}");
    }
    let _ = std::fs::remove_file(&log);
}
