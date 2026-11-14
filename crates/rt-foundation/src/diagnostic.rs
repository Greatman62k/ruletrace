//! The Diagnostic value (Req 7.1, 7.10, 7.18, 7.22).

use crate::{Code, Location, RuleRef, Severity};
use std::cmp::Ordering;

/// An error, warning, ambiguity or unsupported-construct report.
///
/// The fields are private and the only constructor is [`Diagnostic::new`], which takes no
/// severity: the severity is always the one the catalogue fixes for the code (Req 7.18).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    code: Code,
    message: String,
    primary: Location,
    related: Vec<Location>,
    rules: Vec<RuleRef>,
}

impl Diagnostic {
    /// Builds a diagnostic with no related locations and no rules.
    #[must_use]
    pub fn new(code: Code, message: String, primary: Location) -> Self {
        Self {
            code,
            message,
            primary,
            related: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// Replaces the related locations, which keep the given order.
    #[must_use]
    pub fn with_related(mut self, related: Vec<Location>) -> Self {
        self.related = related;
        self
    }

    /// Replaces the rules the diagnostic concerns, which keep the given order.
    #[must_use]
    pub fn with_rules(mut self, rules: Vec<RuleRef>) -> Self {
        self.rules = rules;
        self
    }

    /// The catalogue code.
    #[must_use]
    pub fn code(&self) -> Code {
        self.code
    }

    /// The severity, derived from the code.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }

    /// The primary location.
    #[must_use]
    pub fn primary(&self) -> &Location {
        &self.primary
    }

    /// The related locations, in stored order.
    #[must_use]
    pub fn related(&self) -> &[Location] {
        &self.related
    }

    /// The rules concerned, in stored order. The first one is the rule identifier of Req 7.22.
    #[must_use]
    pub fn rules(&self) -> &[RuleRef] {
        &self.rules
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The total order of Req 7.10 and 7.22: primary location (variant rank first), code identifier
/// bytes, message bytes, related locations (a proper prefix sorts first), then the first rule
/// identifier bytes (none sorts first). Two diagnostics equal under all of those are ordered by
/// the full rule list, so that the order is total and consistent with `Eq`, and sorting any
/// permutation of a list gives one result. Equal items are kept, never merged.
impl Ord for Diagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        self.primary
            .cmp(&other.primary)
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.message.as_bytes().cmp(other.message.as_bytes()))
            .then_with(|| self.related.cmp(&other.related))
            .then_with(|| {
                let first = |d: &Self| d.rules.first().map(|r| r.rule.clone());
                first(self).cmp(&first(other))
            })
            .then_with(|| self.rules.cmp(&other.rules))
    }
}

impl PartialOrd for Diagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CliLocation, DocumentId, InputId, InputLocation, RevisionNumber, RuleId};

    fn rule(id: &str, rev: Option<u32>) -> RuleRef {
        RuleRef {
            rule: RuleId::new(id).unwrap(),
            revision: rev.map(|r| RevisionNumber::new(r).unwrap()),
        }
    }

    fn input(name: &str, line: Option<u32>) -> Location {
        Location::Input(InputLocation {
            input: InputId::new(name).unwrap(),
            line,
            column: None,
            field: None,
        })
    }

    fn d(code: Code, msg: &str, loc: Location) -> Diagnostic {
        Diagnostic::new(code, msg.to_string(), loc)
    }

    #[test]
    fn severity_comes_from_the_code() {
        let loc = Location::None(Code::InternalError);
        let w = d(Code::ExtractNoClauses, "m", loc.clone());
        let e = d(Code::IngestNotUtf8, "m", loc.clone());
        let a = d(Code::ValAmbiguousInterpretation, "m", loc.clone());
        let u = d(Code::IrUnknownField, "m", loc);
        assert_eq!(w.severity(), Severity::Warning);
        assert_eq!(e.severity(), Severity::Error);
        assert_eq!(a.severity(), Severity::Ambiguity);
        assert_eq!(u.severity(), Severity::Unsupported);
    }

    #[test]
    fn accessors_return_what_was_stored() {
        let x = d(Code::CliMissingArgument, "msg", input("a", Some(1)))
            .with_related(vec![input("b", None)])
            .with_rules(vec![rule("r1", Some(2))]);
        assert_eq!(x.code(), Code::CliMissingArgument);
        assert_eq!(x.message(), "msg");
        assert_eq!(x.primary(), &input("a", Some(1)));
        assert_eq!(x.related(), &[input("b", None)]);
        assert_eq!(x.rules(), &[rule("r1", Some(2))]);
    }

    #[test]
    fn location_rank_orders_before_code() {
        let doc = Location::Document(DocumentId::new("z").unwrap());
        let cli = Location::Cli(CliLocation::Command("x".into()));
        let none = Location::None(Code::InternalError);
        let mut v = [
            d(Code::AiDisabled, "", none.clone()),
            d(Code::AiDisabled, "", cli.clone()),
            d(Code::AiDisabled, "", input("a", None)),
            d(Code::AiDisabled, "", doc.clone()),
        ];
        v.sort();
        let prim: Vec<_> = v.iter().map(|x| x.primary().clone()).collect();
        assert_eq!(prim, vec![doc, input("a", None), cli, none]);
    }

    #[test]
    fn absent_line_sorts_first_and_code_orders_by_identifier_bytes() {
        let mut v = [
            d(Code::AiDisabled, "", input("a", Some(1))),
            d(Code::AiDisabled, "", input("a", None)),
        ];
        v.sort();
        assert_eq!(v[0].primary(), &input("a", None));

        // "RT-AI-0001" < "RT-APP-0001" bytewise ('I' < 'P'), whatever the enum order is.
        let loc = input("a", None);
        let mut v = [
            d(Code::AppInputMalformed, "", loc.clone()),
            d(Code::AiDisabled, "", loc),
        ];
        v.sort();
        assert_eq!(v[0].code(), Code::AiDisabled);
    }

    #[test]
    fn message_then_related_prefix_then_rule_id() {
        let loc = input("a", None);
        let base = || d(Code::AiDisabled, "m", loc.clone());
        let r1 = vec![input("r", None)];
        let r2 = vec![input("r", None), input("s", None)];
        let mut v = [
            base().with_related(r2.clone()),
            base().with_related(r1.clone()),
            base().with_related(vec![]),
            d(Code::AiDisabled, "a", loc.clone()),
        ];
        v.sort();
        assert_eq!(v[0].message(), "a");
        assert_eq!(v[1].related(), &[] as &[Location]);
        assert_eq!(v[2].related(), r1.as_slice());
        assert_eq!(v[3].related(), r2.as_slice());

        let mut v = [
            base().with_rules(vec![rule("b", None)]),
            base().with_rules(vec![rule("a", None)]),
            base(),
        ];
        v.sort();
        assert!(v[0].rules().is_empty());
        assert_eq!(v[1].rules()[0].rule.as_str(), "a");
        assert_eq!(v[2].rules()[0].rule.as_str(), "b");
    }

    #[test]
    fn equal_items_are_kept_and_revision_breaks_ties() {
        let loc = input("a", None);
        let base = || d(Code::AiDisabled, "m", loc.clone());
        let x = base().with_rules(vec![rule("a", Some(2))]);
        let y = base().with_rules(vec![rule("a", Some(1))]);
        let mut v = [x.clone(), y.clone(), x.clone()];
        v.sort();
        assert_eq!(v, [y, x.clone(), x]);
    }
}
