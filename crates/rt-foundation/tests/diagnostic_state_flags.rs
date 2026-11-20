//! Property 4: Diagnostic_State flags are derived and consistent.
//!
//! Feature: ruletrace-compiler, Property 4: Diagnostic_State flags are derived
//! and consistent
//!
//! **Validates: Requirements 22.1, 22.2, 22.3, 22.4, 22.5, 22.14, 22.19, 22.20,
//! 25.20, 6.26**
//!
//! The oracle counts severities directly with an exhaustive `match`,
//! independent of `Severity::is_blocking`. "Compilability" is `stage_result`,
//! which accepts a value only when the state has no blocking item. No exit-code
//! mapping from a state exists in this crate yet, so the exit-code clause is
//! covered through `ExitCode::Success` versus a rejection only where the crate
//! defines it (compilability).

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_foundation::{
    stage_result, CliLocation, Code, Diagnostic, DiagnosticState, DocumentId, InputId,
    InputLocation, Location, Severity, SourceSpan, SpanSource, ValidationOutcome,
};

struct Doc(DocumentId);

impl SpanSource for Doc {
    fn document_id(&self) -> &DocumentId {
        &self.0
    }
    fn len_bytes(&self) -> u64 {
        100
    }
    fn is_char_boundary(&self, _offset: u32) -> bool {
        true
    }
    fn line_column(&self, offset: u32) -> Option<(u32, u32)> {
        (offset <= 100).then(|| (offset / 10 + 1, offset % 10 + 1))
    }
}

fn blocking_by_match(s: Severity) -> bool {
    match s {
        Severity::Error | Severity::Ambiguity | Severity::Unsupported => true,
        Severity::Warning => false,
    }
}

fn codes_where(f: impl Fn(Severity) -> bool) -> Vec<Code> {
    Code::ALL
        .iter()
        .copied()
        .filter(|c| f(c.severity()))
        .collect()
}

fn arb_location() -> impl Strategy<Value = Location> {
    prop_oneof![
        (0u32..90, 1u32..10).prop_filter_map("span", |(start, len)| {
            let doc = Doc(DocumentId::new("a").ok()?);
            let (line, column) = doc.line_column(start)?;
            SourceSpan::new(&doc, start, start + len, line, column)
                .ok()
                .map(Location::Source)
        }),
        Just(()).prop_filter_map("doc", |()| DocumentId::new("a")
            .ok()
            .map(Location::Document)),
        Just(()).prop_filter_map("input", |()| {
            Some(Location::Input(InputLocation {
                input: InputId::new("i").ok()?,
                line: Some(1),
                column: None,
                field: Some("f".to_string()),
            }))
        }),
        Just(Location::Cli(CliLocation::Command("c".to_string()))),
        Just(Location::Cli(CliLocation::Argument {
            position: 0,
            name: None
        })),
        prop::sample::select(Code::ALL.to_vec()).prop_map(Location::None),
    ]
}

fn arb_diag_from(codes: Vec<Code>) -> impl Strategy<Value = Diagnostic> {
    (prop::sample::select(codes), arb_location())
        .prop_map(|(code, loc)| Diagnostic::new(code, String::new(), loc))
}

/// Lists of diagnostics of every severity and location variant, including empty
/// lists and lists that hold only Warnings.
fn gen_diagnostics() -> impl Strategy<Value = Vec<Diagnostic>> {
    let any = Code::ALL.to_vec();
    let warnings = codes_where(|s| !blocking_by_match(s));
    prop_oneof![
        Just(Vec::new()),
        prop::collection::vec(arb_diag_from(warnings), 0..8),
        prop::collection::vec(arb_diag_from(any), 0..12),
    ]
}

fn state_of(list: &[Diagnostic]) -> DiagnosticState {
    let mut s = DiagnosticState::empty();
    for d in list {
        s.push(d.clone());
    }
    s
}

fn compilable(s: &DiagnosticState) -> bool {
    stage_result((), s.clone()).is_ok()
}

#[test]
fn catalogue_covers_every_severity() {
    for want in [
        Severity::Error,
        Severity::Warning,
        Severity::Ambiguity,
        Severity::Unsupported,
    ] {
        assert!(
            Code::ALL.iter().any(|c| c.severity() == want),
            "no code with severity {want:?}"
        );
    }
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// has_errors is exactly "some Error, Ambiguity or Unsupported item";
    /// no_errors is its negation; the outcome follows; the flags do not depend
    /// on order or on the sort.
    #[test]
    fn flags_are_derived_from_the_list(list in gen_diagnostics()) {
        let s = state_of(&list);
        let expected = list.iter().any(|d| blocking_by_match(d.severity()));
        prop_assert_eq!(s.has_errors(), expected);
        prop_assert_eq!(s.no_errors(), !expected);
        prop_assert_eq!(s.no_errors(), !s.has_errors());
        let outcome = if expected { ValidationOutcome::Failed } else { ValidationOutcome::Passed };
        prop_assert_eq!(s.outcome(), outcome);
        prop_assert_eq!(compilable(&s), !expected);
        prop_assert_eq!(s.as_slice().len(), list.len());

        // Computed on every call: repeated reads and a clone agree.
        prop_assert_eq!(s.has_errors(), s.clone().has_errors());
        // Reordering never changes a flag.
        prop_assert_eq!(s.clone().sorted().has_errors(), expected);
        let reversed: Vec<Diagnostic> = list.iter().rev().cloned().collect();
        prop_assert_eq!(state_of(&reversed).outcome(), outcome);
        // An empty or Warning-only list never has errors.
        if list.iter().all(|d| d.severity() == Severity::Warning) {
            prop_assert!(s.no_errors() && s.outcome() == ValidationOutcome::Passed);
        }
    }

    /// Adding or removing Warning items changes no flag and not compilability;
    /// adding one blocking item to any list makes it fail.
    #[test]
    fn warnings_never_change_flags(
        list in gen_diagnostics(),
        extra in prop::collection::vec(arb_diag_from(codes_where(|s| !blocking_by_match(s))), 0..6),
        blocker in arb_diag_from(codes_where(blocking_by_match)),
    ) {
        let base = state_of(&list);

        let mut with_warnings = base.clone();
        for w in &extra {
            with_warnings.push(w.clone());
        }
        prop_assert_eq!(with_warnings.has_errors(), base.has_errors());
        prop_assert_eq!(with_warnings.no_errors(), base.no_errors());
        prop_assert_eq!(with_warnings.outcome(), base.outcome());
        prop_assert_eq!(compilable(&with_warnings), compilable(&base));

        // Removing every Warning from the list leaves the same flags.
        let blocking_only: Vec<Diagnostic> =
            list.iter().filter(|d| d.severity() != Severity::Warning).cloned().collect();
        let stripped = state_of(&blocking_only);
        prop_assert_eq!(stripped.has_errors(), base.has_errors());
        prop_assert_eq!(stripped.outcome(), base.outcome());
        prop_assert_eq!(compilable(&stripped), compilable(&base));

        // One blocking item always flips the state to failed, and extend keeps
        // that.
        let mut blocked = base.clone();
        blocked.push(blocker);
        prop_assert!(blocked.has_errors() && !blocked.no_errors());
        prop_assert_eq!(blocked.outcome(), ValidationOutcome::Failed);
        prop_assert!(!compilable(&blocked));
        let mut merged = DiagnosticState::empty();
        merged.extend(blocked);
        prop_assert!(merged.has_errors());
    }
}
