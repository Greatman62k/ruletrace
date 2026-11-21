//! Minimal HTTP/1.1 request reader, response writer and thread-per-connection server.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// A parsed request. `raw_head` is the request line and headers as received.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub raw_head: String,
}

impl Request {
    /// All values of a header, matched case-insensitively by name.
    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The value of a header that appears exactly once; `None` if absent or repeated.
    pub fn single_header(&self, name: &str) -> Option<&str> {
        match self.header_values(name).as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }
}

/// A response written with `Content-Length` and `Connection: close`.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        Response {
            status,
            headers: vec![
                ("Content-Type".into(), content_type.into()),
                ("Cache-Control".into(), "no-store".into()),
                ("Referrer-Policy".into(), "no-referrer".into()),
                ("X-Frame-Options".into(), "DENY".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
                ("Cross-Origin-Resource-Policy".into(), "same-origin".into()),
            ],
            body: body.into(),
        }
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self::new(status, "text/plain; charset=utf-8", body)
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            303 => "See Other",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            _ => "Status",
        }
    }

    fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, self.reason());
        for (n, v) in &self.headers {
            head.push_str(&format!("{n}: {v}\r\n"));
        }
        head.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.body.len()
        ));
        out.write_all(head.as_bytes())?;
        out.write_all(self.body.as_bytes())?;
        out.flush()
    }
}

/// Why a request could not be read; each maps to a response status.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    Malformed,
    TooLarge,
    Io,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Reads one request: head up to 16 KiB, body up to 4 KiB by `Content-Length`.
pub fn read_request(stream: &mut impl Read) -> Result<Request, ReadError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(i) = find(&buf, b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD {
            return Err(ReadError::TooLarge);
        }
        let n = stream.read(&mut chunk).map_err(|_| ReadError::Io)?;
        if n == 0 {
            return Err(ReadError::Malformed);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let raw_head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = raw_head.split("\r\n");
    let request_line = lines.next().ok_or(ReadError::Malformed)?;
    let mut parts = request_line.split(' ');
    let (method, target, version) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v), None) if v.starts_with("HTTP/1.") && !m.is_empty() => (m, t, v),
        _ => return Err(ReadError::Malformed),
    };
    let _ = version;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(ReadError::Malformed)?;
        if name.is_empty() || name.contains(' ') {
            return Err(ReadError::Malformed);
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    let mut req = Request {
        method: method.to_string(),
        target: target.to_string(),
        headers,
        body: Vec::new(),
        raw_head,
    };
    let length = match req.header_values("Content-Length").as_slice() {
        [] => 0,
        [one] => one.parse::<usize>().map_err(|_| ReadError::Malformed)?,
        _ => return Err(ReadError::Malformed),
    };
    if length > MAX_BODY {
        return Err(ReadError::TooLarge);
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).map_err(|_| ReadError::Io)?;
        if n == 0 {
            return Err(ReadError::Malformed);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    req.body = body;
    Ok(req)
}

/// A request handler: request and peer address in, response out.
pub type Handler = Arc<dyn Fn(&Request, SocketAddr) -> Response + Send + Sync>;

fn serve_connection(mut stream: TcpStream, peer: SocketAddr, handler: &Handler) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(req) => handler(&req, peer),
        Err(ReadError::TooLarge) => Response::text(413, "too large\n"),
        Err(ReadError::Malformed) => Response::text(400, "bad request\n"),
        Err(ReadError::Io) => return,
    };
    let _ = response.write_to(&mut stream);
}

/// Binds `127.0.0.1:port`; port 0 chooses a free port.
pub fn bind_local(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Serves on a background thread, one thread per connection.
pub fn spawn_server(listener: TcpListener, handler: Handler) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let Ok(peer) = stream.peer_addr() else {
                continue;
            };
            let handler = Arc::clone(&handler);
            thread::spawn(move || serve_connection(stream, peer, &handler));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Request, ReadError> {
        read_request(&mut raw.as_bytes())
    }

    #[test]
    fn parses_headers_and_body() {
        let r =
            parse("POST /submit HTTP/1.1\r\nHost: a\r\ncontent-length: 3\r\nX: y z\r\n\r\nabcdef")
                .unwrap();
        assert_eq!((r.method.as_str(), r.target.as_str()), ("POST", "/submit"));
        assert_eq!(r.single_header("HOST"), Some("a"));
        assert_eq!(r.single_header("x"), Some("y z"));
        assert_eq!(r.body, b"abc");
    }

    #[test]
    fn repeated_header_is_not_single() {
        let r = parse("GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n").unwrap();
        assert_eq!(r.single_header("host"), None);
        assert_eq!(r.header_values("host"), vec!["a", "b"]);
    }

    #[test]
    fn rejects_malformed_and_oversized() {
        assert_eq!(parse("GET /\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(
            parse("GET / HTTP/1.1\r\nbad\r\n\r\n").unwrap_err(),
            ReadError::Malformed
        );
        assert_eq!(
            parse("GET / HTTP/1.1\r\nHost: a\r\n").unwrap_err(),
            ReadError::Malformed
        );
        assert_eq!(
            parse("POST / HTTP/1.1\r\nContent-Length: 4097\r\n\r\n").unwrap_err(),
            ReadError::TooLarge
        );
        assert_eq!(
            parse("POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nabc").unwrap_err(),
            ReadError::Malformed
        );
    }
}
