//! `check-graph` and `check-tree` (design §3.3, Req 21.8, 21.13, 24.5, 24.9,
//! 24.10, 27.5).
//!
//! Both checks are pure functions of the parsed edge list and the parsed
//! `cargo metadata`. Crates named in the list that are not yet workspace
//! members are ignored.

use crate::edges::{matches_any, Class, EdgeList};
use crate::metadata::Metadata;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Which rule a finding belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// The edge list file is missing.
    EdgeListMissing,
    /// The edge list file is malformed.
    EdgeListMalformed,
    /// The metadata could not be used.
    MetadataMalformed,
    /// A dependency cycle.
    Cycle,
    /// A workspace member without a line in the list.
    UnlistedCrate,
    /// An edge that is not in the list.
    EdgeNotAllowed,
    /// `forbid-class`.
    ForbiddenClass,
    /// `forbid-name`.
    ForbiddenName,
    /// `forbid-transitive`.
    ForbiddenTransitive,
    /// A crate depends on a crate of a later stage.
    StageOrder,
    /// A network or AI crate in the wrong place (`forbid-external`).
    ForbiddenExternal,
    /// An external crate used outside its `allowed-in` set.
    ExternalNotAllowed,
    /// A network or AI crate in a resolved tree (`check-tree`).
    TreeForbidden,
}

impl Rule {
    /// The short name printed in front of a finding.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::EdgeListMissing => "edge-list-missing",
            Self::EdgeListMalformed => "edge-list-malformed",
            Self::MetadataMalformed => "metadata-malformed",
            Self::Cycle => "cycle",
            Self::UnlistedCrate => "unlisted-crate",
            Self::EdgeNotAllowed => "edge-not-allowed",
            Self::ForbiddenClass => "forbid-class",
            Self::ForbiddenName => "forbid-name",
            Self::ForbiddenTransitive => "forbid-transitive",
            Self::StageOrder => "stage-order",
            Self::ForbiddenExternal => "forbid-external",
            Self::ExternalNotAllowed => "external-not-allowed",
            Self::TreeForbidden => "tree-forbidden",
        }
    }
}

/// One failure of a check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The rule that failed.
    pub rule: Rule,
    /// Message naming the crate and edge or path.
    pub message: String,
}

impl Finding {
    fn new(rule: Rule, message: String) -> Self {
        Self { rule, message }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.rule.name(), self.message)
    }
}

type Adjacency = BTreeMap<String, BTreeSet<String>>;

struct Tarjan<'a> {
    adj: &'a Adjacency,
    next: usize,
    index: BTreeMap<&'a str, usize>,
    low: BTreeMap<&'a str, usize>,
    stack: Vec<&'a str>,
    on_stack: BTreeSet<&'a str>,
    components: Vec<Vec<&'a str>>,
}

impl<'a> Tarjan<'a> {
    fn visit(&mut self, v: &'a str) {
        self.index.insert(v, self.next);
        self.low.insert(v, self.next);
        self.next += 1;
        self.stack.push(v);
        self.on_stack.insert(v);
        if let Some(targets) = self.adj.get(v) {
            for w in targets {
                let w = w.as_str();
                if !self.index.contains_key(w) {
                    self.visit(w);
                    let low = self.low.get(w).copied().unwrap_or(usize::MAX);
                    self.lower(v, low);
                } else if self.on_stack.contains(w) {
                    let idx = self.index.get(w).copied().unwrap_or(usize::MAX);
                    self.lower(v, idx);
                }
            }
        }
        if self.low.get(v) == self.index.get(v) {
            let mut component = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack.remove(w);
                component.push(w);
                if w == v {
                    break;
                }
            }
            component.sort_unstable();
            self.components.push(component);
        }
    }

    fn lower(&mut self, v: &'a str, to: usize) {
        if let Some(low) = self.low.get_mut(v) {
            *low = (*low).min(to);
        }
    }
}

/// Strongly connected components of `adj` that contain a cycle, each sorted by
/// name.
fn cyclic_components(adj: &Adjacency) -> Vec<Vec<&str>> {
    let mut t = Tarjan {
        adj,
        next: 0,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        components: Vec::new(),
    };
    for v in adj.keys() {
        if !t.index.contains_key(v.as_str()) {
            t.visit(v);
        }
    }
    let mut out: Vec<Vec<&str>> = t
        .components
        .into_iter()
        .filter(|c| match c.as_slice() {
            [one] => adj.get(*one).is_some_and(|s| s.contains(*one)),
            _ => true,
        })
        .collect();
    out.sort();
    out
}

/// Breadth-first search from `root`, not entering crates for which `skip` is
/// true. Returns the parent of every reached crate.
fn reach(adj: &Adjacency, root: &str, skip: &dyn Fn(&str) -> bool) -> BTreeMap<String, String> {
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([root]);
    let mut queue: VecDeque<&str> = VecDeque::from([root]);
    while let Some(v) = queue.pop_front() {
        for w in adj.get(v).into_iter().flatten() {
            if skip(w) || !seen.insert(w.as_str()) {
                continue;
            }
            parents.insert(w.clone(), v.to_string());
            queue.push_back(w);
        }
    }
    parents
}

fn path_to(parents: &BTreeMap<String, String>, root: &str, node: &str) -> String {
    let mut path = vec![node.to_string()];
    let mut at = node;
    while let Some(parent) = parents.get(at) {
        path.push(parent.clone());
        at = parent;
        if at == root {
            break;
        }
    }
    path.reverse();
    path.join(" -> ")
}

fn member_adjacency(md: &Metadata) -> Adjacency {
    md.members
        .iter()
        .map(|(name, deps)| {
            let targets = deps
                .iter()
                .filter(|d| md.members.contains_key(&d.name))
                .map(|d| d.name.clone())
                .collect();
            (name.clone(), targets)
        })
        .collect()
}

/// Runs every `check-graph` rule on the workspace.
#[must_use]
pub fn check_graph(list: &EdgeList, md: &Metadata) -> Vec<Finding> {
    let mut out = Vec::new();
    let adj = member_adjacency(md);

    for component in cyclic_components(&adj) {
        let members: BTreeSet<&str> = component.iter().copied().collect();
        let mut edges = Vec::new();
        for from in &component {
            for to in adj.get(*from).into_iter().flatten() {
                if members.contains(to.as_str()) {
                    edges.push(format!("{from} -> {to}"));
                }
            }
        }
        out.push(Finding::new(
            Rule::Cycle,
            format!(
                "dependency cycle among crates {}: edges {}",
                component.join(", "),
                edges.join(", ")
            ),
        ));
    }

    for (from, deps) in &md.members {
        let entry = list.crates.get(from);
        if entry.is_none() {
            out.push(Finding::new(
                Rule::UnlistedCrate,
                format!("workspace crate '{from}' has no line in tools/allowed-edges.v1"),
            ));
        }
        for dep in deps {
            let to = dep.name.as_str();
            if md.members.contains_key(to) {
                check_edge(list, from, to, entry.map(|e| e.class), &mut out);
                if let Some(e) = entry {
                    if !e.deps.contains(to) {
                        out.push(Finding::new(
                            Rule::EdgeNotAllowed,
                            format!("edge {from} -> {to} is not in tools/allowed-edges.v1"),
                        ));
                    }
                }
            } else {
                check_external(list, from, to, &mut out);
            }
        }
    }

    for rule in &list.forbid_transitive {
        for from in md.members.keys().filter(|n| matches_any(&rule.from, n)) {
            let parents = reach(&adj, from, &|_| false);
            for to in parents.keys().filter(|n| matches_any(&rule.to, n)) {
                out.push(Finding::new(
                    Rule::ForbiddenTransitive,
                    format!(
                        "crate {from} depends on forbidden crate {to} transitively: {}",
                        path_to(&parents, from, to)
                    ),
                ));
            }
        }
    }
    out.sort_by(|a, b| (a.rule, &a.message).cmp(&(b.rule, &b.message)));
    out
}

fn check_edge(
    list: &EdgeList,
    from: &str,
    to: &str,
    from_class: Option<Class>,
    out: &mut Vec<Finding>,
) {
    let to_class = list.crates.get(to).map(|e| e.class);
    if let (Some(f), Some(t)) = (from_class, to_class) {
        if list.forbid_class.contains(&(f, t)) {
            out.push(Finding::new(
                Rule::ForbiddenClass,
                format!(
                    "edge {from} -> {to} is forbidden: class {} must not depend on class {}",
                    f.word(),
                    t.word()
                ),
            ));
        }
    }
    for rule in &list.forbid_name {
        if matches_any(&rule.from, from) && matches_any(&rule.to, to) {
            out.push(Finding::new(
                Rule::ForbiddenName,
                format!("edge {from} -> {to} is forbidden by a forbid-name rule"),
            ));
        }
    }
    if let (Some(a), Some(b)) = (list.stages.get(from), list.stages.get(to)) {
        if b > a {
            out.push(Finding::new(
                Rule::StageOrder,
                format!("edge {from} (stage {a}) -> {to} (stage {b}) depends on a later stage"),
            ));
        }
    }
}

fn check_external(list: &EdgeList, from: &str, to: &str, out: &mut Vec<Finding>) {
    let forbidden = &list.forbid_external;
    if forbidden.names.contains(to) && !forbidden.except_under.contains(from) {
        out.push(Finding::new(
            Rule::ForbiddenExternal,
            format!("crate {from} depends on network or AI crate {to}"),
        ));
    }
    if let Some(allowed) = list.external.get(to) {
        if !allowed.contains(from) {
            out.push(Finding::new(
                Rule::ExternalNotAllowed,
                format!(
                    "crate {from} depends on external crate {to}, allowed only in: {}",
                    join(allowed)
                ),
            ));
        }
    }
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs every `check-tree` rule: the full resolved tree (normal, build, dev,
/// all features) of `rt-cli`, `rt-ai-mock` and each core crate must hold no
/// network or AI crate; each application's tree must hold no
/// `forbid-transitive` crate; the trees of `rt-cli` and `rt-ai-tool` must hold
/// no application layer.
#[must_use]
pub fn check_tree(list: &EdgeList, md: &Metadata) -> Vec<Finding> {
    let Some(graph) = &md.resolved else {
        return vec![Finding::new(
            Rule::MetadataMalformed,
            "cargo metadata has no resolve section; the dependency tree cannot be checked"
                .to_string(),
        )];
    };
    let mut out = Vec::new();
    let forbidden = &list.forbid_external;
    let skip = |name: &str| forbidden.except_under.contains(name);
    let present = |name: &String| md.members.contains_key(name) && graph.contains_key(name);

    let mut external_roots: BTreeSet<&String> = list.of_class(Class::Core).collect();
    external_roots.extend(
        list.crates
            .keys()
            .filter(|n| *n == "rt-cli" || *n == "rt-ai-mock"),
    );
    for root in external_roots.into_iter().filter(|n| present(n)) {
        let parents = reach(graph, root, &skip);
        for node in parents.keys().filter(|n| forbidden.names.contains(*n)) {
            out.push(Finding::new(
                Rule::TreeForbidden,
                format!(
                    "tree of {root} contains network or AI crate {node}: {}",
                    path_to(&parents, root, node)
                ),
            ));
        }
    }

    for root in list.of_class(Class::App).filter(|n| present(n)) {
        let parents = reach(graph, root, &skip);
        for node in parents.keys().filter(|n| forbidden.names.contains(*n)) {
            out.push(Finding::new(
                Rule::TreeForbidden,
                format!(
                    "tree of {root} contains network or AI crate {node}: {}",
                    path_to(&parents, root, node)
                ),
            ));
        }
        for rule in list
            .forbid_transitive
            .iter()
            .filter(|r| matches_any(&r.from, root))
        {
            for node in parents.keys().filter(|n| matches_any(&rule.to, n)) {
                out.push(Finding::new(
                    Rule::TreeForbidden,
                    format!(
                        "tree of {root} contains forbidden crate {node}: {}",
                        path_to(&parents, root, node)
                    ),
                ));
            }
        }
    }

    let apps: BTreeSet<&String> = list.of_class(Class::App).collect();
    for root in ["rt-cli", "rt-ai-tool"]
        .map(String::from)
        .iter()
        .filter(|n| present(n))
    {
        let parents = reach(graph, root, &|_| false);
        for node in parents.keys().filter(|n| apps.contains(*n)) {
            out.push(Finding::new(
                Rule::TreeForbidden,
                format!(
                    "tree of {root} contains application layer {node}: {}",
                    path_to(&parents, root, node)
                ),
            ));
        }
    }
    out.sort_by(|a, b| (a.rule, &a.message).cmp(&(b.rule, &b.message)));
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges;
    use crate::metadata::parse_metadata;

    const LIST: &str = "# format: ruletrace-allowed-edges v1
crate rt-foundation class core deps -
crate rt-json class core deps rt-foundation
crate rt-ir class core deps rt-foundation rt-json
crate rt-validate class core deps rt-foundation rt-ir
crate rt-engine class core deps rt-foundation rt-json
crate rt-explain class core deps rt-foundation rt-engine
crate rt-cli class core deps rt-foundation rt-json rt-ir rt-validate rt-engine
crate rt-review class core deps rt-foundation rt-ir
crate rt-compile class core deps rt-foundation rt-validate
crate rt-ai-adapter class aux deps rt-foundation rt-json
crate rt-ai-tool class aux deps rt-ai-adapter rt-foundation rt-validate
crate prize-check class app deps rt-foundation rt-engine
crate rt-hash class core deps -
stage rt-ir 3
stage rt-validate 4
stage rt-compile 5
forbid-class core -> aux
forbid-class core -> app
forbid-name rt-ai-* -> rt-compile rt-engine rt-review rt-cli
forbid-name rt-engine -> rt-explain
forbid-transitive prize-check -> rt-cli rt-ai-*
forbid-external ureq rustls ring except-under rt-ai-openai
external sha2 allowed-in rt-hash
";

    type Spec<'a> = &'a [(&'a str, &'a [(&'a str, &'a str)])];

    /// Builds cargo metadata JSON: `members` are workspace crates, `externals`
    /// other packages.
    fn metadata(members: Spec<'_>, externals: Spec<'_>) -> Metadata {
        let all: Vec<_> = members.iter().chain(externals.iter()).collect();
        let package = |name: &str, deps: &[(&str, &str)]| {
            let deps: Vec<String> = deps
                .iter()
                .map(|(d, k)| {
                    let kind = if k.is_empty() {
                        "null".to_string()
                    } else {
                        format!("\"{k}\"")
                    };
                    format!("{{\"name\":\"{d}\",\"kind\":{kind}}}")
                })
                .collect();
            format!(
                "{{\"name\":\"{name}\",\"id\":\"id-{name}\",\"dependencies\":[{}]}}",
                deps.join(",")
            )
        };
        let packages: Vec<String> = all.iter().map(|(n, d)| package(n, d)).collect();
        let ids: Vec<String> = members.iter().map(|(n, _)| format!("\"id-{n}\"")).collect();
        let nodes: Vec<String> = all
            .iter()
            .map(|(n, d)| {
                let deps: Vec<String> = d
                    .iter()
                    .map(|(x, _)| format!("{{\"pkg\":\"id-{x}\"}}"))
                    .collect();
                format!("{{\"id\":\"id-{n}\",\"deps\":[{}]}}", deps.join(","))
            })
            .collect();
        let json = format!(
            "{{\"packages\":[{}],\"workspace_members\":[{}],\"resolve\":{{\"nodes\":[{}]}}}}",
            packages.join(","),
            ids.join(","),
            nodes.join(",")
        );
        parse_metadata(json.as_bytes()).unwrap()
    }

    fn list() -> EdgeList {
        edges::parse(LIST).unwrap()
    }

    /// A conforming workspace; negative tests add one violation each.
    fn base() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
        vec![
            ("rt-foundation", vec![]),
            ("rt-json", vec![("rt-foundation", "")]),
            ("rt-ir", vec![("rt-foundation", ""), ("rt-json", "")]),
            ("rt-validate", vec![("rt-ir", ""), ("rt-foundation", "")]),
            ("rt-engine", vec![("rt-foundation", "")]),
            ("rt-hash", vec![("sha2", "")]),
        ]
    }

    fn run(
        mut members: Vec<(&'static str, Vec<(&'static str, &'static str)>)>,
        change: impl FnOnce(&mut Vec<(&'static str, Vec<(&'static str, &'static str)>)>),
        externals: Spec<'_>,
    ) -> (Vec<Finding>, Vec<Finding>) {
        change(&mut members);
        let owned: Vec<(&str, &[(&str, &str)])> =
            members.iter().map(|(n, d)| (*n, d.as_slice())).collect();
        let md = metadata(&owned, externals);
        let l = list();
        (check_graph(&l, &md), check_tree(&l, &md))
    }

    const SHA: Spec<'static> = &[("sha2", &[])];

    #[test]
    fn conforming_workspace_passes() {
        let (g, t) = run(base(), |_| {}, SHA);
        assert_eq!((g, t), (vec![], vec![]));
    }

    #[test]
    fn unlisted_crates_in_list_are_ignored() {
        // rt-cli, rt-review, ... are in the list but not members above.
        let (g, _) = run(base(), |_| {}, SHA);
        assert!(g.is_empty());
    }

    #[test]
    fn cycle_names_crates_and_edges() {
        let (g, _) = run(
            base(),
            |m| {
                m.push(("rt-explain", vec![("rt-engine", "")]));
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-engine") {
                    e.1.push(("rt-explain", "dev"));
                }
            },
            SHA,
        );
        let cycle = g.iter().find(|f| f.rule == Rule::Cycle).unwrap();
        assert!(cycle.message.contains("rt-engine -> rt-explain"), "{cycle}");
        assert!(cycle.message.contains("rt-explain -> rt-engine"), "{cycle}");
    }

    #[test]
    fn edge_not_in_list_names_edge() {
        let (g, _) = run(
            base(),
            |m| {
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-json") {
                    e.1.push(("rt-hash", ""));
                }
            },
            SHA,
        );
        assert!(g.iter().any(
            |f| f.rule == Rule::EdgeNotAllowed && f.message.contains("edge rt-json -> rt-hash")
        ));
    }

    #[test]
    fn core_to_aux_is_forbidden_by_class() {
        let (g, _) = run(
            base(),
            |m| {
                m.push(("rt-ai-adapter", vec![]));
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-json") {
                    e.1.push(("rt-ai-adapter", ""));
                }
            },
            SHA,
        );
        assert!(g
            .iter()
            .any(|f| f.rule == Rule::ForbiddenClass
                && f.message.contains("rt-json -> rt-ai-adapter")));
    }

    #[test]
    fn ai_crate_depending_on_review_is_forbidden_by_name() {
        let (g, _) = run(
            base(),
            |m| {
                m.push(("rt-review", vec![]));
                m.push(("rt-ai-tool", vec![("rt-review", "")]));
            },
            SHA,
        );
        assert!(
            g.iter()
                .any(|f| f.rule == Rule::ForbiddenName
                    && f.message.contains("rt-ai-tool -> rt-review")),
            "{g:?}"
        );
    }

    #[test]
    fn engine_to_explain_and_later_stage_are_forbidden() {
        let (g, _) = run(
            base(),
            |m| {
                m.push(("rt-explain", vec![]));
                m.push(("rt-compile", vec![]));
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-engine") {
                    e.1.push(("rt-explain", ""));
                }
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-validate") {
                    e.1.push(("rt-compile", ""));
                }
            },
            SHA,
        );
        assert!(g.iter().any(
            |f| f.rule == Rule::ForbiddenName && f.message.contains("rt-engine -> rt-explain")
        ));
        assert!(g.iter().any(|f| f.rule == Rule::StageOrder
            && f.message
                .contains("rt-validate (stage 4) -> rt-compile (stage 5)")));
    }

    #[test]
    fn core_crate_depending_on_ureq_fails_graph_and_tree() {
        let (g, t) = run(
            base(),
            |m| {
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-json") {
                    e.1.push(("ureq", ""));
                }
            },
            &[("sha2", &[]), ("ureq", &[])],
        );
        assert!(g.iter().any(|f| f.rule == Rule::ForbiddenExternal
            && f.message.contains("rt-json")
            && f.message.contains("ureq")));
        assert!(
            t.iter().any(|f| f.rule == Rule::TreeForbidden
                && f.message.contains(
                    "tree of rt-ir contains network or AI crate ureq: rt-ir -> rt-json -> ureq"
                )),
            "{t:?}"
        );
    }

    #[test]
    fn transitive_network_crate_is_found_in_tree_with_path() {
        let (g, t) = run(base(), |_| {}, &[("sha2", &[("ring", "")]), ("ring", &[])]);
        assert!(g.is_empty(), "{g:?}");
        assert!(
            t.iter()
                .any(|f| f.message.contains("rt-hash -> sha2 -> ring")),
            "{t:?}"
        );
    }

    #[test]
    fn forbidden_transitive_for_application() {
        let (g, t) = run(
            base(),
            |m| {
                m.push(("prize-check", vec![("rt-engine", "")]));
                m.push(("rt-cli", vec![]));
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-engine") {
                    e.1.push(("rt-cli", ""));
                }
            },
            SHA,
        );
        assert!(
            g.iter().any(|f| f.rule == Rule::ForbiddenTransitive
                && f.message.contains("prize-check -> rt-engine -> rt-cli")),
            "{g:?}"
        );
        assert!(t.iter().any(|f| f
            .message
            .contains("tree of prize-check contains forbidden crate rt-cli")));
    }

    #[test]
    fn external_outside_allowed_set_fails() {
        let (g, _) = run(
            base(),
            |m| {
                if let Some(e) = m.iter_mut().find(|(n, _)| *n == "rt-json") {
                    e.1.push(("sha2", ""));
                }
            },
            SHA,
        );
        assert!(g.iter().any(|f| f.rule == Rule::ExternalNotAllowed
            && f.message.contains("rt-json")
            && f.message.contains("sha2")));
    }

    #[test]
    fn unlisted_member_fails() {
        let (g, _) = run(base(), |m| m.push(("rt-surprise", vec![])), SHA);
        assert!(g
            .iter()
            .any(|f| f.rule == Rule::UnlistedCrate && f.message.contains("rt-surprise")));
    }

    #[test]
    fn missing_edge_list_is_its_own_failure() {
        let e =
            edges::load(std::path::Path::new("/nonexistent/tools/allowed-edges.v1")).unwrap_err();
        match e {
            edges::EdgeListError::Missing(m) => assert!(m.contains("allowed-edges.v1")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_resolve_fails_tree() {
        let md = parse_metadata(br#"{"packages":[],"workspace_members":[]}"#).unwrap();
        let f = check_tree(&list(), &md);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule, Rule::MetadataMalformed);
    }

    #[test]
    fn real_edge_list_parses_and_covers_design_crates() {
        let l = edges::parse(include_str!("../../../tools/allowed-edges.v1")).unwrap();
        for name in [
            "rt-hash",
            "rt-cli",
            "rt-ai-openai",
            "prize-check",
            "rt-tests-ai",
            "rt-xtask",
        ] {
            assert!(l.crates.contains_key(name), "{name}");
        }
        assert!(l.forbid_external.names.contains("ureq"));
    }
}
