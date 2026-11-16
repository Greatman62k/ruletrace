//! A loopback HTTP/1.1 stub with hand-written responses. It records the
//! exact bytes of every request it receives.

#![allow(dead_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rt_ai_adapter::ProviderId;
use rt_ai_adapter::{
    AdapterInfo, AiAdapter, AttemptOutcome, AttemptReport, ProviderRequest, RequestLimits,
    StructuredOutput,
};
use rt_ai_responses::{Credential, ResponsesAdapter, Settings};
use rt_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub const CANARY: &str = "canary-Zq81xK4mP2-credential";
pub const SCHEMA: &str = r#"{"type":"object","additionalProperties":false,"required":["a"],"properties":{"a":{"type":"string"}}}"#;

/// What the stub does with one connection.
#[derive(Clone)]
pub enum Reply {
    /// Write these bytes, then close.
    Raw(Vec<u8>),
    /// Write the head and a part of the body, then wait, then close.
    Slow {
        head_and_part: Vec<u8>,
        pause: Duration,
    },
    /// Write these bytes at once, then read until the client closes (so no
    /// reset can race the client), then close.
    Eager(Vec<u8>),
    /// Close without writing.
    Close,
}

pub struct Captured {
    pub raw: Vec<u8>,
}

impl Captured {
    pub fn head(&self) -> String {
        let end = find(&self.raw, b"\r\n\r\n").unwrap_or(self.raw.len());
        String::from_utf8_lossy(&self.raw[..end]).into_owned()
    }
    pub fn body(&self) -> &[u8] {
        let start = find(&self.raw, b"\r\n\r\n").map_or(self.raw.len(), |i| i + 4);
        &self.raw[start..]
    }
    pub fn request_line(&self) -> String {
        self.head().lines().next().unwrap_or("").to_owned()
    }
    pub fn headers(&self) -> Vec<(String, String)> {
        self.head()
            .lines()
            .skip(1)
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect()
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub struct Stub {
    pub port: u16,
    captured: Arc<Mutex<Vec<Captured>>>,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl Stub {
    /// Serves `replies` in order, one per connection; further connections
    /// are answered with `Close`.
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let (c, s, n) = (captured.clone(), stop.clone(), connections.clone());
        let handle = std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        n.fetch_add(1, Ordering::SeqCst);
                        let reply = replies.next().unwrap_or(Reply::Close);
                        serve(stream, reply, &c);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            port,
            captured,
            stop,
            connections,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    pub fn finish(mut self) -> Vec<Captured> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            h.join().unwrap();
        }
        std::mem::take(&mut *self.captured.lock().unwrap())
    }
}

fn serve(mut stream: TcpStream, reply: Reply, out: &Mutex<Vec<Captured>>) {
    stream.set_nonblocking(false).unwrap();
    if let Reply::Eager(bytes) = &reply {
        let _ = stream.write_all(bytes);
        let _ = stream.flush();
        // Read until the client closes, so that closing never resets the
        // connection while client data is unread. A reset could otherwise
        // reach the client before it parses the bytes written above.
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
        }
        out.lock().unwrap().push(Captured { raw });
        return;
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if let Some(i) = find(&raw, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..i]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if raw.len() >= i + 4 + len {
                break;
            }
        }
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
        }
    }
    out.lock().unwrap().push(Captured { raw });
    match reply {
        Reply::Raw(bytes) => {
            let _ = stream.write_all(&bytes);
        }
        Reply::Slow {
            head_and_part,
            pause,
        } => {
            let _ = stream.write_all(&head_and_part);
            let _ = stream.flush();
            std::thread::sleep(pause);
        }
        Reply::Close | Reply::Eager(_) => {}
    }
}

/// A complete response with the given status line text, headers and body.
pub fn http(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Reply {
    let mut out = format!("HTTP/1.1 {status}\r\n").into_bytes();
    for (k, v) in headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(body);
    Reply::Raw(out)
}

pub fn json_ok(body: &str) -> Reply {
    http(
        "200 OK",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    )
}

pub fn message_envelope(text: &str, extra: &str) -> String {
    let escaped = rt_json::write_compact(&Value::string(text));
    format!(
        r#"{{"id":"resp_1","object":"response","status":"completed","model":"muse-reported","output":[{{"type":"reasoning","id":"rs_1","summary":[]}},{{"type":"message","id":"msg_1","status":"completed","role":"assistant","content":[{{"type":"output_text","text":{escaped},"annotations":[]}}]}}]{extra}}}"#
    )
}

pub fn request(effort: Option<&str>, response_bytes: u32) -> ProviderRequest {
    ProviderRequest {
        task: "propose_rules".to_owned(),
        model: "muse-spark-1.3".to_owned(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes,
        },
        reasoning_effort: effort.map(str::to_owned),
        context: vec![(
            "clauses".to_owned(),
            Value::array(vec![Value::string("Clause 1: a team scores 3 points.")]),
        )],
    }
}

pub fn adapter(endpoint: &str) -> ResponsesAdapter {
    ResponsesAdapter::new(
        Settings {
            provider: ProviderId::new("meta").unwrap(),
            endpoint: endpoint.to_owned(),
            structured_output: StructuredOutput {
                name: "ruletrace_proposals".to_owned(),
                schema: SCHEMA.as_bytes().to_vec(),
            },
            max_output_tokens: 8192,
        },
        Credential::new(CANARY.to_owned()).unwrap(),
    )
    .unwrap()
}

pub fn attempt(
    adapter: &mut ResponsesAdapter,
    req: &ProviderRequest,
    timeout: Duration,
) -> AttemptReport {
    adapter.attempt(req, timeout)
}

pub fn response_of(report: &AttemptReport) -> &rt_ai_adapter::ProviderResponse {
    match &report.outcome {
        AttemptOutcome::Response(r) => r,
        other => panic!("expected a response, got {other:?}"),
    }
}

pub fn failure_of(report: &AttemptReport) -> rt_ai_adapter::ProviderFailure {
    match &report.outcome {
        AttemptOutcome::Failure(f) => *f,
        other => panic!("expected a failure, got {other:?}"),
    }
}

pub fn identity_of(a: &ResponsesAdapter) -> rt_ai_adapter::Identity {
    a.identity()
}
