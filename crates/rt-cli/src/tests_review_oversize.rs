//! A Proposal_Store batch file above the limit is `RT-LIM-0001` with the true
//! size of the file in the review path (Req 19.3, 19.8, 19.9).
//!
//! Expected sizes and the limit are written out here and are not computed by
//! the code under test: the limit is 16 MiB, `16777216` bytes.

use super::commit_tests::{adoption_fs, approve, fixture};
use super::fault_tests::FaultFs;
use super::*;
use crate::io::target_scratch::Dir;

const LIMIT: u64 = 16_777_216;
const BATCH: &str = "store/batches/req.json";

fn store_with(bytes: Vec<u8>) -> FaultFs {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let mut files = fs.snapshot();
    files.insert(PathBuf::from(BATCH), bytes);
    fs.replace_all(files);
    fs
}

fn list(extra: &[&str]) -> Vec<OsString> {
    let mut a = vec![
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
    ];
    a.extend_from_slice(extra);
    os(&a)
}

fn expected(path: &str, size: u64) -> String {
    format!(
        "input file '{path}' is {size} bytes, larger than the effective limit of \
         16777216 bytes"
    )
}

fn assert_only_size_refusal(out: &Outcome, path: &str, size: u64) {
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{}", out.stderr);
    assert!(
        out.stderr
            .starts_with(&format!("error RT-LIM-0001: {}", expected(path, size))),
        "{}",
        out.stderr
    );
}

#[test]
fn list_reports_a_batch_one_byte_over_the_limit_with_its_size() {
    let fs = store_with(vec![b'x'; usize::try_from(LIMIT + 1).unwrap()]);
    let out = run(&list(&["--proposals", "store"]), &fs);
    assert_only_size_refusal(&out, BATCH, 16_777_217);
    let reads_of_batch = fs
        .log()
        .iter()
        .filter(|op| op.kind == "read" && op.path == Path::new(BATCH))
        .count();
    assert_eq!(reads_of_batch, 0, "an oversize batch is not read");
}

#[test]
fn list_reports_a_much_larger_batch_with_its_true_size() {
    let size = 3 * LIMIT + 5;
    let fs = store_with(vec![b'x'; usize::try_from(size).unwrap()]);
    let out = run(&list(&["--proposals", "store"]), &fs);
    assert_only_size_refusal(&out, BATCH, 50_331_653);
}

#[test]
fn list_parses_a_batch_of_exactly_the_limit() {
    let fs = store_with(vec![b'x'; usize::try_from(LIMIT).unwrap()]);
    let out = run(&list(&["--proposals", "store"]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0213"), "{}", out.stderr);
    assert!(!out.stderr.contains("RT-LIM-"), "{}", out.stderr);
    assert!(!out.stderr.contains("larger than"), "{}", out.stderr);
}

#[test]
fn approve_reports_an_oversize_batch_and_changes_nothing() {
    let fx = fixture();
    let fs = store_with(vec![b'x'; usize::try_from(3 * LIMIT + 5).unwrap()]);
    let ledger = fs.snapshot()[Path::new("ledger.jsonl")].clone();
    let rulebook = fs.snapshot()[Path::new("rb.json")].clone();
    let out = run(&approve(&fx), &fs);
    assert_only_size_refusal(&out, BATCH, 50_331_653);
    assert_eq!(fs.snapshot()[Path::new("ledger.jsonl")], ledger);
    assert_eq!(fs.snapshot()[Path::new("rb.json")], rulebook);
}

#[test]
fn the_size_comes_from_file_metadata_on_the_real_file_system() {
    let fx = fixture();
    for size in [LIMIT + 1, 5 * LIMIT + 3] {
        let dir = Dir::new("oversize-batch");
        dir.load(&adoption_fs(&fx, true).snapshot());
        dir.sparse(BATCH, size);
        let args: Vec<OsString> = list(&["--proposals", "store"])
            .into_iter()
            .map(|a| match a.to_str() {
                Some("rb.json" | "ledger.jsonl" | "model.json" | "store") => {
                    dir.0.join(a).into_os_string()
                }
                _ => a,
            })
            .collect();
        let out = run(&args, &StdFs);
        let shown = dir.0.join(BATCH).display().to_string();
        assert_only_size_refusal(&out, &shown, size);
    }
}

#[test]
fn a_failing_size_query_is_an_unreadable_batch_and_not_a_size_fault() {
    let fx = fixture();
    let source = adoption_fs(&fx, true).snapshot();
    let fs = MemFs::default();
    *fs.files.borrow_mut() = source;
    fs.fail_len.set(true);
    let out = run(&list(&["--proposals", "store"]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{}", out.stderr);
    assert!(
        out.stderr.starts_with(
            "error RT-APR-0213: the batch 'store/batches/req.json' cannot be read: \
             injected len failure"
        ),
        "{}",
        out.stderr
    );
}
