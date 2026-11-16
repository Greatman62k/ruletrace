//! Offline replay of the two live recordings (task 10.6, Req 33.8, 23.25).
//!
//! The committed `ai/meta-replay.toml` (prompt-2) and
//! `ai/meta-replay-prompt-3.toml` with their exported recordings are run
//! through the real `ruletrace-ai propose` path with the mock-replay
//! provider. The expected batch is the live batch itself (a read-only
//! fixture), so nothing here is derived from the replay output. The run gets
//! no environment value, no endpoint and no credential. A further test shows
//! that a loopback endpoint in the configuration is refused. Both recordings
//! carry the same Request_Identifier and are separate files, never merged.
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

// One recording with its configuration and the live batch it was exported
// from. The hash and adapter version are literals read from those batches.
#[derive(Clone, Copy)]
struct Rec {
    script: &'static str,
    config: &'static str,
    log: &'static str,
    fixture: &'static str,
    adapter_version: &'static str,
    response_sha256: &'static str,
    created_at: &'static str,
}

const P2: Rec = Rec {
    script: "provider/recorded-meta-muse-spark-1.3-prompt-2.json",
    config: "ai/meta-replay.toml",
    log: "work/replay/transmission.log.jsonl",
    fixture: "live-run1-prompt-2",
    adapter_version: "responses-1/prompt-2",
    response_sha256: "9585ce1dac804dc1c2ceebe5c2221bc67c70245abb7099d918d7afeb123f098e",
    created_at: "2026-11-16T05:13:25Z",
};

const P3: Rec = Rec {
    script: "provider/recorded-meta-muse-spark-1.3-prompt-3.json",
    config: "ai/meta-replay-prompt-3.toml",
    log: "work/replay-prompt-3/transmission.log.jsonl",
    fixture: "live-run2-prompt-3",
    adapter_version: "responses-1/prompt-3",
    response_sha256: "7f3371ca503a4f614dda9c4288e346a1822e9fde88546bc058ecba9294706c26",
    created_at: "2026-11-16T09:08:32Z",
};

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-ai")
}

fn live_batch(rec: Rec) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rt-ai-tool/tests/fixtures")
            .join(rec.fixture)
            .join("batch.json"),
    )
    .unwrap()
}

// The committed configuration with its two relative paths made absolute for
// the scratch directory; every other byte is the committed text.
fn config(work: &Work, rec: Rec) -> String {
    let text = std::fs::read_to_string(demo().join(rec.config)).unwrap();
    let script = format!("script = \"{}\"", rec.script);
    let log = format!("transmission_log = \"{}\"", rec.log);
    assert!(text.contains(&script) && text.contains(&log), "{text}");
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
    text.replace(&script, &format!("script = \"{}\"", path(rec.script)))
        .replace(
            &log,
            &format!("transmission_log = \"{}\"", path("work/tx.jsonl")),
        )
}

fn replay(name: &str, rec: Rec) -> (Work, support::Ran) {
    replay_script(name, rec, &|text| text, None)
}

// `endpoint` is a loopback URL added to the committed configuration, to test
// that the replay never uses an endpoint it is given.
fn replay_with(name: &str, rec: Rec, endpoint: &str) -> (Work, support::Ran) {
    let add = |text: String| {
        let edited = text.replacen(
            "[provider]\n",
            &format!("[provider]\nendpoint = \"{endpoint}\"\n"),
            1,
        );
        assert!(edited.contains(endpoint), "{edited}");
        edited
    };
    replay_script(name, rec, &add, None)
}

// `edit` changes the text of the scratch configuration. `script` replaces
// the bytes of the recording in the scratch directory.
fn replay_script(
    name: &str,
    rec: Rec,
    edit: &dyn Fn(String) -> String,
    script: Option<&str>,
) -> (Work, support::Ran) {
    let work = Work::new(name);
    let dir = demo();
    work.copy_from(
        &dir,
        "documents/stand-in-rules.md",
        "documents/stand-in-rules.md",
    );
    work.copy_from(&dir, "rulebook/seed.json", "seed.json");
    match script {
        Some(text) => work.write(rec.script, text.as_bytes()),
        None => work.copy_from(&dir, rec.script, rec.script),
    }
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
    let text = edit(config(&work, rec));
    work.write("meta-replay.toml", text.as_bytes());
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
        rec.created_at,
        "--consent",
        "stand-in-rules.md",
    ]);
    (work, ran)
}

fn propose_args(work: &Work, config: &str, created_at: &str) -> Vec<String> {
    [
        "propose",
        "--config",
        &work.arg(config),
        "--rulebook",
        &work.arg("seed.json"),
        "--source-model",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        created_at,
        "--consent",
        "stand-in-rules.md",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn run_propose(work: &Work, config: &str, created_at: &str) -> support::Ran {
    let args = propose_args(work, config, created_at);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    work.ruletrace_ai(&refs)
}

fn stored_batch(work: &Work) -> rt_proposals::Batch {
    let input = InputId::new("b").unwrap();
    parse_batch(&input, &work.read(&format!("store/batches/{LIVE_ID}.json"))).unwrap()
}

fn assert_replays_the_live_batch(name: &str, rec: Rec) {
    let (work, ran) = replay(name, rec);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stdout.contains("run summary: complete"),
        "{}",
        ran.stdout
    );
    assert_eq!(
        ran.codes(),
        support::stored_with_blocking_findings_codes(),
        "{}",
        ran.stderr
    );

    let file = format!("store/batches/{LIVE_ID}.json");
    assert!(work.exists(&file), "{:?}", work.state().keys());
    let input = InputId::new("b").unwrap();
    let replayed = parse_batch(&input, &work.read(&file)).unwrap();
    let live = parse_batch(&input, &live_batch(rec)).unwrap();
    assert_eq!(replayed.request_id, live.request_id);
    assert_eq!(replayed.response_sha256, live.response_sha256);
    assert_eq!(replayed.response_sha256.as_str(), rec.response_sha256);
    assert_eq!(replayed.response, live.response);
    assert_eq!(replayed.proposals, live.proposals);
    assert_eq!(replayed.model, live.model);
    assert_eq!(replayed.created_at, live.created_at);
    // The replay keeps the adapter version of the recording, though the
    // hosted adapter of this build reports a newer prompt profile.
    assert_eq!(replayed.adapter_version, rec.adapter_version);
    assert_eq!(live.adapter_version, rec.adapter_version);
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
fn the_prompt_2_replay_stores_the_proposals_and_payload_of_the_live_batch_unchanged() {
    assert_replays_the_live_batch("replay-live", P2);
}

#[test]
fn the_prompt_3_replay_stores_the_proposals_and_payload_of_the_live_batch_unchanged() {
    assert_replays_the_live_batch("replay-live-p3", P3);
}

// The two live batches share a Request_Identifier but are different data.
// A replay that mixed them up (wrong script, relabelled adapter version)
// would pass the checks above for one recording only.
#[test]
fn the_two_recordings_replay_to_different_payloads_and_adapter_versions() {
    let (w2, r2) = replay("replay-differ-2", P2);
    let (w3, r3) = replay("replay-differ-3", P3);
    assert_eq!((r2.code, r3.code), (0, 0), "{} {}", r2.stderr, r3.stderr);
    let (b2, b3) = (stored_batch(&w2), stored_batch(&w3));
    assert_eq!(b2.request_id, b3.request_id);
    assert_eq!(b2.adapter_version, "responses-1/prompt-2");
    assert_eq!(b3.adapter_version, "responses-1/prompt-3");
    assert_ne!(b2.response_sha256, b3.response_sha256);
    assert_ne!(b2.response.payload, b3.response.payload);
}

fn assert_endpoint_refused(name: &str, rec: Rec) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (work, ran) = replay_with(name, rec, &url);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert_eq!(ran.codes(), vec!["RT-AI-0002"], "{}", ran.stderr);
    assert!(
        ran.stderr
            .contains("field `provider.endpoint` must be absent for provider `mock-replay`"),
        "{}",
        ran.stderr
    );
    assert!(!work.exists(&format!("store/batches/{LIVE_ID}.json")));
    assert!(matches!(
        listener.accept(),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
    ));
}

// Proves: the configuration cannot point a mock-replay run at an endpoint.
// A loopback endpoint is refused with RT-AI-0002 before any provider is
// built, nothing is stored, and the listener sees no connection. It does not
// prove that no other network use exists; the rig passes no environment, so
// proxy variables cannot reach the run, and the no-route namespace is the
// remaining evidence.
#[test]
fn a_prompt_2_replay_configuration_naming_an_endpoint_is_refused_and_nothing_connects() {
    assert_endpoint_refused("replay-endpoint", P2);
}

#[test]
fn a_prompt_3_replay_configuration_naming_an_endpoint_is_refused_and_nothing_connects() {
    assert_endpoint_refused("replay-endpoint-p3", P3);
}

fn assert_log_has_no_document_text(name: &str, rec: Rec) {
    let (work, ran) = replay(name, rec);
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
fn the_prompt_2_replay_log_holds_one_entry_and_no_document_text() {
    assert_log_has_no_document_text("replay-log", P2);
}

#[test]
fn the_prompt_3_replay_log_holds_one_entry_and_no_document_text() {
    assert_log_has_no_document_text("replay-log-p3", P3);
}

fn batch_files(work: &Work) -> Vec<(String, Vec<u8>)> {
    work.state()
        .into_iter()
        .filter(|(k, _)| k.starts_with("store/"))
        .collect()
}

fn assert_second_replay_stores_nothing_new(name: &str, rec: Rec) {
    let (work, first) = replay(name, rec);
    assert_eq!(first.code, 0, "{}", first.stderr);
    let before = batch_files(&work);
    let again = run_propose(&work, "meta-replay.toml", rec.created_at);
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert_eq!(again.codes(), ["RT-AI-0301"], "{}", again.stderr);
    assert_eq!(batch_files(&work), before);
}

#[test]
fn a_second_prompt_2_replay_into_the_same_store_stores_nothing_new() {
    assert_second_replay_stores_nothing_new("replay-twice", P2);
}

#[test]
fn a_second_prompt_3_replay_into_the_same_store_stores_nothing_new() {
    assert_second_replay_stores_nothing_new("replay-twice-p3", P3);
}

// Req 28.15: no credential-looking string in either recording, its
// configuration or the notes (patterns of Req 28.4).
#[test]
fn the_recordings_and_their_configurations_hold_no_credential_looking_string() {
    for rel in [
        P2.script,
        P3.script,
        P2.config,
        P3.config,
        "provider/RECORDINGS.md",
    ] {
        let text = std::fs::read_to_string(demo().join(rel)).unwrap();
        for pattern in ["sk-", "sk_", "Bearer", "ghp_", "AKIA", "LLM_"] {
            assert!(!text.contains(pattern), "{rel}: {pattern}");
        }
    }
}

// The prompt-3 recording, the live batch and the configuration of prompt-3
// agree on every identifying member; the configuration names no other
// script than its own.
#[test]
fn each_configuration_names_only_its_own_recording() {
    for (own, other) in [(P2, P3), (P3, P2)] {
        let text = std::fs::read_to_string(demo().join(own.config)).unwrap();
        assert!(text.contains(&format!("script = \"{}\"", own.script)));
        assert!(!text.contains(other.script), "{}", own.config);
        assert!(!text.contains(other.log), "{}", own.config);
        assert_eq!(text.matches("script =").count(), 1, "{}", own.config);
    }
}

// Same-Request_Identifier recordings stay separate files. A single script
// holding both entries is refused by the loader, and a configuration
// cannot name two scripts: the key is a single string.
#[test]
fn one_script_holding_both_recordings_is_refused_and_nothing_is_stored() {
    let one = std::fs::read_to_string(demo().join(P2.script)).unwrap();
    let two = std::fs::read_to_string(demo().join(P3.script)).unwrap();
    let entry = |text: &str| {
        let start = text.find("\"entries\": [\n").unwrap() + "\"entries\": [\n".len();
        let end = text.rfind("\n  ]\n}").unwrap();
        text[start..end].to_owned()
    };
    let merged = one.replace(&entry(&one), &format!("{},\n{}", entry(&one), entry(&two)));
    assert_eq!(merged.matches("\"request_id\"").count(), 2);
    let (work, ran) = replay_script("replay-merged", P2, &|text| text, Some(&merged));
    assert_eq!(ran.code, 2, "{}", ran.stdout);
    assert_eq!(ran.codes(), vec!["RT-CLI-0004"], "{}", ran.stderr);
    assert!(
        ran.stderr
            .contains("(entries[1].request_id): repeated request_id"),
        "{}",
        ran.stderr
    );
    assert!(!work.exists(&format!("store/batches/{LIVE_ID}.json")));
}

// Replaces the `script` line of the scratch configuration by `lines`.
fn with_script_lines(text: &str, lines: &str) -> String {
    let edited: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with("script = ") {
                lines.to_owned()
            } else {
                l.to_owned()
            }
        })
        .collect();
    let edited = edited.join("\n") + "\n";
    assert_ne!(edited, text);
    edited
}

// A configuration holds one `script` string. A list of both recordings, or
// a second `script` key, is refused and nothing is stored.
#[test]
fn a_configuration_naming_both_recordings_is_refused_and_nothing_is_stored() {
    let list = |text: String| {
        let both = format!("script = [\"{}\", \"{}\"]", P2.script, P3.script);
        with_script_lines(&text, &both)
    };
    let (work, ran) = replay_script("replay-list", P2, &list, None);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert_eq!(ran.codes(), vec!["RT-AI-0002"], "{}", ran.stderr);
    assert!(
        ran.stderr.contains("field `script` must be a string"),
        "{}",
        ran.stderr
    );
    assert!(!work.exists(&format!("store/batches/{LIVE_ID}.json")));

    let twice = |text: String| {
        let both = format!("script = \"{}\"\nscript = \"{}\"", P2.script, P3.script);
        with_script_lines(&text, &both)
    };
    let (work, ran) = replay_script("replay-dup-key", P2, &twice, None);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert_eq!(ran.codes(), vec!["RT-AI-0002"], "{}", ran.stderr);
    assert!(
        ran.stderr.contains("invalid TOML: duplicate key"),
        "{}",
        ran.stderr
    );
    assert!(!work.exists(&format!("store/batches/{LIVE_ID}.json")));
}
