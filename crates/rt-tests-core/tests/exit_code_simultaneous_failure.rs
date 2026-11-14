//! Requirement 13.24: simultaneous Indeterminate execution and provenance
//! failure.
//!
//! Through the real `rt_cli::run` over an in-memory file system: a run with
//! an Indeterminate Outcome and a provenance Error exits 4, a run whose
//! Outcomes are all Decided and whose provenance fails exits 2. Both write
//! complete results with the affected traces marked not verified.
//!
//! **Validates: Requirements 13.18, 13.24, 16.17**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use rt_cli::{run, Fs};
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_srcmodel::{DocumentKind, SourceModel};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// An in-memory file system whose `create_new` can be made to fail.
#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    fail_create: Cell<bool>,
}

impl MemFs {
    fn put(&self, name: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(name), bytes.to_vec());
    }
}

struct NoLock;

impl rt_cli::io::LockHeld for NoLock {}

impl Fs for MemFs {
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        Ok(Box::new(NoLock))
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        if self.exists(path)? {
            Ok(Box::new(NoLock))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes
            .iter()
            .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
            .copied()
            .collect())
    }
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if self.fail_create.get() {
            return Err(io::Error::other("injected create failure"));
        }
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        let bytes = files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        files.insert(to.to_path_buf(), bytes);
        Ok(())
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.6 times the judges score.\n";
const CLAUSE: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const AGE: &str = r#"{"node":"field","t":"number","name":"age"}"#;
const DATA_INDETERMINATE: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17"},"collections":{}},{"id":"p2","fields":{},"collections":{}}]}"#;
const DATA_COMPLETE: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17"},"collections":{}},{"id":"p2","fields":{"age":"20"},"collections":{}}]}"#;

fn rulebook_text() -> String {
    let hash = sha256(TEXT.as_bytes());
    let start = TEXT.find(CLAUSE).expect("clause");
    let end = start + CLAUSE.len();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"d.a","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"a","type":"number","expr":{AGE},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

fn model_text() -> String {
    SourceModel::new(
        DocumentId::new("rules.md").expect("id"),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .expect("model")
    .print()
}

fn approved_ledger(rulebook: &str) -> String {
    let parsed = rt_ir::parse_rulebook(
        rulebook.as_bytes(),
        &InputId::new("rb.json").expect("id"),
        1 << 22,
        64,
    )
    .expect("rulebook")
    .into_parts()
    .0;
    let content =
        rt_ir::rule_content_hash(&parsed.ir_version, &parsed.revisions[0], &parsed.documents)
            .expect("hash");
    let doc = sha256(TEXT.as_bytes());
    let base = format!(
        r#"{{"seq":1,"kind":"approval","rule":"d.a","revision":1,"content_hash":"{content}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{ZERO}""#
    );
    let hash = sha256(format!("{base}}}").as_bytes());
    format!("{}\n{base},\"hash\":\"{hash}\"}}\n", rt_ledger::HEADER_LINE)
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

const VALIDATE: [&str; 9] = [
    "validate",
    "--rulebook",
    "rb.json",
    "--ledger",
    "ledger.jsonl",
    "--source-model",
    "model.json",
    "--out",
    "validation.json",
];
const COMPILE: [&str; 5] = [
    "compile",
    "--validation",
    "validation.json",
    "--out",
    "compiled.json",
];
const EVALUATE: [&str; 9] = [
    "evaluate",
    "--compiled",
    "compiled.json",
    "--data",
    "data.json",
    "--source-model",
    "model.json",
    "--out",
    "results.json",
];
const TEXT_ALTERED: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.7 times the judges score.\n";

fn prepared(data: &str, altered_source: bool) -> MemFs {
    let rulebook = rulebook_text();
    let fs = MemFs::default();
    fs.put("rb.json", rulebook.as_bytes());
    fs.put("model.json", model_text().as_bytes());
    fs.put("ledger.jsonl", approved_ledger(&rulebook).as_bytes());
    assert_eq!(run(&os(&VALIDATE), &fs).code.value(), 0);
    assert_eq!(run(&os(&COMPILE), &fs).code.value(), 0);
    fs.put("data.json", data.as_bytes());
    if altered_source {
        let model = SourceModel::new(
            DocumentId::new("rules.md").expect("id"),
            DocumentKind::Markdown,
            TEXT_ALTERED.to_owned(),
            Vec::new(),
        )
        .expect("model")
        .print();
        fs.put("model.json", model.as_bytes());
    }
    fs
}

fn evaluate_case(data: &str, altered_source: bool) -> (u8, String, String) {
    let fs = prepared(data, altered_source);
    let out = run(&os(&EVALUATE), &fs);
    let results = String::from_utf8(
        fs.files
            .borrow()
            .get(Path::new("results.json"))
            .expect("results written")
            .clone(),
    )
    .expect("utf8");
    (out.code.value(), out.stderr, results)
}

#[test]
fn indeterminate_and_provenance_failure_exit_4() {
    let (code, stderr, results) = evaluate_case(DATA_INDETERMINATE, true);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("RT-PRV-0002"), "{stderr}");
    assert!(results.contains("indeterminate"), "{results}");
    assert!(results.contains(r#""status":"not_verified""#), "{results}");
    assert!(!results.contains(r#""status":"verified""#), "{results}");
}

#[test]
fn decided_outcomes_with_provenance_failure_exit_2() {
    let (code, stderr, results) = evaluate_case(DATA_COMPLETE, true);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("RT-PRV-0002"), "{stderr}");
    assert!(!results.contains("indeterminate"), "{results}");
    assert!(results.contains(r#""status":"not_verified""#), "{results}");
    assert!(!results.contains(r#""status":"verified""#), "{results}");
}

#[test]
fn outcomes_are_identical_with_and_without_provenance_failure() {
    let cut = |text: String| text[..text.find(r#""verification":"#).expect("section")].to_owned();
    let (_, _, plain) = evaluate_case(DATA_INDETERMINATE, false);
    let (_, _, broken) = evaluate_case(DATA_INDETERMINATE, true);
    assert_eq!(cut(plain), cut(broken));
}
