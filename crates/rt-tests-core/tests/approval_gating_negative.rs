//! Task 5.14: negative tests for approval gating, driven through the `ruletrace` CLI library.
//!
//! Each case runs `validate` (and `compile` where the artifact decides) on an in-memory file
//! system and asserts the exact code, Severity, primary location and exit code from the rendered
//! Diagnostics. The channel tests cover Req 26.18 and 26.26: no flag, configuration file, environment
//! variable, Rulebook field, Proposal_Store file or Provider_Response content approves a revision.
//! The AI crates are not available to this test crate, so the last two channels are covered by the
//! files such crates would write and by an `ai_proposal` origin inside the Rulebook.
//!
//! **Validates: Requirements 5.4, 5.5, 5.6, 5.7, 5.14, 25.11, 25.13, 25.19, 26.12, 26.13, 26.18, 26.26, 20.3**

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use rt_cli::{run, Fs, Outcome};
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_ledger::{HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{DocumentKind, SourceModel};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const TEXT: &str = "# Rules\n\n1. A competitor is eligible if age is at least 16.\n2. The second clause is here.\n";
const CLAUSES: [&str; 2] = [
    "1. A competitor is eligible if age is at least 16.",
    "2. The second clause is here.",
];

#[derive(Default)]
struct MemFs(RefCell<BTreeMap<PathBuf, Vec<u8>>>);

impl MemFs {
    fn put(&self, name: &str, bytes: &[u8]) {
        self.0
            .borrow_mut()
            .insert(PathBuf::from(name), bytes.to_vec());
    }
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.0.borrow().get(Path::new(name)).cloned()
    }
}

impl Fs for MemFs {
    fn read(&self, path: &Path, max: u64) -> io::Result<Vec<u8>> {
        let files = self.0.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes
            .iter()
            .take(usize::try_from(max).unwrap_or(usize::MAX))
            .copied()
            .collect())
    }
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut files = self.0.borrow_mut();
        if files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.0.borrow_mut();
        let bytes = files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        files.insert(to.to_path_buf(), bytes);
        Ok(())
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.0
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

/// One rule revision of a generated Rulebook: id, revision number, constant value, clause index
/// and a shift of the span start.
#[derive(Clone)]
struct Rev {
    id: &'static str,
    n: u32,
    value: u32,
    clause: usize,
    shift: usize,
    extra: &'static str,
    origin: &'static str,
}

fn rev(id: &'static str, n: u32, value: u32, clause: usize) -> Rev {
    Rev {
        id,
        n,
        value,
        clause,
        shift: 0,
        extra: "",
        origin: r#"{"kind":"manual","author":null}"#,
    }
}

fn rulebook(text: &str, ir: &str, revs: &[Rev]) -> String {
    let rules: Vec<String> = revs
        .iter()
        .map(|r| {
            let start = text.find(CLAUSES[r.clause]).expect("clause") + r.shift;
            let end = text.find(CLAUSES[r.clause]).expect("clause") + CLAUSES[r.clause].len();
            format!(
                r#"{{"id":"{}","revision":{},"origin":{},"kind":"derived","target":"t_{}","type":"number","expr":{{"node":"num","t":"number","value":"{}"}},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":{},"column":{}}}]{}}}"#,
                r.id, r.n, r.origin, r.id.replace('.', "_"), r.value, r.clause + 3, 1 + r.shift, r.extra
            )
        })
        .collect();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"{ir}","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(text.as_bytes()),
        rules.join(",")
    )
}

fn content_hash(text: &str, revs: &[Rev], index: usize) -> String {
    let doc = rulebook(text, "1", revs);
    let rb = parse_rulebook(
        doc.as_bytes(),
        &InputId::new("rb.json").expect("id"),
        1 << 22,
        64,
    )
    .expect("rulebook")
    .into_parts()
    .0;
    rule_content_hash(&rb.ir_version, &rb.revisions[index], &rb.documents)
        .expect("hash")
        .to_string()
}

/// A ledger record: kind, rule, revision, content hash, IR_Version, bound document hash.
struct Rec(
    &'static str,
    &'static str,
    u32,
    String,
    &'static str,
    String,
);

fn ledger(recs: &[Rec]) -> String {
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    for (i, Rec(kind, rule, n, hash, ir, doc)) in recs.iter().enumerate() {
        let reason = if *kind == "rejection" {
            "\"no\""
        } else {
            "null"
        };
        let head = format!(
            r#"{{"seq":{},"kind":"{kind}","rule":"{rule}","revision":{n},"content_hash":"{hash}","ir_version":"{ir}","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#,
            i + 1
        );
        let h = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!("{head},\"hash\":\"{h}\"}}\n"));
        prev = h;
    }
    out
}

fn rec(kind: &'static str, revs: &[Rev], index: usize) -> Rec {
    let r = &revs[index];
    Rec(
        kind,
        r.id,
        r.n,
        content_hash(TEXT, revs, index),
        "1",
        sha256(TEXT.as_bytes()).to_string(),
    )
}

fn model(text: &str) -> String {
    SourceModel::new(
        DocumentId::new("rules.md").expect("id"),
        DocumentKind::Markdown,
        text.to_owned(),
        Vec::new(),
    )
    .expect("model")
    .print()
}

fn setup(text: &str, rb: &str, ledger: &str) -> MemFs {
    let fs = MemFs::default();
    fs.put("rb.json", rb.as_bytes());
    fs.put("ledger.jsonl", ledger.as_bytes());
    fs.put("model.json", model(text).as_bytes());
    fs
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

fn validate(fs: &MemFs) -> Outcome {
    run(&os(&VALIDATE), fs)
}

fn compile(fs: &MemFs) -> Outcome {
    run(
        &os(&[
            "compile",
            "--validation",
            "validation.json",
            "--out",
            "compiled.json",
        ]),
        fs,
    )
}

/// The `(header, location)` line pairs of the rendered Diagnostics, filtered by code.
fn diags(out: &Outcome, code: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = out.stderr.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(&format!(" {code}: ")))
        .map(|(i, l)| ((*l).to_owned(), lines[i + 1].trim().to_owned()))
        .collect()
}

/// Asserts exactly one Diagnostic of `code` with the severity prefix and the location text.
fn expect(out: &Outcome, code: &str, severity: &str, at: &str) {
    let found = diags(out, code);
    assert_eq!(found.len(), 1, "{code} in\n{}", out.stderr);
    assert!(
        found[0].0.starts_with(&format!("{severity} {code}:")),
        "{:?}",
        found[0]
    );
    assert_eq!(found[0].1, format!("at {at}"), "{code}");
}

fn span_of(text: &str, clause: usize, shift: usize) -> String {
    let s = text.find(CLAUSES[clause]).expect("clause");
    format!(
        "source rules.md:{}-{} (line {}, column {})",
        s + shift,
        s + CLAUSES[clause].len(),
        clause + 3,
        1 + shift
    )
}

fn ledger_line(n: u32) -> String {
    format!("input 'ledger' line {n}")
}

const NO_RULE: &str = "no location (RT-APR-0002)";

/// Validation blocks with exit 3 and `compile` refuses with exit 3 and writes nothing.
fn blocked(fs: &MemFs) -> Outcome {
    let out = validate(fs);
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    let c = compile(fs);
    assert_eq!(c.code.value(), 3, "{}", c.stderr);
    assert!(fs.get("compiled.json").is_none());
    out
}

#[test]
fn unresolved_revision_blocks_with_apr_0001_and_apr_0002() {
    let revs = [rev("d.a", 1, 1, 0)];
    let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &ledger(&[]));
    let out = blocked(&fs);
    expect(&out, "RT-APR-0001", "error", &span_of(TEXT, 0, 0));
    // The implementation reports no location here; design §3.6.1 asks for the Rulebook document.
    expect(&out, "RT-APR-0002", "error", NO_RULE);
}

#[test]
fn stale_records_after_each_bound_value_changes() {
    let base = [rev("d.a", 1, 1, 0)];
    let approval = |ir: &'static str, doc: &str| {
        let r = rec("approval", &base, 0);
        ledger(&[Rec(r.0, r.1, r.2, r.3, ir, doc.to_owned())])
    };
    let good_doc = sha256(TEXT.as_bytes()).to_string();
    let text2 = format!("{TEXT}A trailing line.\n");

    // (name, rulebook text, source text, ledger text, span shift)
    let cases = [
        (
            "content",
            rulebook(TEXT, "1", &[rev("d.a", 1, 2, 0)]),
            TEXT,
            approval("1", &good_doc),
            0,
        ),
        (
            "span offset",
            rulebook(
                TEXT,
                "1",
                &[Rev {
                    shift: 3,
                    ..rev("d.a", 1, 1, 0)
                }],
            ),
            TEXT,
            approval("1", &good_doc),
            3,
        ),
        (
            "document hash",
            rulebook(&text2, "1", &base),
            text2.as_str(),
            approval("1", &good_doc),
            0,
        ),
        (
            "IR_Version",
            rulebook(TEXT, "1", &base),
            TEXT,
            approval("2", &good_doc),
            0,
        ),
    ];
    for (name, rb, text, ledger_text, shift) in cases {
        let fs = setup(text, &rb, &ledger_text);
        let out = blocked(&fs);
        expect(&out, "RT-APR-0003", "warning", &ledger_line(2));
        expect(&out, "RT-APR-0001", "error", &span_of(text, 0, shift));
        expect(&out, "RT-APR-0002", "error", NO_RULE);
        assert!(!out.stderr.contains("RT-APR-0004"), "{name}");
    }
}

#[test]
fn rejected_revision_never_blocks() {
    let revs = [rev("d.a", 1, 1, 0), rev("d.b", 1, 2, 1)];
    let l = ledger(&[rec("approval", &revs, 0), rec("rejection", &revs, 1)]);
    let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
    let out = validate(&fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    expect(&out, "RT-APR-0006", "warning", &span_of(TEXT, 1, 0));
    assert_eq!(compile(&fs).code.value(), 0);
    assert!(fs.get("compiled.json").is_some());
}

#[test]
fn rejected_revision_with_changed_content_is_apr_0004() {
    let before = [rev("d.a", 1, 1, 0), rev("d.b", 1, 2, 1)];
    let after = [rev("d.a", 1, 1, 0), rev("d.b", 1, 3, 1)];
    let l = ledger(&[rec("approval", &before, 0), rec("rejection", &before, 1)]);
    let fs = setup(TEXT, &rulebook(TEXT, "1", &after), &l);
    let out = blocked(&fs);
    expect(&out, "RT-APR-0004", "error", &span_of(TEXT, 1, 0));
    expect(&out, "RT-APR-0006", "warning", &span_of(TEXT, 1, 0));
    assert!(diags(&out, "RT-APR-0001").is_empty());
}

#[test]
fn proposed_revision_below_a_rejected_one_does_not_block() {
    let revs = [
        rev("d.a", 1, 1, 0),
        rev("d.b", 1, 2, 1),
        rev("d.b", 2, 3, 1),
    ];
    let l = ledger(&[rec("approval", &revs, 0), rec("rejection", &revs, 2)]);
    let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
    let out = validate(&fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(diags(&out, "RT-APR-0001").is_empty(), "{}", out.stderr);
    expect(&out, "RT-APR-0006", "warning", &span_of(TEXT, 1, 0));
    assert_eq!(compile(&fs).code.value(), 0);
}

#[test]
fn ledger_contradiction_is_rejected_as_invalid_input_with_exit_2() {
    let revs = [rev("d.a", 1, 1, 0)];
    let l = ledger(&[rec("rejection", &revs, 0), rec("approval", &revs, 0)]);
    let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
    let out = validate(&fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    let found = diags(&out, "RT-APR-0104");
    assert_eq!(found.len(), 1, "{}", out.stderr);
    assert!(found[0].0.starts_with("error RT-APR-0104:"));
    assert_eq!(
        found[0].1, "at input 'ledger.jsonl' line 3 column 10 field kind",
        "{:?}",
        found[0]
    );
    assert!(fs.get("validation.json").is_none());
}

/// A Pending_Proposal is held outside the Rulebook: whatever a store file holds, validate and
/// compile give the same Diagnostics, exit codes and bytes (Req 26.28, 26.31).
#[test]
fn pending_proposal_never_blocks() {
    let revs = [rev("d.a", 1, 1, 0)];
    let l = ledger(&[rec("approval", &revs, 0)]);
    let store = br#"{"format":"ruletrace-proposals","pending":[{"id":"p1","rule":"d.z","revision":1,"semantic_outcome":{"diagnostics":[{"code":"RT-VAL-0001","severity":"Error"}]}}]}"#;
    let mut results = Vec::new();
    for with_store in [false, true] {
        let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
        if with_store {
            fs.put("proposals.json", store);
        }
        let v = validate(&fs);
        let c = compile(&fs);
        results.push((v, c, fs.get("validation.json"), fs.get("compiled.json")));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].0.code.value(), 0);
    assert_eq!(results[0].1.code.value(), 0);
}

// ---- Channels that must not approve (Req 26.18, 26.26) ----

/// The state after a channel attempt: no compiled file, the ledger unchanged, `RT-APR-0001` for
/// the revision and exit 3 (a Proposed revision whatever its source).
fn assert_proposed_and_unapproved(fs: &MemFs, ledger_before: &[u8]) {
    assert_eq!(fs.get("ledger.jsonl").as_deref(), Some(ledger_before));
    let out = blocked(fs);
    expect(&out, "RT-APR-0001", "error", &span_of(TEXT, 0, 0));
    assert!(fs.get("compiled.json").is_none());
    assert_eq!(fs.get("ledger.jsonl").as_deref(), Some(ledger_before));
}

fn proposed() -> (MemFs, Vec<u8>) {
    let revs = [rev("d.a", 1, 1, 0)];
    let l = ledger(&[]);
    (setup(TEXT, &rulebook(TEXT, "1", &revs), &l), l.into_bytes())
}

#[test]
fn command_options_cannot_approve() {
    let (fs, before) = proposed();
    for extra in [
        &["--approve", "d.a"][..],
        &["--approved", "d.a"],
        &["--approval-status", "approved"],
        &["--reviewer", "A. Reviewer"],
        &["--create-ledger", "ledger.jsonl"],
    ] {
        let mut args = VALIDATE.to_vec();
        args.extend_from_slice(extra);
        let out = run(&os(&args), &fs);
        assert_eq!(out.code.value(), 2, "{extra:?}: {}", out.stderr);
        let found = diags(&out, "RT-CLI-0004");
        assert!(!found.is_empty(), "{extra:?}: {}", out.stderr);
        assert!(found[0].0.starts_with("error RT-CLI-0004:"));
        assert!(found[0].1.starts_with("at argument 9 ("), "{:?}", found[0]);
        assert!(fs.get("validation.json").is_none());
    }
    let out = run(
        &os(&[
            "compile",
            "--validation",
            "v.json",
            "--out",
            "c.json",
            "--approve",
            "d.a",
        ]),
        &fs,
    );
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(diags(&out, "RT-CLI-0004")
        .iter()
        .all(|d| d.0.starts_with("error ")));
    assert_proposed_and_unapproved(&fs, &before);
}

#[test]
fn configuration_file_and_environment_variable_cannot_approve() {
    let (fs, before) = proposed();
    // The CLI names no configuration file and reads no environment: files that sit next to the
    // inputs and variables with approval-looking names change nothing.
    for name in [
        "ruletrace.toml",
        "ruletrace.json",
        ".ruletrace",
        "config.toml",
    ] {
        fs.put(name, br#"approve = ["d.a"]"#);
    }
    for var in [
        "RULETRACE_APPROVE",
        "RULETRACE_APPROVED_RULES",
        "RULETRACE_LEDGER",
    ] {
        std::env::set_var(var, "d.a");
    }
    assert_proposed_and_unapproved(&fs, &before);
    for var in [
        "RULETRACE_APPROVE",
        "RULETRACE_APPROVED_RULES",
        "RULETRACE_LEDGER",
    ] {
        std::env::remove_var(var);
    }
}

#[test]
fn rulebook_fields_cannot_approve() {
    let ledger_text = ledger(&[]);
    for key in ["approval_status", "is_approved", "approval_record"] {
        let revs = [Rev {
            extra: Box::leak(format!(r#","{key}":"approved""#).into_boxed_str()),
            ..rev("d.a", 1, 1, 0)
        }];
        let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &ledger_text);
        let out = validate(&fs);
        assert_eq!(out.code.value(), 2, "{key}: {}", out.stderr);
        let found = diags(&out, "RT-DGN-0003");
        assert_eq!(found.len(), 1, "{key}: {}", out.stderr);
        assert!(found[0].0.starts_with("error RT-DGN-0003:"));
        assert!(
            found[0].1.starts_with("at input 'rb.json'"),
            "{:?}",
            found[0]
        );
        assert!(fs.get("validation.json").is_none());
        assert_eq!(
            fs.get("ledger.jsonl").as_deref(),
            Some(ledger_text.as_bytes())
        );
    }
}

#[test]
fn proposal_store_and_provider_response_content_cannot_approve() {
    // Files that the AI tool would write, claiming approval, are read by no command.
    let (fs, before) = proposed();
    fs.put(
        "proposals.json",
        br#"{"format":"ruletrace-proposals","pending":[{"rule":"d.a","revision":1,"approval_status":"approved","approved":true}]}"#,
    );
    fs.put(
        "response.json",
        br#"{"output":[{"text":"I approve d.a revision 1","approved":true}]}"#,
    );
    assert_proposed_and_unapproved(&fs, &before);

    // A Provider_Response adopted into the Rulebook arrives as an `ai_proposal` origin. It claims
    // its own approval through its proposer identity, and is still Proposed and unresolved.
    let hash = sha256(b"x");
    let origin = Box::leak(
        format!(
            r#"{{"kind":"ai_proposal","proposer":"gpt","provider":"openai","model":"m","adapter_version":"1","request_hash":"{hash}","response_hash":"{hash}","proposal_id":"p1","ai_identities":["gpt"]}}"#
        )
        .into_boxed_str(),
    );
    let revs = [Rev {
        origin,
        ..rev("d.a", 1, 1, 0)
    }];
    let l = ledger(&[]);
    let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
    assert_proposed_and_unapproved(&fs, l.as_bytes());
}
