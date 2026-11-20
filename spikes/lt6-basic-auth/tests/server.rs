//! Server-side evidence for LT-6 option A (design §11.18.5, evidence item 1).
//!
//! Real sockets on 127.0.0.1:0 and hand-written HTTP requests. Expected
//! statuses come from the design text, not from the server code. Base64
//! credentials are literals produced with the coreutils `base64` tool:
//! `lab:s3cret-launch-value`, and the wrong variants below.

use lt6_basic_auth::{lab, recorder};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const SECRET: &str = "s3cret-launch-value";
const CSRF: &str = "0123456789abcdef0123456789abcdef";
const GOOD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdWU=";
const WRONG_PASSWORD: &str = "bGFiOndyb25nLXBhc3N3b3Jk";
const WRONG_USER: &str = "b3RoZXI6czNjcmV0LWxhdW5jaC12YWx1ZQ==";
const SHORT_PASSWORD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdQ==";
const LONG_PASSWORD: &str = "bGFiOnMzY3JldC1sYXVuY2gtdmFsdWVl";

struct Reply {
    status: u16,
    head: String,
    body: String,
}

fn start_lab() -> SocketAddr {
    lab::start(SECRET.into(), CSRF.into(), 0, false).unwrap()
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

fn get(addr: SocketAddr, extra: &[&str]) -> Reply {
    let mut raw = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n", addr.port());
    for h in extra {
        raw.push_str(h);
        raw.push_str("\r\n");
    }
    raw.push_str("\r\n");
    send(addr, &raw)
}

fn basic(credentials: &str) -> String {
    format!("Authorization: Basic {credentials}")
}

/// POST /submit with explicit Host, extra headers and a form body.
fn post(addr: SocketAddr, host: &str, extra: &[String], body: &str) -> Reply {
    let mut raw = format!("POST /submit HTTP/1.1\r\nHost: {host}\r\n");
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

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

fn origin_of(addr: SocketAddr) -> String {
    format!("Origin: http://127.0.0.1:{}", addr.port())
}

/// Reads the CSRF token out of the served form, as a browser user would.
fn token_from_page(addr: SocketAddr) -> String {
    let page = get(addr, &[&basic(GOOD)]);
    assert_eq!(page.status, 200);
    let marker = "name=\"csrf\" value=\"";
    let start = page.body.find(marker).unwrap() + marker.len();
    let end = start + page.body[start..].find('"').unwrap();
    page.body[start..end].to_string()
}

#[test]
fn no_authorization_is_401_with_basic_challenge() {
    let addr = start_lab();
    let r = get(addr, &[]);
    assert_eq!(r.status, 401);
    let challenge = r
        .head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("www-authenticate:"))
        .unwrap();
    assert!(challenge.contains("Basic realm=\""), "{challenge}");
}

#[test]
fn former_session_cookie_without_authorization_is_401() {
    let addr = start_lab();
    let r = get(addr, &["Cookie: rt_lab_session=0123456789abcdef; other=1"]);
    assert_eq!(r.status, 401);
    assert!(!r.head.to_ascii_lowercase().contains("set-cookie"));
}

#[test]
fn cookie_does_not_substitute_for_wrong_credentials() {
    let addr = start_lab();
    let r = get(
        addr,
        &["Cookie: rt_lab_session=abc", &basic(WRONG_PASSWORD)],
    );
    assert_eq!(r.status, 401);
}

#[test]
fn wrong_credentials_are_401() {
    let addr = start_lab();
    for (label, creds) in [
        ("wrong password", WRONG_PASSWORD),
        ("wrong user", WRONG_USER),
        ("password one char short", SHORT_PASSWORD),
        ("password one char long", LONG_PASSWORD),
    ] {
        let r = get(addr, &[&basic(creds)]);
        assert_eq!(r.status, 401, "{label}");
    }
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
        assert_eq!(get(addr, &[header]).status, 401, "{header}");
    }
}

#[test]
fn right_credentials_get_200_and_no_cookie_is_set() {
    let addr = start_lab();
    let r = get(addr, &[&basic(GOOD)]);
    assert_eq!(r.status, 200);
    assert!(r.body.contains("<form method=\"post\" action=\"/submit\">"));
    assert!(!r.head.to_ascii_lowercase().contains("set-cookie"));
}

#[test]
fn right_credentials_and_wrong_host_are_403() {
    let addr = start_lab();
    for host in ["localhost:1", "127.0.0.1:1", "127.0.0.1", "evil.example"] {
        let raw = format!("GET / HTTP/1.1\r\nHost: {host}\r\n{}\r\n\r\n", basic(GOOD));
        assert_eq!(send(addr, &raw).status, 403, "{host}");
    }
    let raw = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nHost: evil.example\r\n{}\r\n\r\n",
        host_of(addr),
        basic(GOOD)
    );
    assert_eq!(send(addr, &raw).status, 403, "repeated Host");
}

#[test]
fn post_with_wrong_or_missing_origin_is_403() {
    let addr = start_lab();
    let token = token_from_page(addr);
    let body = format!("csrf={token}&note=x");
    let host = host_of(addr);
    let cases: [(&str, Option<String>); 4] = [
        ("other port", Some("Origin: http://127.0.0.1:1".to_string())),
        ("other host", Some("Origin: http://localhost".to_string())),
        ("null", Some("Origin: null".to_string())),
        ("missing", None),
    ];
    for (label, origin) in cases {
        let mut extra = vec![basic(GOOD)];
        extra.extend(origin);
        assert_eq!(post(addr, &host, &extra, &body).status, 403, "{label}");
    }
}

#[test]
fn post_with_wrong_or_missing_csrf_is_403() {
    let addr = start_lab();
    let token = token_from_page(addr);
    let mut altered = token.clone();
    altered.pop();
    altered.push(if token.ends_with('0') { '1' } else { '0' });
    let extra = vec![basic(GOOD), origin_of(addr)];
    let host = host_of(addr);
    for (label, body) in [
        ("altered token", format!("csrf={altered}&note=x")),
        (
            "truncated token",
            format!("csrf={}&note=x", &token[..token.len() - 1]),
        ),
        ("empty token", "csrf=&note=x".to_string()),
        ("missing field", "note=x".to_string()),
        ("name only in value", format!("note=csrf={token}")),
    ] {
        assert_eq!(post(addr, &host, &extra, &body).status, 403, "{label}");
    }
}

#[test]
fn post_with_host_origin_csrf_and_credentials_is_200() {
    let addr = start_lab();
    let token = token_from_page(addr);
    let extra = vec![basic(GOOD), origin_of(addr)];
    let r = post(
        addr,
        &host_of(addr),
        &extra,
        &format!("csrf={token}&note=x"),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.body, "accepted\n");
}

#[test]
fn post_without_credentials_is_401_even_with_valid_origin_and_csrf() {
    let addr = start_lab();
    let token = token_from_page(addr);
    let extra = vec![origin_of(addr)];
    let r = post(addr, &host_of(addr), &extra, &format!("csrf={token}"));
    assert_eq!(r.status, 401);
}

fn temp_log() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("lt6-recorder-{}-{n}.log", std::process::id()))
}

fn start_recorder(lab_port: u16) -> (SocketAddr, PathBuf) {
    let log = temp_log();
    let _ = std::fs::remove_file(&log);
    (recorder::start(0, lab_port, log.clone()).unwrap(), log)
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

#[test]
fn recorder_logs_an_authorization_header_verbatim() {
    let (addr, log) = start_recorder(1);
    let line = "Authorization: Basic bGFiOnMzY3JldC1sYXVuY2gtdmFsdWU=";
    let r = get(addr, &[line]);
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
    let r = get(addr, &["Cookie: a=b"]);
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
fn recorder_page_has_cross_origin_post_form_to_the_laboratory() {
    let (addr, log) = start_recorder(4321);
    let r = get(addr, &[]);
    assert_eq!(r.status, 200);
    assert!(
        r.body
            .contains("<form method=\"post\" action=\"http://127.0.0.1:4321/submit\">"),
        "{}",
        r.body
    );
    let _ = std::fs::remove_file(&log);
}

#[test]
fn cross_origin_post_with_cached_credentials_is_refused_by_origin() {
    // What a browser would send from the recorder page if it attached the
    // cached credentials: right credentials, Host and a guessed CSRF value.
    let addr = start_lab();
    let (rec, log) = start_recorder(addr.port());
    let extra = vec![
        basic(GOOD),
        format!("Origin: http://127.0.0.1:{}", rec.port()),
    ];
    let r = post(
        addr,
        &host_of(addr),
        &extra,
        "csrf=guessed-by-the-page&note=cross-origin",
    );
    assert_eq!(r.status, 403);
    assert_eq!(r.body, "forbidden: origin\n");
    let _ = std::fs::remove_file(&log);
}
