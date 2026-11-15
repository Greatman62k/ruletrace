//! Refused `ruletrace review` requests leave every file byte-identical
//! (single-file case).
//!
//! **Property 15: Review refusals leave files unchanged**
//!
//! For each refusal class (stale hash, invalid transition, invalid or AI-equal
//! reviewer, blocking Diagnostic, missing target, bad reason, missing
//! Source_Model) over generated parameters, the Rulebook, ledger and
//! Source_Model are byte identical afterwards (no lock or temporary file
//! remains), stdout is empty, and exactly one REVIEW Diagnostic with the
//! specified code and exit code is returned. The Proposal_Store is not part of
//! the single-file case.
//!
//! **Validates: Requirements 26.16, 26.17, 26.18, 26.19, 26.24, 26.25, 5.10**

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use proptest::prelude::*;
use rt_cli::{run, Fs};
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
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.0.borrow().clone()
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

    fn len(&self, path: &Path) -> io::Result<u64> {
        let files = self.0.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    }
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

/// One rule revision of a generated Rulebook: id, revision number, constant
/// value, clause index and a shift of the span start.
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

/// A ledger record: kind, rule, revision, content hash, IR_Version, bound
/// document hash.
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

const TIME: &str = "2026-11-14T09:00:00Z";

fn args(
    hash: &str,
    reviewer: &str,
    rule: &str,
    revision: u32,
    reason: Option<&str>,
    model: bool,
) -> Vec<String> {
    let verb = if reason.is_some() {
        "reject"
    } else {
        "approve"
    };
    let mut a: Vec<String> = [
        "review",
        verb,
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
    ]
    .map(String::from)
    .into();
    if model {
        a.extend(["--source-model", "model.json"].map(String::from));
    }
    a.extend(["--rule", rule].map(String::from));
    a.extend(["--revision".to_owned(), revision.to_string()]);
    a.extend(
        [
            "--content-hash",
            hash,
            "--reviewer",
            reviewer,
            "--time",
            TIME,
        ]
        .map(String::from),
    );
    if let Some(r) = reason {
        a.extend(["--reason".to_owned(), r.to_owned()]);
    }
    a
}

/// One refusal request: files, arguments, expected code and exit code.
struct Refusal {
    fs: MemFs,
    args: Vec<String>,
    code: &'static str,
    exit: u8,
}

fn base_revs() -> [Rev; 2] {
    [rev("d.a", 1, 1, 0), rev("d.b", 1, 2, 1)]
}

fn refusal(kind: u8, n: u32, text: &str, reject: bool) -> Refusal {
    let revs = base_revs();
    let hash = content_hash(TEXT, &revs, 0);
    let plain = || setup(TEXT, &rulebook(TEXT, "1", &revs), &ledger(&[]));
    let reason = reject.then_some("wrong threshold");
    let long = "x".repeat(201 + usize::try_from(n % 60).expect("small"));
    let build = |fs: MemFs, a: Vec<String>, code, exit| Refusal {
        fs,
        args: a,
        code,
        exit,
    };
    match kind {
        // Stale hash: any hash other than the current one.
        0 => {
            let stale = format!("{n:064x}");
            let stale = if stale == hash { "0".repeat(64) } else { stale };
            build(
                plain(),
                args(&stale, "A. Reviewer", "d.a", 1, reason, true),
                "RT-APR-0203",
                3,
            )
        }
        // Invalid transition: a second decision on a decided revision.
        1 => {
            let l = ledger(&[rec(
                if n.is_multiple_of(2) {
                    "approval"
                } else {
                    "rejection"
                },
                &revs,
                0,
            )]);
            let fs = setup(TEXT, &rulebook(TEXT, "1", &revs), &l);
            build(
                fs,
                args(&hash, "A. Reviewer", "d.a", 1, reason, true),
                "RT-APR-0204",
                3,
            )
        }
        // Invalid reviewer: empty or too long.
        2 => {
            let who = if n.is_multiple_of(2) {
                ""
            } else {
                long.as_str()
            };
            build(
                plain(),
                args(&hash, who, "d.a", 1, reason, true),
                "RT-APR-0202",
                3,
            )
        }
        // AI-equal reviewer: the name of an identity in the revision's AI
        // origin.
        3 => {
            let id = text;
            let h = sha256(b"x");
            let origin: &'static str = Box::leak(
                format!(r#"{{"kind":"ai_proposal","proposer":"{id}","provider":"openai","model":"m","adapter_version":"1","request_hash":"{h}","response_hash":"{h}","proposal_id":"p1","ai_identities":["{id}"]}}"#)
                    .into_boxed_str(),
            );
            let ai = [
                Rev {
                    origin,
                    ..rev("d.a", 1, 1, 0)
                },
                rev("d.b", 1, 2, 1),
            ];
            let fs = setup(TEXT, &rulebook(TEXT, "1", &ai), &ledger(&[]));
            build(
                fs,
                args(&content_hash(TEXT, &ai, 0), id, "d.a", 1, reason, true),
                "RT-APR-0201",
                3,
            )
        }
        // Blocking Diagnostic: a span on an undeclared document leaves no
        // approval binding.
        4 => {
            let rb = rulebook(TEXT, "1", &revs).replacen(
                r#""spans":[{"document":"rules.md""#,
                r#""spans":[{"document":"other.md""#,
                1,
            );
            let fs = setup(TEXT, &rb, &ledger(&[]));
            build(
                fs,
                args(&hash, "A. Reviewer", "d.a", 1, reason, true),
                "RT-APR-0205",
                3,
            )
        }
        // Missing target: unknown revision number or unknown rule.
        5 => {
            let (rule, revision) = if n.is_multiple_of(2) {
                ("d.a", 2 + n % 1000)
            } else {
                ("d.zz", 1)
            };
            build(
                plain(),
                args(&hash, "A. Reviewer", rule, revision, reason, true),
                "RT-APR-0206",
                2,
            )
        }
        // Bad reason: empty or over 2000 characters (reject only).
        6 => {
            let bad = if n.is_multiple_of(2) {
                String::new()
            } else {
                "r".repeat(2001 + usize::try_from(n % 50).expect("small"))
            };
            build(
                plain(),
                args(&hash, "A. Reviewer", "d.a", 1, Some(&bad), true),
                "RT-APR-0207",
                2,
            )
        }
        // Missing Source_Model: none supplied, or a model for another document
        // hash.
        _ => {
            let fs = plain();
            let a = args(&hash, "A. Reviewer", "d.a", 1, reason, false);
            if n % 2 == 1 {
                fs.put(
                    "model.json",
                    model("# Other\n\nDifferent text.\n").as_bytes(),
                );
            }
            let mut a = a;
            if n % 2 == 1 {
                a.extend(["--source-model".to_owned(), "model.json".to_owned()]);
            }
            build(fs, a, "RT-APR-0209", 2)
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn refused_review_leaves_files_unchanged(
        kind in 0u8..8, n in any::<u32>(), name in "[a-z]{1,12}", reject in any::<bool>()
    ) {
        let r = refusal(kind, n, &name, reject);
        let before = r.fs.snapshot();
        let argv: Vec<OsString> = r.args.iter().map(OsString::from).collect();
        let out = run(&argv, &r.fs);
        prop_assert_eq!(out.code.value(), r.exit, "{}", out.stderr);
        prop_assert_eq!(out.stderr.matches("RT-APR-").count(), 1, "{}", out.stderr);
        prop_assert!(out.stderr.contains(&format!(" {}: ", r.code)), "{}", out.stderr);
        prop_assert!(out.stdout.is_empty());
        prop_assert_eq!(r.fs.snapshot(), before);
    }
}
