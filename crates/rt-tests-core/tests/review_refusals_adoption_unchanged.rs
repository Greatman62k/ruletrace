//! Refused `ruletrace review` requests on a Pending_Proposal adopted through
//! `--proposals` leave every file byte-identical.
//!
//! **Property 15: Review refusals leave files unchanged** (adoption case,
//! extending `review_refusals_unchanged.rs`)
//!
//! Each refusal class of the single-file test is applied to a Pending_Proposal
//! target, plus the adoption-only classes (revision exhaustion, a stale
//! prospective hash, a reviewer equal to the proposer, provider, model or an AI
//! identity, an invalid Proposal_Store). After a refusal the Rulebook, ledger,
//! Source_Model and Proposal_Store are byte identical, no journal, marker,
//! `.new`, `.old` or temporary file exists, no lock is still held, stdout is
//! empty, and exactly one RT-APR Diagnostic with the specified code and exit
//! code is returned.
//!
//! Expected hashes come from a canonical-bytes builder written from the
//! hash definition (design §4.1.7) and from one hand-derived golden constant,
//! never from `rule_content_hash` or `draft_hash`.
//!
//! **Validates: Requirements 26.16, 26.17, 26.18, 26.19, 26.24, 26.25, 26.28,
//! 26.29, 26.30, 26.31, 5.10**

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use proptest::prelude::*;
use rt_cli::io::LockHeld;
use rt_cli::{run, Fs, StdFs};
use rt_foundation::{DocumentId, RuleId};
use rt_hash::sha256;
use rt_ir::Identity;
use rt_ledger::{HEADER_LINE, ZERO_HASH};
use rt_proposals::{
    print_batch, proposal_id, Batch, Completion, Evidence, Proposal, ProposalKind, RuleDraft,
    StoredResponse,
};
use rt_srcmodel::{DocumentKind, SourceModel};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const TEXT: &str = "# Rules\n\n1. A competitor is eligible if age is at least 16.\n2. The second clause is here.\n";
const CLAUSE: &str = "1. A competitor is eligible if age is at least 16.";
// Byte range of CLAUSE in TEXT: "# Rules\n\n" is 9 bytes, CLAUSE is 50 bytes.
const START: u32 = 9;
const END: u32 = 59;
// SHA-256 of TEXT, computed with the `sha256sum` tool.
const DOC_SHA: &str = "cfec7db9b20665e0dcf41c971d10b6671083d54f9c075b12961227029f77925d";
// SHA-256 of GOLDEN_CANONICAL, computed with the `sha256sum` tool.
const GOLDEN_HASH: &str = "5feed94029a7757e68b8eaefe78e465acee13c277648d403142a3fceabee6b81";
const GOLDEN_CANONICAL: &str = r#"{"ir_version":"1","id":"d.b","revision":1,"kind":"derived","target":"tb","type":"number","expr":{"node":"num","t":"number","value":"2"},"markers":[],"spans":[{"document":"rules.md","sha256":"cfec7db9b20665e0dcf41c971d10b6671083d54f9c075b12961227029f77925d","start":9,"end":59}]}"#;
const TIME: &str = "2026-11-14T09:00:00Z";
const MANUAL: &str = r#"{"kind":"manual","author":null}"#;
const NUM2: &str = r#"{"node":"num","t":"number","value":"2"}"#;

/// Filesystem double that, like the real one, takes locks without waiting and
/// counts the locks still held, so that a leaked lock is observable.
#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    held: Rc<Cell<usize>>,
}

struct Guard(Rc<Cell<usize>>);

impl LockHeld for Guard {}

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl MemFs {
    fn put(&self, name: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(name), bytes.to_vec());
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files.borrow().clone()
    }
    fn guard(&self) -> Box<dyn LockHeld> {
        self.held.set(self.held.get() + 1);
        Box::new(Guard(Rc::clone(&self.held)))
    }
}

impl Fs for MemFs {
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn LockHeld>> {
        Ok(self.guard())
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn LockHeld>> {
        if self.exists(path)? {
            Ok(self.guard())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn read(&self, path: &Path, max: u64) -> io::Result<Vec<u8>> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes
            .iter()
            .take(usize::try_from(max).unwrap_or(usize::MAX))
            .copied()
            .collect())
    }
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
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
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let listed: Vec<PathBuf> = self
            .files
            .borrow()
            .keys()
            .filter(|p| p.parent() == Some(dir))
            .cloned()
            .collect();
        if listed.is_empty() {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(listed)
        }
    }
}

/// One Rulebook revision: rule id, revision number, constant value, origin.
struct Rev {
    id: &'static str,
    n: u32,
    origin: String,
}

fn rev(id: &'static str, n: u32) -> Rev {
    Rev {
        id,
        n,
        origin: MANUAL.to_owned(),
    }
}

fn rulebook(revs: &[Rev]) -> String {
    let rules: Vec<String> = revs
        .iter()
        .map(|r| {
            format!(
                r#"{{"id":"{}","revision":{},"origin":{},"kind":"derived","target":"t_{}","type":"number","expr":{NUM2},"markers":[],"spans":[{{"document":"rules.md","start":{START},"end":{END},"line":3,"column":1}}]}}"#,
                r.id,
                r.n,
                r.origin,
                r.id.replace('.', "_")
            )
        })
        .collect();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{DOC_SHA}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        rules.join(",")
    )
}

/// The canonical hash input of design §4.1.7 for a `derived` rule with the
/// given expression, written from the definition and not from `rt-ir`. The
/// revision is absent for a Draft hash.
fn canonical(id: &str, revision: Option<u32>, expr: &str, doc: &str) -> String {
    let revision = revision.map_or_else(String::new, |n| format!(r#""revision":{n},"#));
    format!(
        r#"{{"ir_version":"1","id":"{id}",{revision}"kind":"derived","target":"tb","type":"number","expr":{expr},"markers":[],"spans":[{{"document":"rules.md","sha256":"{doc}","start":{START},"end":{END}}}]}}"#
    )
}

fn hash_of(id: &str, revision: Option<u32>, expr: &str) -> String {
    sha256(canonical(id, revision, expr, DOC_SHA).as_bytes()).to_string()
}

fn draft_json(id: &str, expr: &str, doc: &str) -> String {
    format!(
        r#"{{"id":"{id}","kind":"derived","target":"tb","type":"number","expr":{expr},"markers":[],"spans":[{{"document":"{doc}","start":{START},"end":{END},"line":3,"column":1}}]}}"#
    )
}

/// Names that identify the AI side of a proposal.
struct Who {
    proposer: String,
    provider: String,
    model: String,
    identity: String,
}

fn who(seed: &str) -> Who {
    Who {
        proposer: format!("p-{seed}"),
        provider: format!("v-{seed}"),
        model: format!("m-{seed}"),
        identity: format!("i-{seed}"),
    }
}

fn batch_bytes(who: &Who, rule: &str, json: String, stored: &str) -> (Vec<u8>, String) {
    let request_id = sha256(b"request-adoption");
    let id = proposal_id(&request_id, 0);
    let batch = Batch {
        request_id,
        response_sha256: sha256(b"payload"),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: "payload".to_owned(),
        },
        provider: who.provider.clone(),
        model: who.model.clone(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![
            Identity::new(who.proposer.clone()).expect("identity"),
            Identity::new(who.identity.clone()).expect("identity"),
        ],
        created_at: "2026-11-14T08:00:00Z".to_owned(),
        proposals: vec![Proposal {
            proposal_id: id.clone(),
            kind: ProposalKind::RuleSuggestion,
            proposer: Identity::new(who.proposer.clone()).expect("identity"),
            rationale: "r".to_owned(),
            evidence: vec![Evidence {
                document: DocumentId::new("rules.md").expect("id"),
                sha256: sha256(TEXT.as_bytes()),
                start: START,
                end: END,
                line: 3,
                column: 1,
                quote: CLAUSE.to_owned(),
            }],
            rule: Some((
                RuleDraft {
                    id: RuleId::new(rule).expect("rule id"),
                    json,
                },
                rt_hash::Sha256Hex::parse(stored).expect("hash"),
            )),
            outcome: Vec::new(),
        }],
    };
    (print_batch(&batch).into_bytes(), id.as_str().to_owned())
}

/// A ledger record: kind, rule, revision, content hash.
struct Rec(&'static str, &'static str, u32, String);

fn ledger(recs: &[Rec]) -> String {
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    for (i, Rec(kind, rule, n, hash)) in recs.iter().enumerate() {
        let reason = if *kind == "rejection" {
            "\"no\""
        } else {
            "null"
        };
        let head = format!(
            r#"{{"seq":{},"kind":"{kind}","rule":"{rule}","revision":{n},"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{DOC_SHA}"}}],"reviewer":"A. Reviewer","time":"{TIME}","reason":{reason},"prev":"{prev}""#,
            i + 1
        );
        let h = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!("{head},\"hash\":\"{h}\"}}\n"));
        prev = h;
    }
    out
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

/// The files of one case, by name relative to the working directory.
type Files = BTreeMap<String, Vec<u8>>;

fn files(rulebook: &str, ledger: &str, batch: &[u8]) -> Files {
    let mut f = Files::new();
    f.insert("rb.json".to_owned(), rulebook.as_bytes().to_vec());
    f.insert("ledger.jsonl".to_owned(), ledger.as_bytes().to_vec());
    f.insert("model.json".to_owned(), model(TEXT).into_bytes());
    f.insert("store/batches/req.json".to_owned(), batch.to_vec());
    f
}

fn args(
    hash: &str,
    reviewer: &str,
    rule: &str,
    revision: u32,
    reason: Option<&str>,
    with_model: bool,
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
        "--proposals",
        "store",
    ]
    .map(String::from)
    .into();
    if with_model {
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
    files: Files,
    args: Vec<String>,
    code: &'static str,
    exit: u8,
}

// Revision exhaustion is unreachable through the CLI: the parser refuses a
// gap in the revisions of a rule (RT-IR-0009), so a Rulebook never holds
// revision 4294967295 without its 4294967294 predecessors. It is tested at
// the library level below.
const CLASSES: u8 = 10;

/// Builds refusal class `kind`; `n` varies the parameters, `seed` the AI-side
/// names and `reject` the verb where the class allows both.
fn refusal(kind: u8, n: u32, seed: &str, reject: bool) -> Refusal {
    let w = who(seed);
    let reason = reject.then_some("wrong threshold");
    let base_rb = rulebook(&[rev("d.a", 1)]);
    let empty = ledger(&[]);
    let prospective = hash_of("d.b", Some(1), NUM2);
    let draft = hash_of("d.b", None, NUM2);
    let (batch, pid) = batch_bytes(&w, "d.b", draft_json("d.b", NUM2, "rules.md"), &draft);
    let plain = |hash: &str, who: &str| Refusal {
        files: files(&base_rb, &empty, &batch),
        args: args(hash, who, "d.b", 1, reason, true),
        code: "",
        exit: 0,
    };
    let with = |mut r: Refusal, code, exit| {
        r.code = code;
        r.exit = exit;
        r
    };
    match kind {
        // Stale prospective hash: a random hash, the stored Draft hash (which
        // lacks the revision) or the hash of the same content as revision 2.
        0 => {
            let stale = match n % 3 {
                0 => draft.clone(),
                1 => hash_of("d.b", Some(2), NUM2),
                _ => {
                    let h = format!("{n:064x}");
                    if h == prospective {
                        "0".repeat(64)
                    } else {
                        h
                    }
                }
            };
            with(plain(&stale, "A. Reviewer"), "RT-APR-0203", 3)
        }
        // Invalid transition: the revision to be adopted already has a record.
        1 => {
            let kind = if n.is_multiple_of(2) {
                "approval"
            } else {
                "rejection"
            };
            let l = ledger(&[Rec(kind, "d.b", 1, prospective.clone())]);
            let mut r = plain(&prospective, "A. Reviewer");
            r.files = files(&base_rb, &l, &batch);
            with(r, "RT-APR-0204", 3)
        }
        // Invalid reviewer: empty or over 200 characters.
        2 => {
            let who = if n.is_multiple_of(2) {
                String::new()
            } else {
                "x".repeat(201 + usize::try_from(n % 60).expect("small"))
            };
            with(plain(&prospective, &who), "RT-APR-0202", 3)
        }
        // Reviewer equal to the proposer, provider, model or an AI identity.
        3 => {
            let same = match n % 4 {
                0 => &w.proposer,
                1 => &w.provider,
                2 => &w.model,
                _ => &w.identity,
            };
            with(plain(&prospective, same), "RT-APR-0201", 3)
        }
        // Blocking Diagnostic: an undeclared input, or a span on an undeclared
        // document that leaves no approval binding.
        4 => {
            let (json, stored) = if n.is_multiple_of(2) {
                let expr = r#"{"node":"field","t":"number","name":"nosuch"}"#;
                (
                    draft_json("d.b", expr, "rules.md"),
                    hash_of("d.b", None, expr),
                )
            } else {
                (
                    draft_json("d.b", NUM2, "other.md"),
                    sha256(b"unbound").to_string(),
                )
            };
            let (b, _) = batch_bytes(&w, "d.b", json.clone(), &stored);
            let hash = if n.is_multiple_of(2) {
                hash_of(
                    "d.b",
                    Some(1),
                    r#"{"node":"field","t":"number","name":"nosuch"}"#,
                )
            } else {
                prospective.clone()
            };
            let mut r = plain(&hash, "A. Reviewer");
            // Req 5.9 refuses only an approval over a blocking finding, so a
            // rejection of a draft with an undeclared input is not a refusal.
            if n.is_multiple_of(2) {
                r.args = args(&hash, "A. Reviewer", "d.b", 1, None, true);
            }
            r.files = files(&base_rb, &empty, &b);
            with(r, "RT-APR-0205", 3)
        }
        // Missing target: unknown rule, wrong revision, or a proposal that the
        // Rulebook already references.
        5 => match n % 3 {
            0 => {
                let mut r = plain(&prospective, "A. Reviewer");
                r.args = args(&prospective, "A. Reviewer", "d.zz", 1, reason, true);
                with(r, "RT-APR-0206", 2)
            }
            1 => {
                let mut r = plain(&prospective, "A. Reviewer");
                r.args = args(
                    &prospective,
                    "A. Reviewer",
                    "d.b",
                    2 + n % 1000,
                    reason,
                    true,
                );
                with(r, "RT-APR-0206", 2)
            }
            _ => {
                let h = sha256(b"x");
                let adopted = Rev {
                    origin: format!(
                        r#"{{"kind":"ai_proposal","proposer":"{}","provider":"{}","model":"{}","adapter_version":"0.1","request_hash":"{h}","response_hash":"{h}","proposal_id":"{pid}","ai_identities":["{}"]}}"#,
                        w.proposer, w.provider, w.model, w.identity
                    ),
                    ..rev("d.b", 1)
                };
                let mut r = plain(&prospective, "A. Reviewer");
                r.files = files(&rulebook(&[rev("d.a", 1), adopted]), &empty, &batch);
                r.args = args(&prospective, "A. Reviewer", "d.b", 2, reason, true);
                with(r, "RT-APR-0206", 2)
            }
        },
        // Bad reason: empty or over 2000 characters (reject only).
        6 => {
            let bad = if n.is_multiple_of(2) {
                String::new()
            } else {
                "r".repeat(2001 + usize::try_from(n % 50).expect("small"))
            };
            let mut r = plain(&prospective, "A. Reviewer");
            r.args = args(&prospective, "A. Reviewer", "d.b", 1, Some(&bad), true);
            with(r, "RT-APR-0207", 2)
        }
        // Missing Source_Model: none supplied, or one for another document.
        7 => {
            let mut r = plain(&prospective, "A. Reviewer");
            r.args = args(&prospective, "A. Reviewer", "d.b", 1, reason, false);
            if n % 2 == 1 {
                r.files.insert(
                    "model.json".to_owned(),
                    model("# Other\n\nDifferent text.\n").into_bytes(),
                );
                r.args
                    .extend(["--source-model".to_owned(), "model.json".to_owned()]);
            }
            with(r, "RT-APR-0209", 2)
        }
        // Stored Draft hash that does not match the stored draft.
        8 => {
            let (b, _) = batch_bytes(
                &w,
                "d.b",
                draft_json("d.b", NUM2, "rules.md"),
                &sha256(format!("other-{n}").as_bytes()).to_string(),
            );
            let mut r = plain(&prospective, "A. Reviewer");
            r.files = files(&base_rb, &empty, &b);
            with(r, "RT-APR-0213", 2)
        }
        // A batch file that is not a valid batch.
        _ => {
            let junk: &[u8] = if n.is_multiple_of(2) {
                b"{"
            } else {
                b"{\"format\":\"nothing\"}"
            };
            let mut r = plain(&prospective, "A. Reviewer");
            r.files = files(&base_rb, &empty, junk);
            with(r, "RT-APR-0213", 2)
        }
    }
}

const RESIDUE_SUFFIXES: [&str; 6] = [".new", ".old", ".journal", ".journal.tmp", ".lock", ".tmp"];

fn residue(names: impl Iterator<Item = String>) -> Vec<String> {
    names
        .filter(|n| {
            RESIDUE_SUFFIXES.iter().any(|s| n.ends_with(s))
                || n.contains(".tmp")
                || n.contains("marker")
        })
        .collect()
}

fn check_outcome(r: &Refusal, out: &rt_cli::Outcome) -> Result<(), TestCaseError> {
    prop_assert_eq!(out.code.value(), r.exit, "{}", out.stderr);
    prop_assert_eq!(out.stderr.matches("RT-APR-").count(), 1, "{}", out.stderr);
    prop_assert!(
        out.stderr.contains(&format!(" {}: ", r.code)),
        "{}",
        out.stderr
    );
    prop_assert!(out.stdout.is_empty());
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn refused_adoption_leaves_files_unchanged(
        kind in 0u8..CLASSES, n in any::<u32>(), seed in "[a-z]{1,12}", reject in any::<bool>()
    ) {
        let r = refusal(kind, n, &seed, reject);
        let fs = MemFs::default();
        for (name, bytes) in &r.files {
            fs.put(name, bytes);
        }
        let before = fs.snapshot();
        let argv: Vec<OsString> = r.args.iter().map(OsString::from).collect();
        let out = run(&argv, &fs);
        check_outcome(&r, &out)?;
        prop_assert_eq!(fs.snapshot(), before);
        prop_assert!(
            residue(fs.snapshot().keys().map(|p| p.display().to_string())).is_empty()
        );
        prop_assert_eq!(fs.held.get(), 0, "a lock is still held");
    }
}

fn run_args(r: &Refusal, fs: &dyn Fs, root: Option<&Path>) -> rt_cli::Outcome {
    let known = ["rb.json", "ledger.jsonl", "model.json", "store"];
    let argv: Vec<OsString> = r
        .args
        .iter()
        .map(|a| match root {
            Some(root) if known.contains(&a.as_str()) => root.join(a).into_os_string(),
            _ => OsString::from(a),
        })
        .collect();
    run(&argv, fs)
}

fn tree(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            tree(root, &path, into);
        } else {
            let name = path.strip_prefix(root).expect("prefix");
            into.insert(
                name.display().to_string(),
                std::fs::read(&path).expect("read"),
            );
        }
    }
}

/// Runs every class once on the real file system, in the workspace target
/// directory. The real `try_lock` creates the empty advisory lock file next to
/// the ledger; it is the only permitted addition.
#[test]
fn refused_adoption_on_the_real_file_system_leaves_no_residue() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("review-adoption-refusals");
    for kind in 0..CLASSES {
        for n in [0_u32, 1, 2] {
            let r = refusal(kind, n, "seed", n == 1);
            let root = base.join(format!("c{kind}-{n}"));
            let _ = std::fs::remove_dir_all(&root);
            for (name, bytes) in &r.files {
                let path = root.join(name);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(path, bytes).expect("write");
            }
            let mut before = BTreeMap::new();
            tree(&root, &root, &mut before);
            let out = run_args(&r, &StdFs, Some(&root));
            assert_eq!(out.code.value(), r.exit, "class {kind}/{n}: {}", out.stderr);
            assert!(
                out.stderr.contains(&format!(" {}: ", r.code)),
                "class {kind}/{n}: {}",
                out.stderr
            );
            assert!(out.stdout.is_empty());
            let mut after = BTreeMap::new();
            tree(&root, &root, &mut after);
            let lock = after.remove("ledger.jsonl.lock");
            assert!(
                lock.is_none_or(|b| b.is_empty()),
                "class {kind}/{n}: lock file is not empty"
            );
            assert_eq!(after, before, "class {kind}/{n}");
            assert!(residue(after.keys().cloned()).is_empty());
            std::fs::remove_dir_all(&root).expect("cleanup");
        }
    }
}

/// The prospective hash of the adopted revision rests on hand-derived bytes:
/// `GOLDEN_CANONICAL` is the canonical content written out by hand and
/// `GOLDEN_HASH` was computed by `sha256sum`. The CLI must print that hash,
/// accept it, and write it to the ledger.
#[test]
fn adopted_revision_hash_matches_the_hand_derived_golden() {
    assert_eq!(sha256(GOLDEN_CANONICAL.as_bytes()).as_str(), GOLDEN_HASH);
    assert_eq!(canonical("d.b", Some(1), NUM2, DOC_SHA), GOLDEN_CANONICAL);

    let w = who("g");
    let draft = hash_of("d.b", None, NUM2);
    let (batch, _) = batch_bytes(&w, "d.b", draft_json("d.b", NUM2, "rules.md"), &draft);
    let fs = MemFs::default();
    for (name, bytes) in files(&rulebook(&[rev("d.a", 1)]), &ledger(&[]), &batch) {
        fs.put(&name, &bytes);
    }
    let list = run(
        &[
            "review",
            "list",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--source-model",
            "model.json",
            "--proposals",
            "store",
        ]
        .map(OsString::from),
        &fs,
    );
    let printed = String::from_utf8(list.stdout).expect("utf8");
    assert!(
        printed.contains(&format!("  prospective content hash: {GOLDEN_HASH}\n")),
        "{printed}"
    );

    let out = run(
        &args(GOLDEN_HASH, "A. Reviewer", "d.b", 1, None, true).map_os(),
        &fs,
    );
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let ledger_text = String::from_utf8(
        fs.read(Path::new("ledger.jsonl"), u64::MAX)
            .expect("ledger"),
    )
    .expect("utf8");
    assert!(
        ledger_text.contains(&format!(r#""content_hash":"{GOLDEN_HASH}""#)),
        "{ledger_text}"
    );
    assert_eq!(ledger_text.lines().count(), 2);
}

trait MapOs {
    fn map_os(self) -> Vec<OsString>;
}

impl MapOs for Vec<String> {
    fn map_os(self) -> Vec<OsString> {
        self.into_iter().map(OsString::from).collect()
    }
}

fn pending_rulebook() -> (rt_ir::Rulebook, Vec<Batch>) {
    let rb = rt_ir::parse_rulebook(
        rulebook(&[rev("d.a", 1), rev("d.b", 1)]).as_bytes(),
        &rt_foundation::InputId::new("rb.json").expect("id"),
        1 << 22,
        64,
    )
    .expect("rulebook")
    .into_parts()
    .0;
    let draft = hash_of("d.b", None, NUM2);
    let (bytes, _) = batch_bytes(
        &who("e"),
        "d.b",
        draft_json("d.b", NUM2, "rules.md"),
        &draft,
    );
    let batch = rt_proposals::parse_batch(&rt_foundation::InputId::new("b").expect("id"), &bytes)
        .expect("batch");
    (rb, vec![batch])
}

/// Revision exhaustion cannot be built from a Rulebook file, so the Rulebook is
/// changed in memory: its only `d.b` revision is renumbered to the maximum.
#[test]
fn adoption_beyond_the_highest_revision_number_is_refused_with_rt_apr_0210() {
    let (mut rb, batches) = pending_rulebook();
    let last = rb
        .revisions
        .iter_mut()
        .find(|r| r.key.id.as_str() == "d.b")
        .expect("d.b");
    last.key.revision = rt_foundation::RevisionNumber::new(u32::MAX).expect("max");
    let before = rb.clone();
    let pending = rt_proposals::pending_proposals(&batches, &rb);
    assert_eq!(pending.len(), 1);
    let err = rt_review::prospective_adoption(&rb, &pending[0]).expect_err("exhausted");
    assert_eq!(err.code(), rt_foundation::Code::ReviewRevisionExhausted);
    assert_eq!(rb, before);
}

/// One below the limit still adopts, as the highest number the type allows.
#[test]
fn adoption_at_the_last_revision_number_is_not_refused() {
    let (mut rb, batches) = pending_rulebook();
    let last = rb
        .revisions
        .iter_mut()
        .find(|r| r.key.id.as_str() == "d.b")
        .expect("d.b");
    last.key.revision = rt_foundation::RevisionNumber::new(u32::MAX - 1).expect("number");
    let pending = rt_proposals::pending_proposals(&batches, &rb);
    let ok = rt_review::prospective_adoption(&rb, &pending[0]).expect("adoptable");
    assert_eq!(ok.revision.get(), u32::MAX);
}
