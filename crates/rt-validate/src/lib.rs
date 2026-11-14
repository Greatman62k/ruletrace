//! Semantic validation of a Rulebook (design §3.6.1, Req 6, 14.6, 14.7, 18.5).
//!
//! This crate implements passes 1 to 11: structure, sources, names, types,
//! numeric declarations, boundary, markers, the reference graph, conflicts, the
//! approval gate and the Candidate_Clause check. [`validate`] is the stage
//! function and [`validate_content`] the content-only entry point (passes 1 to
//! 8).
//!
//! * Every finding is collected. Nothing short-circuits (Req 6.11).
//! * A name that does not resolve gets the internal type `Error`, which
//!   suppresses downstream type findings on that node, so one defect gives one
//!   Diagnostic.
//! * `Origin` is not an argument of any check. A manual and an AI-origin
//!   revision with equal content get equal findings (Req 18.5).
//! * A missing or mismatched Source_Model is `RT-VAL-0017` or `RT-VAL-0018`,
//!   which the catalogue classes as exit 2 (UD-14).
//! * Spans become `Location::Source` only after `SourceSpan::new` verified them
//!   against the Source_Model. Otherwise the Diagnostic points at the
//!   Document_Location (Req 6.24, 7.15).
//! * The crate executes no rule and reads no data (Req 6.15).
//!
//! Which revisions are validated in full: the Effective revisions and the
//! unresolved (Proposed) ones. Rejected revisions, superseded Approved
//! revisions and Proposed revisions that a higher decided revision supersedes
//! get only the structural checks of pass 1 (Req 6.25, 25.12). A `ref` resolves
//! to the Effective revision of a rule or, when the rule has none, to its
//! highest unresolved revision, so that a Proposed rule that refers to another
//! Proposed rule is not blocked by a second Diagnostic besides `RT-APR-0001`
//! (Req 26.12). It never resolves to a Rejected or superseded revision.
//!
//! Choices the design leaves open:
//!
//! * `and` or `or` with fewer than 2 or more than 64 arguments, and a marker
//!   outside its supported shape, are `RT-VAL-0003` (outside the supported
//!   boundary).
//! * `rank_reference` is `RT-VAL-0003` ("rank reference") until UD-32 selects
//!   Rank_Reference.
//! * A `sum`, `min` or `max` without an element field is `RT-VAL-0002`.
#![forbid(unsafe_code)]

mod approval;
mod conflicts;
mod ctx;
mod graph;
mod markers;
mod structure;
mod walk;

use ctx::{Decls, Site};
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId, RuleRef,
    MAX_REVISIONS_PER_RULEBOOK, MAX_RULES_PER_RULEBOOK,
};
use rt_ir::{Name, RuleContent, RuleKey, Rulebook};
use rt_ledger::{derive_statuses, effective_revisions, unresolved, ApprovalLedger};
use rt_srcmodel::{CandidateClause, SourceModels};
use std::collections::{BTreeMap, BTreeSet};
use walk::{kind_type, Deps, TargetInfo, Targets, Walk};

/// Input identifier that names the Rule_IR input in rulebook-wide
/// Diagnostics. The Rulebook does not carry the path it was read from, so the
/// logical name is used, as `ledger` is for ledger records.
pub const RULEBOOK_INPUT: &str = "rulebook";

/// The whole-input Location of the Rulebook: an Input_Location with no line,
/// column or field (Req 5.7, 7.14). `code` is the reason of the
/// No_Location fallback, used only when the identifier is refused.
#[must_use]
pub fn rulebook_input_location(code: Code) -> Location {
    match rt_foundation::InputId::new(RULEBOOK_INPUT) {
        Ok(input) => Location::Input(rt_foundation::InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        }),
        Err(_) => Location::None(code),
    }
}

/// A proposed rule that is checked without a ledger: its identifier and its
/// content. The Revision_Number is assigned at adoption, and the Origin is not
/// part of the draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDraft {
    /// The rule identifier.
    pub id: RuleId,
    /// The Rule_Content.
    pub content: RuleContent,
}

/// A conflict class that pass 9 checks (design §3.6.1). Ordered as the classes
/// are recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConflictClass {
    /// `default` rules on one input with unequal literals or different types.
    ContradictoryDefault,
    /// Two or more ranking rules with one Target.
    DuplicateRankKey,
    /// Two or more value rules with one Target.
    DuplicateTarget,
    /// A value or ranking Target equal to a declared input name.
    InputShadowing,
}

impl ConflictClass {
    /// The identifier recorded in a Validation_Result.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ConflictClass::ContradictoryDefault => "ContradictoryDefault",
            ConflictClass::DuplicateRankKey => "DuplicateRankKey",
            ConflictClass::DuplicateTarget => "DuplicateTarget",
            ConflictClass::InputShadowing => "InputShadowing",
        }
    }
}

/// What a rule depends on (Req 6.10): the input fields it reads directly or
/// through referenced rules, and the collections it aggregates, each once, in
/// ascending name order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleMetadata {
    input_fields: Vec<Name>,
    collections: Vec<Name>,
}

impl RuleMetadata {
    /// Metadata read back from a stored Validation_Result. Both lists are
    /// sorted and made distinct.
    #[must_use]
    pub fn new(mut input_fields: Vec<Name>, mut collections: Vec<Name>) -> Self {
        input_fields.sort();
        input_fields.dedup();
        collections.sort();
        collections.dedup();
        Self {
            input_fields,
            collections,
        }
    }

    /// Input fields, ascending, without repetition.
    #[must_use]
    pub fn input_fields(&self) -> &[Name] {
        &self.input_fields
    }

    /// Collections, ascending, without repetition.
    #[must_use]
    pub fn collections(&self) -> &[Name] {
        &self.collections
    }
}

/// The result of validating a Rulebook against a ledger. Fields are private;
/// the pass or fail marking is derived from the Diagnostic_State only (Req 22).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationResult {
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    state: DiagnosticState,
    classes: Vec<ConflictClass>,
    metadata: BTreeMap<RuleId, RuleMetadata>,
}

impl ValidationResult {
    /// Rebuilds a result from the parts of a stored Validation_Result artifact
    /// (RN-29), so that `compile` can run in another process than `validate`.
    /// Nothing is re-checked here and no validity flag exists: the Compiler
    /// derives its verdict again from the Rulebook, the ledger and the state
    /// (Req 9.2, 9.12). `classes` are sorted and made distinct.
    #[must_use]
    pub fn from_parts(
        rulebook: Rulebook,
        ledger: ApprovalLedger,
        state: DiagnosticState,
        mut classes: Vec<ConflictClass>,
        metadata: BTreeMap<RuleId, RuleMetadata>,
    ) -> Self {
        classes.sort();
        classes.dedup();
        Self {
            rulebook,
            ledger,
            state,
            classes,
            metadata,
        }
    }

    /// The validated Rulebook, unmodified.
    #[must_use]
    pub fn rulebook(&self) -> &Rulebook {
        &self.rulebook
    }

    /// The ledger as supplied.
    #[must_use]
    pub fn ledger(&self) -> &ApprovalLedger {
        &self.ledger
    }

    /// Every Diagnostic, in canonical order.
    #[must_use]
    pub fn state(&self) -> &DiagnosticState {
        &self.state
    }

    /// The conflict classes that were checked, ascending, without repetition
    /// (Req 6.23). All four classes once pass 9 has run.
    #[must_use]
    pub fn checked_conflict_classes(&self) -> &[ConflictClass] {
        &self.classes
    }

    /// Dependency metadata of each validated rule (Req 6.10).
    #[must_use]
    pub fn rule_metadata(&self) -> &BTreeMap<RuleId, RuleMetadata> {
        &self.metadata
    }
}

/// One revision to check.
struct Item<'a> {
    id: &'a RuleId,
    revision: Option<RevisionNumber>,
    content: &'a RuleContent,
    /// Passes 2 to 8 apply (otherwise only pass 1).
    full: bool,
    /// Its Targets and references are in scope for names and the graph.
    current: bool,
    /// Its Diagnostics are returned.
    report: bool,
}

struct Scope {
    /// Rulebook-wide checks (counts, duplicate keys).
    rulebook_checks: bool,
    /// Documents whose Source_Model checks are reported.
    report_docs: BTreeSet<DocumentId>,
    /// Report only the cycles through this rule.
    cycles_through: Option<RuleId>,
}

fn run(
    rb: &Rulebook,
    sources: &SourceModels,
    items: &[Item<'_>],
    scope: &Scope,
) -> (DiagnosticState, BTreeMap<RuleId, RuleMetadata>) {
    let mut out = DiagnosticState::empty();
    if scope.rulebook_checks {
        rulebook_checks(rb, &mut out);
    }
    let decls = Decls::new(rb, sources, &scope.report_docs, &mut out);

    let mut targets = Targets::new();
    for it in items.iter().filter(|i| i.current) {
        if let Some(ty) = kind_type(&it.content.body).filter(|t| *t != rt_ir::Type::Ranking) {
            targets
                .entry(it.content.target.clone())
                .or_default()
                .push(TargetInfo {
                    rule: it.id.clone(),
                    ty,
                });
        }
    }

    let mut sites: BTreeMap<RuleId, Site> = BTreeMap::new();
    let mut deps_by_rule: BTreeMap<RuleId, Deps> = BTreeMap::new();
    for it in items {
        let site = Site::new(it.id, it.revision, &it.content.spans, &decls);
        let mut scratch = DiagnosticState::empty();
        let sink = if it.report { &mut out } else { &mut scratch };
        // Pass 1 (span structure) and pass 2 (spans against the Source_Models).
        let ok = structure::check_span_structure(&it.content.spans, &site, &decls, sink);
        if it.full {
            structure::check_span_sources(&it.content.spans, &ok, &site, &decls, sink);
        }
        // Passes 1, 3, 4, 5 and 6 over the expressions.
        let mut w = Walk::new(&decls, &targets, &site, sink, it.full);
        w.body(it.content);
        let deps = std::mem::take(&mut w.deps);
        if it.full {
            markers::check(&it.content.markers, &site, sink);
        }
        if it.current {
            deps_by_rule.entry(it.id.clone()).or_insert(deps);
            sites.entry(it.id.clone()).or_insert(site);
        }
    }

    let metadata = graph_pass(&targets, &deps_by_rule, &sites, scope, &mut out);
    (out, metadata)
}

fn rulebook_checks(rb: &Rulebook, out: &mut DiagnosticState) {
    let rule_count = rb.rules().len();
    if rule_count > MAX_RULES_PER_RULEBOOK as usize {
        out.push(Diagnostic::new(
            Code::LimitCountExceeded,
            format!("the Rulebook has {rule_count} rules, more than {MAX_RULES_PER_RULEBOOK}"),
            Location::None(Code::LimitCountExceeded),
        ));
    }
    if rb.revisions.len() > MAX_REVISIONS_PER_RULEBOOK as usize {
        out.push(Diagnostic::new(
            Code::LimitCountExceeded,
            format!(
                "the Rulebook has {} revisions, more than {MAX_REVISIONS_PER_RULEBOOK}",
                rb.revisions.len()
            ),
            Location::None(Code::LimitCountExceeded),
        ));
    }
    let mut seen: BTreeSet<&RuleKey> = BTreeSet::new();
    for rev in &rb.revisions {
        if !seen.insert(&rev.key) {
            out.push(
                Diagnostic::new(
                    Code::ValDuplicateRevision,
                    format!(
                        "revision {} of rule '{}' appears more than once",
                        rev.key.revision, rev.key.id
                    ),
                    rev.content
                        .spans
                        .first()
                        .map_or(Location::None(Code::ValDuplicateRevision), |s| {
                            Location::Document(s.document.clone())
                        }),
                )
                .with_rules(vec![RuleRef {
                    rule: rev.key.id.clone(),
                    revision: Some(rev.key.revision),
                }]),
            );
        }
    }
}

/// Pass 8: reference cycles and dependency metadata.
fn graph_pass(
    targets: &Targets,
    deps: &BTreeMap<RuleId, Deps>,
    sites: &BTreeMap<RuleId, Site>,
    scope: &Scope,
    out: &mut DiagnosticState,
) -> BTreeMap<RuleId, RuleMetadata> {
    let ids: Vec<&RuleId> = deps.keys().collect();
    let index: BTreeMap<&RuleId, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let adj: Vec<Vec<usize>> = deps
        .values()
        .map(|d| {
            let set: BTreeSet<usize> = d
                .refs
                .iter()
                .filter_map(|t| targets.get(t))
                .flatten()
                .filter_map(|info| index.get(&info.rule).copied())
                .collect();
            set.into_iter().collect()
        })
        .collect();

    let comps = graph::tarjan(&adj);
    let id_at = |i: usize| ids.get(i).copied();
    for comp in comps.iter().filter(|c| graph::is_cycle(c, &adj)) {
        let path = graph::cycle_path(comp, &adj);
        let rules: Vec<&RuleId> = path.iter().filter_map(|i| id_at(*i)).collect();
        if let Some(only) = &scope.cycles_through {
            if !rules.contains(&only) {
                continue;
            }
        }
        let Some(first) = rules.first().and_then(|r| sites.get(*r)) else {
            continue;
        };
        let names: Vec<String> = rules.iter().map(|r| format!("'{r}'")).collect();
        let related: Vec<Location> = rules
            .iter()
            .skip(1)
            .filter_map(|r| sites.get(*r).and_then(|s| s.primary().cloned()))
            .collect();
        let primary = first
            .primary()
            .cloned()
            .unwrap_or(Location::None(Code::ValReferenceCycle));
        out.push(
            Diagnostic::new(
                Code::ValReferenceCycle,
                format!("reference cycle among rules: {}", names.join(" -> ")),
                primary,
            )
            .with_related(related)
            .with_rules(
                rules
                    .iter()
                    .map(|r| RuleRef {
                        rule: (*r).clone(),
                        revision: None,
                    })
                    .collect(),
            ),
        );
    }

    // Dependencies including those reached through referenced rules. `comps`
    // lists a component after every component it points to, so successors are
    // final when a component is reached.
    let mut fields: Vec<BTreeSet<Name>> = deps.values().map(|d| d.fields.clone()).collect();
    let mut colls: Vec<BTreeSet<Name>> = deps.values().map(|d| d.collections.clone()).collect();
    for comp in &comps {
        let mut f: BTreeSet<Name> = BTreeSet::new();
        let mut c: BTreeSet<Name> = BTreeSet::new();
        for &n in comp {
            f.extend(fields.get(n).into_iter().flatten().cloned());
            c.extend(colls.get(n).into_iter().flatten().cloned());
            for &s in adj.get(n).into_iter().flatten() {
                f.extend(fields.get(s).into_iter().flatten().cloned());
                c.extend(colls.get(s).into_iter().flatten().cloned());
            }
        }
        for &n in comp {
            if let Some(slot) = fields.get_mut(n) {
                slot.clone_from(&f);
            }
            if let Some(slot) = colls.get_mut(n) {
                slot.clone_from(&c);
            }
        }
    }
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let meta = RuleMetadata {
                input_fields: fields.get(i).into_iter().flatten().cloned().collect(),
                collections: colls.get(i).into_iter().flatten().cloned().collect(),
            };
            ((*id).clone(), meta)
        })
        .collect()
}

/// Checks a proposed rule against the declarations of a Rulebook with passes 1
/// to 8, without a ledger and without approval gating (the content-only pass of
/// `ruletrace-ai`).
///
/// The other rules of `declarations` (their highest revisions) are in scope for
/// names and reference cycles, but only findings about the draft are returned.
/// Origin is not an argument.
#[must_use]
pub fn validate_content(
    declarations: &Rulebook,
    draft: &RuleDraft,
    sources: &SourceModels,
) -> DiagnosticState {
    let mut items: Vec<Item<'_>> = Vec::new();
    for rule in declarations.rules() {
        if rule.id == &draft.id {
            continue;
        }
        if let Some(rev) = rule.revisions.last() {
            items.push(Item {
                id: rule.id,
                revision: Some(rev.key.revision),
                content: &rev.content,
                full: true,
                current: true,
                report: false,
            });
        }
    }
    items.push(Item {
        id: &draft.id,
        revision: None,
        content: &draft.content,
        full: true,
        current: true,
        report: true,
    });
    let scope = Scope {
        rulebook_checks: false,
        report_docs: draft
            .content
            .spans
            .iter()
            .map(|s| s.document.clone())
            .collect(),
        cycles_through: Some(draft.id.clone()),
    };
    run(declarations, sources, &items, &scope).0.sorted()
}

/// Validates a Rulebook against a verified ledger and the Source_Models (stage
/// 4).
///
/// Runs passes 1 to 11 and collects every finding. Statuses are derived from
/// `ledger` and are never stored. The result passes when its Diagnostic_State
/// has no Error, Ambiguity or Unsupported Diagnostic
/// (`result.state().no_errors()`). `candidates`, when given, enables the
/// unreferenced Candidate_Clause Warning. Origin is not an argument of any
/// check.
#[must_use]
pub fn validate(
    rulebook: &Rulebook,
    ledger: &ApprovalLedger,
    sources: &SourceModels,
    candidates: Option<&[CandidateClause]>,
) -> ValidationResult {
    let mut result = validate_passes_1_to_9(rulebook, ledger, sources);
    let mut state = std::mem::replace(&mut result.state, DiagnosticState::empty());
    // Pass 10. The Decls is built without reported documents, as for pass 9.
    {
        let mut scratch = DiagnosticState::empty();
        let decls = Decls::new(rulebook, sources, &BTreeSet::new(), &mut scratch);
        approval::gate(rulebook, ledger, &decls, &mut state);
    }
    if let Some(clauses) = candidates {
        approval::candidates(rulebook, clauses, &mut state);
    }
    result.state = state.sorted();
    result
}

/// Passes 1 to 9 over every revision of a Rulebook, with statuses derived from
/// the ledger.
pub(crate) fn validate_passes_1_to_9(
    rulebook: &Rulebook,
    ledger: &ApprovalLedger,
    sources: &SourceModels,
) -> ValidationResult {
    let statuses = derive_statuses(rulebook, ledger);
    let effective = effective_revisions(rulebook, &statuses);
    let effective_map = effective.clone();
    let unresolved_keys = unresolved(rulebook, &statuses);
    let mut validated: BTreeSet<RuleKey> = effective.values().cloned().collect();
    validated.extend(unresolved_keys.iter().cloned());
    // The revision of each rule that is in scope for names and the graph: the
    // Effective one, else the highest unresolved one.
    let mut current: BTreeMap<RuleId, RuleKey> = BTreeMap::new();
    for key in &unresolved_keys {
        current.insert(key.id.clone(), key.clone());
    }
    current.extend(effective);
    let mut seen_current: BTreeSet<RuleId> = BTreeSet::new();
    let items: Vec<Item<'_>> = rulebook
        .revisions
        .iter()
        .map(|rev| {
            let is_current = current.get(&rev.key.id) == Some(&rev.key)
                && seen_current.insert(rev.key.id.clone());
            Item {
                id: &rev.key.id,
                revision: Some(rev.key.revision),
                content: &rev.content,
                full: validated.contains(&rev.key),
                current: is_current,
                report: true,
            }
        })
        .collect();
    let scope = Scope {
        rulebook_checks: true,
        report_docs: rulebook.documents.iter().map(|d| d.id.clone()).collect(),
        cycles_through: None,
    };
    let (mut state, metadata) = run(rulebook, sources, &items, &scope);
    // Pass 9. Its Decls is rebuilt with no reported documents, so the
    // Source_Model findings of pass 2 are not repeated; spans still verify
    // against the same models.
    let classes = {
        let mut scratch = DiagnosticState::empty();
        let decls = Decls::new(rulebook, sources, &BTreeSet::new(), &mut scratch);
        conflicts::check(rulebook, &effective_map, &decls, &mut state)
    };
    ValidationResult {
        rulebook: rulebook.clone(),
        ledger: ledger.clone(),
        state: state.sorted(),
        classes,
        metadata,
    }
}

#[cfg(test)]
mod tests;
