//! Property 30: the exit code follows the earliest failing stage.
//!
//! For each of the four pipeline commands, any subset of that command's failure
//! causes is injected at once (through the real `rt_cli::run` over an in-memory
//! file system). The exit code must be the code of the earliest stage that
//! fails: invalid arguments or unreadable input (2), then the blocking
//! validation, compilation or Indeterminate result (3 or 4), then a failed
//! write (70). A Warning (`ingest` of a document without clauses) never changes
//! it. No value outside {0, 2, 3, 4} and 70 occurs.
//!
//! **Validates: Requirements 16.8, 16.10, 16.11, 16.12, 16.13, 16.14, 16.15,
//! 16.16, 16.17, 7.19, 27.10**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
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
    fn drop_file(&self, name: &str) {
        self.files.borrow_mut().remove(Path::new(name));
    }
}

impl Fs for MemFs {
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
const INGEST_DOC: &[u8] = b"1. Eligibility\n\n1.1 A participant must be at least 18 years old.\n";

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
const INGEST: [&str; 7] = [
    "ingest",
    "--document",
    "rules.txt",
    "--out",
    "model.json",
    "--candidates",
    "c.json",
];

/// The four failure causes of one command in stage order: arguments, input,
/// result, write. For `ingest` the third cause is the Warning "no clause",
/// which has exit code 0.
type Toggles = [bool; 4];

/// The code of the earliest active stage that has a failing code, else 0.
fn expected(toggles: Toggles, codes: [u8; 4]) -> u8 {
    toggles
        .iter()
        .zip(codes)
        .find(|(on, code)| **on && *code != 0)
        .map_or(0, |(_, code)| code)
}

fn command(base: &[&str], bad_argument: bool) -> Vec<OsString> {
    let mut args = base.to_vec();
    if bad_argument {
        args.push("--no-such-option");
    }
    os(&args)
}

/// Runs `which` (0 validate, 1 compile, 2 evaluate, 3 ingest) and returns the
/// exit code.
fn exit_of(which: usize, t: Toggles) -> u8 {
    let rulebook = rulebook_text();
    let fs = MemFs::default();
    fs.put("rb.json", rulebook.as_bytes());
    fs.put("model.json", model_text().as_bytes());
    let blocked_ledger = format!("{}\n", rt_ledger::HEADER_LINE);
    let [bad_argument, bad_input, result_fails, write_fails] = t;
    let ledger = if result_fails && which < 2 {
        blocked_ledger
    } else {
        approved_ledger(&rulebook)
    };
    fs.put("ledger.jsonl", ledger.as_bytes());
    let args = match which {
        0 => {
            if bad_input {
                fs.drop_file("model.json");
            }
            command(&VALIDATE, bad_argument)
        }
        1 => {
            // The earlier stage runs without faults and leaves its artifact
            // behind.
            let first = run(&os(&VALIDATE), &fs);
            assert!(matches!(first.code.value(), 0 | 3), "{}", first.stderr);
            if bad_input {
                fs.put("validation.json", b"not json");
            }
            command(&COMPILE, bad_argument)
        }
        2 => {
            let approved = approved_ledger(&rulebook);
            fs.put("ledger.jsonl", approved.as_bytes());
            assert_eq!(run(&os(&VALIDATE), &fs).code.value(), 0);
            assert_eq!(run(&os(&COMPILE), &fs).code.value(), 0);
            let data = if result_fails {
                DATA_INDETERMINATE
            } else {
                DATA_COMPLETE
            };
            fs.put("data.json", data.as_bytes());
            if bad_input {
                fs.drop_file("data.json");
            }
            command(&EVALUATE, bad_argument)
        }
        _ => {
            let doc: &[u8] = if result_fails {
                b"no numbered clause here\n"
            } else {
                INGEST_DOC
            };
            if !bad_input {
                fs.put("rules.txt", doc);
            }
            command(&INGEST, bad_argument)
        }
    };
    fs.fail_create.set(write_fails);
    run(&args, &fs).code.value()
}

fn codes_of(which: usize) -> [u8; 4] {
    match which {
        0 | 1 => [2, 2, 3, 70],
        2 => [2, 2, 4, 70],
        _ => [2, 2, 0, 70],
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn exit_code_is_that_of_the_earliest_failing_stage(
        which in 0usize..4,
        toggles in any::<[bool; 4]>(),
    ) {
        let code = exit_of(which, toggles);
        prop_assert!([0u8, 2, 3, 4, 70].contains(&code));
        prop_assert_eq!(code, expected(toggles, codes_of(which)), "command {} {:?}", which, toggles);
    }
}
