//! Offline replay of the first live recording (task 10.6, Req 33.8, 23.25).
//!
//! The committed `ai/meta-replay.toml` and the exported recording are run
//! through the real `ruletrace-ai propose` path with the mock-replay
//! provider. The expected batch is the live batch itself (a read-only
//! fixture), so nothing here is derived from the replay output. The run gets
//! no environment value, no endpoint and no credential. A loopback listener
//! that the test binds must see no connection.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use rt_foundation::InputId;
use rt_proposals::parse_batch;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use support::Work;

const LIVE_ID: &str = "1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2";
const RECORDING: &str = "provider/recorded-meta-muse-spark-1.3-prompt-2.json";

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-ai")
}

fn live_batch() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rt-ai-tool/tests/fixtures/live-run1-prompt-2/batch.json"),
    )
    .unwrap()
}

// The committed configuration with its two relative paths made absolute for
// the scratch directory; every other byte is the committed text.
fn config(work: &Work) -> String {
    let text = std::fs::read_to_string(demo().join("ai/meta-replay.toml")).unwrap();
    let script = format!("script = \"{RECORDING}\"");
    let log = "transmission_log = \"work/replay/transmission.log.jsonl\"";
    assert!(text.contains(&script) && text.contains(log), "{text}");
    let settings: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    for forbidden in ["endpoint", "credential_env_var", "MODEL_API_KEY"] {
        assert!(
            settings.iter().all(|l| !l.contains(forbidden)),
            "{forbidden}"
        );
    }
    let path = |rel: &str| work.arg(rel).replace('\\', "/");
    text.replace(&script, &format!("script = \"{}\"", path(RECORDING)))
        .replace(
            log,
            &format!("transmission_log = \"{}\"", path("work/tx.jsonl")),
        )
}

fn replay(name: &str) -> (Work, support::Ran) {
    let work = Work::new(name);
    let dir = demo();
    work.copy_from(
        &dir,
        "documents/stand-in-rules.md",
        "documents/stand-in-rules.md",
    );
    work.copy_from(&dir, "rulebook/seed.json", "seed.json");
    work.copy_from(&dir, RECORDING, RECORDING);
    std::fs::create_dir_all(work.path("work")).unwrap();
    let ingest = work.ruletrace(&[
        "ingest",
        "--document",
        &work.arg("documents/stand-in-rules.md"),
        "--document-id",
        "stand-in-rules.md",
        "--out",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
    ]);
    assert_eq!(ingest.code, 0, "{}", ingest.stderr);
    work.write("meta-replay.toml", config(&work).as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(work.path("meta-replay.toml"), private).unwrap();
    }
    let ran = work.ruletrace_ai(&[
        "propose",
        "--config",
        &work.arg("meta-replay.toml"),
        "--rulebook",
        &work.arg("seed.json"),
        "--source-model",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        "2026-11-16T05:13:25Z",
        "--consent",
        "stand-in-rules.md",
    ]);
    (work, ran)
}

#[test]
fn the_replay_stores_the_proposals_and_payload_of_the_live_batch_unchanged() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let (work, ran) = replay("replay-live");
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stdout.contains("run summary: complete"),
        "{}",
        ran.stdout
    );
    assert_eq!(ran.codes(), ["RT-AI-0305"], "{}", ran.stderr);
    // No connection reached the listener during the run.
    assert!(matches!(
        listener.accept(),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
    ));

    let file = format!("store/batches/{LIVE_ID}.json");
    assert!(work.exists(&file), "{:?}", work.state().keys());
    let input = InputId::new("b").unwrap();
    let replayed = parse_batch(&input, &work.read(&file)).unwrap();
    let live = parse_batch(&input, &live_batch()).unwrap();
    assert_eq!(replayed.request_id, live.request_id);
    assert_eq!(replayed.response_sha256, live.response_sha256);
    assert_eq!(replayed.response, live.response);
    assert_eq!(replayed.proposals, live.proposals);
    assert_eq!(replayed.model, live.model);
    assert_eq!(replayed.created_at, live.created_at);
    // The prompt-2 recording replays as prompt-2 data, though the hosted
    // adapter of this build reports prompt-3.
    assert_eq!(replayed.adapter_version, "responses-1/prompt-2");
    assert_eq!(live.adapter_version, "responses-1/prompt-2");
    // What differs is what the replay is: no provider metadata is invented.
    assert_eq!(replayed.provider, "mock-replay");
    let rt_proposals::Format::V2(extras) = &replayed.format else {
        panic!("a new batch is version 2");
    };
    assert_eq!(extras.protocol.as_deref(), Some("mock-replay"));
    assert_eq!(extras.reported_model, None);
    assert_eq!(extras.response_id, None);
    assert_eq!(extras.usage, None);
}

#[test]
fn the_replay_log_holds_one_entry_and_no_document_text() {
    let (work, ran) = replay("replay-log");
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let log = String::from_utf8(work.read("work/tx.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(LIVE_ID), "{log}");
    assert!(log.contains(r#""endpoint_host":"""#), "{log}");
    let document = std::fs::read_to_string(demo().join("documents/stand-in-rules.md")).unwrap();
    for line in document.lines().filter(|l| l.len() > 8) {
        assert!(!log.contains(line), "document text in the log: {line}");
    }
    for word in ["entrant", "licence", "prize", "junior"] {
        assert!(!log.contains(word), "{word}");
    }
}

#[test]
fn a_second_replay_into_the_same_store_stores_nothing_new() {
    let (work, first) = replay("replay-twice");
    assert_eq!(first.code, 0, "{}", first.stderr);
    let before = work.state();
    let again = work.ruletrace_ai(&[
        "propose",
        "--config",
        &work.arg("meta-replay.toml"),
        "--rulebook",
        &work.arg("seed.json"),
        "--source-model",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        "2026-11-16T05:13:25Z",
        "--consent",
        "stand-in-rules.md",
    ]);
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert_eq!(again.codes(), ["RT-AI-0301"], "{}", again.stderr);
    let after = work.state();
    let batches = |s: &std::collections::BTreeMap<String, Vec<u8>>| {
        s.iter()
            .filter(|(k, _)| k.starts_with("store/"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(batches(&after), batches(&before));
}

// Req 28.15: no credential-looking string in the recording, its
// configuration or its notes (patterns of Req 28.4).
#[test]
fn the_recording_and_its_configuration_hold_no_credential_looking_string() {
    for rel in [RECORDING, "ai/meta-replay.toml", "provider/RECORDINGS.md"] {
        let text = std::fs::read_to_string(demo().join(rel)).unwrap();
        for pattern in ["sk-", "sk_", "Bearer", "ghp_", "AKIA", "LLM_"] {
            assert!(!text.contains(pattern), "{rel}: {pattern}");
        }
    }
}
