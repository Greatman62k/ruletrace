//! The adapter verifies the server certificate (Req 28.6).
//!
//! A loopback TLS server presents a self-signed certificate that no trust
//! store holds. The adapter must fail with `TlsFailed`, the server must
//! complete no handshake and read no application bytes, so the credential is
//! never sent. A positive control shows the same server works for a client
//! that trusts the fixture. The server listens on 127.0.0.1 only.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use rt_ai_adapter::ProviderFailure as F;
use rustls::pki_types::ServerName;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};
use std::io::{Read, Write};
use std::net::IpAddr;
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use support::{adapter, attempt, failure_of, request, CANARY};

const CERT: &[u8] = include_bytes!("fixtures/tls/self-signed-cert.der");
const KEY: &[u8] = include_bytes!("fixtures/tls/self-signed-key.pkcs8.der");

/// What the server saw of one connection.
#[derive(Debug)]
struct Seen {
    /// Application bytes decrypted by the server.
    application: Vec<u8>,
    /// Whether the handshake completed from the server's side.
    handshake_completed: bool,
}

fn tls_server() -> (u16, JoinHandle<Seen>) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(CERT.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut conn = ServerConnection::new(Arc::new(config)).unwrap();
        let mut seen = Seen {
            application: Vec::new(),
            handshake_completed: false,
        };
        // An aborted handshake ends in an error or end of stream; both end
        // the loop with nothing read.
        while let Ok((read, written)) = conn.complete_io(&mut tcp) {
            if !conn.is_handshaking() {
                seen.handshake_completed = true;
                let mut buf = [0u8; 64];
                if let Ok(n) = conn.reader().read(&mut buf) {
                    seen.application.extend_from_slice(&buf[..n]);
                }
                break;
            }
            if read == 0 && written == 0 {
                break;
            }
        }
        seen
    });
    (port, handle)
}

// Positive control: the same server and fixture complete a handshake with a
// client that trusts the fixture, so the failure below comes from trust and
// not from a broken server or certificate.
#[test]
fn the_fixture_server_completes_a_handshake_with_a_client_that_trusts_it() {
    let (port, server) = tls_server();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(CERT.to_vec())).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::IpAddress(IpAddr::from([127, 0, 0, 1]).into());
    let mut conn = rustls::ClientConnection::new(Arc::new(config), name).unwrap();
    let mut tcp = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    conn.writer().write_all(b"ping").unwrap();
    while conn.is_handshaking() || conn.wants_write() {
        conn.complete_io(&mut tcp).unwrap();
    }
    let seen = server.join().unwrap();
    assert!(seen.handshake_completed, "{seen:?}");
    assert_eq!(seen.application, b"ping".to_vec());
}

#[test]
fn an_unknown_issuer_is_tls_failed_and_no_request_byte_is_sent() {
    let (port, server) = tls_server();
    let mut a = adapter(&format!("https://127.0.0.1:{port}"));
    let report = attempt(&mut a, &request(None, 1 << 20), Duration::from_secs(5));
    assert_eq!(failure_of(&report), F::TlsFailed, "{report:?}");
    let seen = server.join().unwrap();
    assert!(!seen.handshake_completed, "{seen:?}");
    assert_eq!(seen.application, Vec::<u8>::new(), "{seen:?}");
    assert!(
        !String::from_utf8_lossy(&seen.application).contains(CANARY),
        "the credential reached the server"
    );
}
