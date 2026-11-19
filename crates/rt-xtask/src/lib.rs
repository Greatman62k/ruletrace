//! Repository checks and generators (development tooling, not a delivered
//! executable).
#![forbid(unsafe_code)]

pub mod api;
pub mod app_layers;
pub mod catalogue;
pub mod decisions;
pub mod edges;
pub mod executables;
pub mod export;
pub mod graph;
pub mod justification;
pub mod metadata;
pub mod scan;
pub mod tokens;

use std::path::Path;

/// Which graph check to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphCheck {
    /// `check-graph`.
    Graph,
    /// `check-tree`.
    Tree,
}

/// Runs a graph check on the repository at `root`, given the bytes of
/// `cargo metadata`. Returns the number of workspace crates checked.
///
/// # Errors
/// One line per finding.
pub fn run_graph_check(
    root: &Path,
    metadata_bytes: &[u8],
    which: GraphCheck,
) -> Result<usize, String> {
    let list = edges::load(&root.join(edges::EDGES_PATH)).map_err(|e| {
        let (rule, message) = match e {
            edges::EdgeListError::Missing(m) => (graph::Rule::EdgeListMissing, m),
            edges::EdgeListError::Malformed(m) => (graph::Rule::EdgeListMalformed, m),
        };
        graph::Finding { rule, message }.to_string()
    })?;
    let md = metadata::parse_metadata(metadata_bytes).map_err(|m| {
        graph::Finding {
            rule: graph::Rule::MetadataMalformed,
            message: m,
        }
        .to_string()
    })?;
    let findings = match which {
        GraphCheck::Graph => graph::check_graph(&list, &md),
        GraphCheck::Tree => graph::check_tree(&list, &md),
    };
    if findings.is_empty() {
        Ok(md.members.len())
    } else {
        let lines: Vec<String> = findings.iter().map(ToString::to_string).collect();
        Err(lines.join("\n"))
    }
}

/// Runs `check-decisions` given the bytes of `cargo metadata`. Returns the
/// number of gated crates that are workspace members.
///
/// # Errors
/// One line per finding.
pub fn run_check_decisions(root: &Path, metadata_bytes: &[u8]) -> Result<usize, String> {
    let md = metadata::parse_metadata(metadata_bytes).map_err(|m| {
        graph::Finding {
            rule: graph::Rule::MetadataMalformed,
            message: m,
        }
        .to_string()
    })?;
    let members = md.members.keys().cloned().collect();
    decisions::run(root, &members)
}
