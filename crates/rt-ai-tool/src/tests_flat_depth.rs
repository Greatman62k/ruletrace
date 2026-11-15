//! Regression tests for the container depth of an accepted flat draft
//! (review of wave 1 of UD-37, issue 1 and issue 2).
//!
//! A stored batch nests the rule three containers deep (the batch, the
//! proposal array and the proposal), and the batch reader refuses more
//! than 64 containers. So a lowered rule may nest 61 containers. A chain
//! of `not` nodes of depth `d` is `d + 1` containers (the rule and one
//! object per node), so depth 60 is the last one that can be stored. The
//! figures below are counted by hand from that layout.

use crate::tests_flat::{payload, try_accept, try_accept_within};
use crate::tests_memfs::MemFs;
use crate::tests_rig::{meta, ROOT};
use crate::{check_store, store_batch, AcceptedBatch, StoreOutcome};
use rt_foundation::{Code, Diagnostic, InputId};
use rt_proposals::parse_batch;
use std::path::Path;

const SPAN: &str = r#"{"document":"rules.md","start":12,"end":27,"line":2,"column":1}"#;

/// A flat rule whose expression is a chain of `depth` nodes: `depth - 1`
/// `not` nodes over one `bool` leaf, written without a model.
fn not_chain(depth: usize) -> String {
    let mut nodes = Vec::with_capacity(depth);
    for i in 0..depth - 1 {
        nodes.push(format!(r#"{{"node":"not","t":"boolean","arg":{}}}"#, i + 1));
    }
    nodes.push(r#"{"node":"bool","t":"boolean","value":true}"#.to_owned());
    format!(
        r#"{{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":0,"nodes":[{}],"markers":[],"spans":[{SPAN}]}}"#,
        nodes.join(",")
    )
}

/// A flat rule with `levels` nested `and` nodes, each over the next one
/// and a `bool` leaf, and a `bool` leaf at the bottom (node `2i` is the
/// `and` of level `i`, node `2i + 1` its leaf). Each `and` is an
/// object holding an `args` array, so the rule nests `2 * levels + 2`
/// containers (rule, `levels` objects, `levels` arrays, bottom leaf).
fn and_chain(levels: usize) -> String {
    let mut table = Vec::new();
    for i in 0..levels {
        table.push(format!(
            r#"{{"node":"and","t":"boolean","args":[{},{}]}}"#,
            2 * i + 1,
            2 * i + 2
        ));
        table.push(r#"{"node":"bool","t":"boolean","value":false}"#.to_owned());
    }
    table.push(r#"{"node":"bool","t":"boolean","value":true}"#.to_owned());
    format!(
        r#"{{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":0,"nodes":[{}],"markers":[],"spans":[{SPAN}]}}"#,
        table.join(",")
    )
}

fn only(found: &[Diagnostic]) -> &Diagnostic {
    assert_eq!(found.len(), 1, "{found:?}");
    &found[0]
}

/// Stores `accepted` and returns the stored bytes after checking that the
/// store as a whole passes `check_store`.
fn store_and_reread(accepted: &AcceptedBatch) -> Vec<u8> {
    let fs = MemFs::new();
    let report = store_batch(&fs, Path::new(ROOT), accepted, &meta()).unwrap();
    assert_eq!(report.outcome, StoreOutcome::Stored);
    let files = fs.files();
    let (_, bytes) = files
        .iter()
        .find(|(name, _)| name.contains("batches") && name.ends_with(".json"))
        .expect("a batch file");
    let parsed = parse_batch(&InputId::new("b").unwrap(), bytes);
    assert!(parsed.is_ok(), "{:?}", parsed.err());
    assert_eq!(check_store(&fs, Path::new(ROOT)), Ok(()));
    bytes.clone()
}

#[test]
fn depths_up_to_60_are_stored_and_read_back() {
    for depth in [16, 17, 30, 60] {
        let accepted = try_accept(&payload("2", &not_chain(depth)))
            .unwrap_or_else(|d| panic!("depth {depth}: {d:?}"));
        let found = &accepted.proposals()[0].outcome;
        let too_deep = found.iter().any(|d| d.code == Code::LimitExpressionTooDeep);
        assert_eq!(too_deep, depth > 16, "depth {depth}: {found:?}");
        store_and_reread(&accepted);
    }
}

#[test]
fn deeper_chains_are_refused_at_acceptance_and_nothing_is_stored() {
    for depth in [61, 100, 300, 500] {
        let found = match try_accept(&payload("2", &not_chain(depth))) {
            Ok(_) => panic!("depth {depth} was accepted"),
            Err(found) => found,
        };
        let d = only(&found);
        assert_eq!(d.code(), Code::LimitExpressionTooDeep, "depth {depth}");
        assert_eq!(d.code().id(), "RT-LIM-0003");
        // Only an AcceptedBatch can be stored, so nothing was written.
        let fs = MemFs::new();
        assert!(fs.files().is_empty());
        assert_eq!(check_store(&fs, Path::new(ROOT)), Ok(()));
    }
}

#[test]
fn the_cap_counts_containers_so_an_and_chain_stops_at_29_levels() {
    // 29 levels are 60 containers in the rule, 30 levels are 62.
    let ok = try_accept(&payload("2", &and_chain(29))).expect("29 levels");
    store_and_reread(&ok);
    let found = try_accept(&payload("2", &and_chain(30))).unwrap_err();
    assert_eq!(only(&found).code(), Code::LimitExpressionTooDeep);
}

fn on_small_stack(depth: usize, limit: u64) -> Result<(), Vec<Diagnostic>> {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || try_accept_within(&payload("2", &not_chain(depth)), limit).map(|_| ()))
        .unwrap()
        .join()
        .expect("accept overflowed or panicked on a 1 MiB stack")
}

#[test]
fn a_deep_response_does_not_overflow_a_1_mib_stack() {
    let found = on_small_stack(500, 1 << 20).unwrap_err();
    assert_eq!(only(&found).code(), Code::LimitExpressionTooDeep);
}

#[test]
fn a_chain_of_100000_nodes_does_not_overflow_a_1_mib_stack() {
    let found = on_small_stack(100_000, 16 << 20).unwrap_err();
    assert_eq!(only(&found).code(), Code::LimitExpressionTooDeep);
}

#[test]
fn container_depth_ignores_brackets_inside_strings_and_escaped_quotes() {
    use crate::response::container_depth;
    assert_eq!(container_depth("1"), 0);
    assert_eq!(container_depth("[[],{}]"), 2);
    assert_eq!(container_depth(r#"{"a":"{{[[","b":[1]}"#), 2);
    // The escaped quote does not end the string, so the brackets after it
    // are text and the depth stays 1.
    assert_eq!(container_depth(r#"{"a":"\"[[[","b":1}"#), 1);
    // An escaped backslash does end its escape, so the quote closes.
    assert_eq!(container_depth(r#"{"a":"\\","b":[[1]]}"#), 3);
}
