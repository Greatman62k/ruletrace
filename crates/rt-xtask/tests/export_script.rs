//! `rt-xtask export-script` over real store directories (task 10.6, Req
//! 33.8, 28.15, UD-36). Expected bytes are committed, reviewed files: the
//! live batch of the first Meta call and the recording exported from it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rt_foundation::InputId;
use rt_proposals::{parse_batch, print_batch};
use rt_xtask::export::{parse_args, run, ExportArgs};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LIVE_ID: &str = "1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2";
const CAPTURED: &str = "Meta Model API muse-spark-1.3, reasoning_effort low, captured 2026-11-16 by the user, synthetic stand-in document";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn live_batch() -> Vec<u8> {
    std::fs::read(repo().join("crates/rt-ai-tool/tests/fixtures/live-run1-prompt-2/batch.json"))
        .unwrap()
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("rt-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("store/batches")).unwrap();
        Self(path)
    }

    fn batch(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join("store/batches").join(name), bytes).unwrap();
    }

    // Every file below the directory with its bytes.
    fn state(&self) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for item in std::fs::read_dir(dir).unwrap() {
                let path = item.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    out.insert(rel, std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.0, &self.0, &mut out);
        out
    }

    fn args(&self, out: &str, extra: &[(&str, &str)]) -> ExportArgs {
        let mut given: Vec<(&str, String)> = vec![
            ("--store", self.0.join("store").display().to_string()),
            ("--provider", "openai-responses".to_owned()),
            ("--provider-id", "meta".to_owned()),
            ("--model", "muse-spark-1.3".to_owned()),
            ("--captured", CAPTURED.to_owned()),
            ("--out", self.0.join(out).display().to_string()),
        ];
        for (name, value) in extra {
            given.retain(|(n, _)| n != name);
            if !value.is_empty() {
                given.push((name, (*value).to_owned()));
            }
        }
        let flat: Vec<String> = given
            .into_iter()
            .flat_map(|(n, v)| [n.to_owned(), v])
            .collect();
        parse_args(&flat).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_live_store_exports_to_the_committed_recording_byte_for_byte() {
    let dir = Dir::new("live");
    dir.batch(&format!("{LIVE_ID}.json"), &live_batch());
    run(&dir.args("script.json", &[])).unwrap();
    let got = std::fs::read(dir.0.join("script.json")).unwrap();
    let committed = std::fs::read(
        repo().join("demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json"),
    )
    .unwrap();
    assert_eq!(got, committed);
    let text = String::from_utf8(got).unwrap();
    assert!(text.contains(&format!("\"captured\": \"{CAPTURED}\"")));
    assert!(text.contains("\"adapter_version\": \"responses-1/prompt-2\""));
    assert!(text.contains("\"origin\": \"recorded\""));
    // The live response identifier and usage are not members of the format.
    assert!(!text.contains("resp_6afa910e4a3898da13064e79"));
    assert!(!text.contains("total_tokens"));
}

#[test]
fn equal_stores_give_equal_bytes_and_only_the_output_file_is_written() {
    let one = Dir::new("det-1");
    let two = Dir::new("det-2");
    for d in [&one, &two] {
        d.batch(&format!("{LIVE_ID}.json"), &live_batch());
    }
    let before = one.state();
    run(&one.args("script.json", &[])).unwrap();
    run(&two.args("script.json", &[])).unwrap();
    assert_eq!(
        std::fs::read(one.0.join("script.json")).unwrap(),
        std::fs::read(two.0.join("script.json")).unwrap()
    );
    let mut after = one.state();
    assert!(after.remove("script.json").is_some());
    // No ledger, Rulebook, compiled or result file appears, and the store is
    // unchanged.
    assert_eq!(after, before);
}

#[test]
fn a_tampered_payload_fails_naming_the_file_and_the_request_and_writes_nothing() {
    let dir = Dir::new("tamper");
    let text = String::from_utf8(live_batch()).unwrap();
    let tampered = text.replacen(
        "Clause defines eligibility",
        "Clause defines eligibilitY",
        1,
    );
    assert_ne!(text, tampered);
    dir.batch(&format!("{LIVE_ID}.json"), tampered.as_bytes());
    let before = dir.state();
    let err = run(&dir.args("script.json", &[])).unwrap_err();
    assert!(err.contains(&format!("batches/{LIVE_ID}.json")), "{err}");
    assert!(err.contains(&format!("request {LIVE_ID}")), "{err}");
    assert!(err.contains("RT-AI-0302"), "{err}");
    assert_eq!(dir.state(), before);
}

#[test]
fn a_credential_looking_payload_is_refused_and_no_script_is_written() {
    let canary = "sk-canary9f3e2d1c0b";
    let mut batch = parse_batch(&InputId::new("b").unwrap(), &live_batch()).unwrap();
    batch.response.payload = batch.response.payload.replacen(
        "Clause defines eligibility",
        &format!("{canary} eligibility"),
        1,
    );
    batch.response_sha256 = rt_hash::sha256(batch.response.payload.as_bytes());
    let dir = Dir::new("canary");
    dir.batch(&format!("{LIVE_ID}.json"), print_batch(&batch).as_bytes());
    let before = dir.state();
    let err = run(&dir.args("script.json", &[])).unwrap_err();
    assert!(err.contains("credential-looking"), "{err}");
    assert!(err.contains(LIVE_ID), "{err}");
    assert!(!err.contains(canary), "{err}");
    assert_eq!(dir.state(), before);
}

#[test]
fn an_existing_output_file_is_never_overwritten() {
    let dir = Dir::new("exists");
    dir.batch(&format!("{LIVE_ID}.json"), &live_batch());
    std::fs::write(dir.0.join("script.json"), b"keep").unwrap();
    let err = run(&dir.args("script.json", &[])).unwrap_err();
    assert!(err.contains("cannot be created"), "{err}");
    assert_eq!(std::fs::read(dir.0.join("script.json")).unwrap(), b"keep");
}

#[test]
fn a_version_1_store_is_read_and_its_replayed_batches_are_not_exported_as_recorded() {
    let dir = Dir::new("v1");
    let v1 = repo().join("tests/fixtures/proposal-store-v1");
    for item in std::fs::read_dir(&v1).unwrap() {
        let path = item.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            dir.batch(&name, &std::fs::read(&path).unwrap());
        }
    }
    let read = rt_xtask::export::read_store(&dir.0.join("store")).unwrap();
    assert_eq!(read.len(), 2);
    assert!(read.iter().all(|b| b.batch.provider == "mock-replay"));
    let args = dir.args(
        "script.json",
        &[
            ("--provider", "mock-replay"),
            ("--provider-id", ""),
            ("--model", "authored-script"),
        ],
    );
    let err = run(&args).unwrap_err();
    assert!(err.contains("offline replay"), "{err}");
    assert!(!dir.0.join("script.json").exists());
}

#[test]
fn a_file_that_is_not_a_batch_name_is_refused_by_name() {
    let dir = Dir::new("name");
    dir.batch("notes.txt", b"x");
    let err = run(&dir.args("script.json", &[])).unwrap_err();
    assert!(err.contains("batches/notes.txt"), "{err}");
}

#[test]
fn every_required_option_is_checked_and_an_unknown_one_is_refused() {
    let base = [
        "--store",
        "s",
        "--provider",
        "p",
        "--model",
        "m",
        "--captured",
        "c",
        "--out",
        "o",
    ];
    let full: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
    assert!(parse_args(&full).is_ok());
    for skip in ["--store", "--provider", "--model", "--captured", "--out"] {
        let at = base.iter().position(|a| *a == skip).unwrap();
        let mut cut = full.clone();
        cut.drain(at..at + 2);
        let err = parse_args(&cut).unwrap_err();
        assert!(err.contains(&format!("{skip} is required")), "{err}");
    }
    let mut extra = full.clone();
    extra.extend(["--key".to_owned(), "v".to_owned()]);
    assert!(parse_args(&extra).unwrap_err().contains("unknown argument"));
    let mut twice = full.clone();
    twice.extend(["--model".to_owned(), "x".to_owned()]);
    assert!(parse_args(&twice).unwrap_err().contains("given twice"));
}
