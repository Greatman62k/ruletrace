//! Declarations in scope, document states and rule locations shared by all
//! passes.

use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId, RuleRef,
    SourceSpan,
};
use rt_ir::{EnumDecl, Name, Rulebook, SetDecl, Span, Type};
use rt_srcmodel::{SourceModel, SourceModels};
use std::collections::{BTreeMap, BTreeSet};

/// What is known about a declared document.
pub(crate) enum DocState<'a> {
    /// The Source_Model is present and its hash equals the declared hash.
    Verified(&'a SourceModel),
    /// No model, or a hash mismatch (already reported). Spans cannot be checked
    /// or resolved.
    Unavailable,
}

/// The declared names of a Rulebook and the documents it references.
pub(crate) struct Decls<'a> {
    pub inputs: BTreeMap<&'a Name, &'a Type>,
    pub enums: BTreeMap<&'a Name, &'a EnumDecl>,
    pub sets: BTreeMap<&'a Name, &'a SetDecl>,
    pub collections: BTreeMap<&'a Name, BTreeMap<&'a Name, &'a Type>>,
    pub docs: BTreeMap<&'a DocumentId, DocState<'a>>,
}

impl<'a> Decls<'a> {
    /// Collects the declarations and resolves every declared document against
    /// `sources` (pass 2). A missing model gives `RT-VAL-0017` and a hash
    /// mismatch `RT-VAL-0018`, once per document in `report_docs`, with the
    /// Document_Location.
    pub(crate) fn new(
        rb: &'a Rulebook,
        sources: &'a SourceModels,
        report_docs: &BTreeSet<DocumentId>,
        out: &mut DiagnosticState,
    ) -> Self {
        let mut d = Decls {
            inputs: BTreeMap::new(),
            enums: BTreeMap::new(),
            sets: BTreeMap::new(),
            collections: BTreeMap::new(),
            docs: BTreeMap::new(),
        };
        for i in &rb.types.inputs {
            d.inputs.entry(&i.name).or_insert(&i.ty);
        }
        for e in &rb.types.enums {
            d.enums.entry(&e.name).or_insert(e);
        }
        for s in &rb.types.sets {
            d.sets.entry(&s.name).or_insert(s);
        }
        for c in &rb.types.collections {
            let fields = d.collections.entry(&c.name).or_default();
            for f in &c.fields {
                fields.entry(&f.name).or_insert(&f.ty);
            }
        }
        for doc in &rb.documents {
            if d.docs.contains_key(&doc.id) {
                continue;
            }
            let report = report_docs.contains(&doc.id);
            let state = match sources.get(&doc.id) {
                None => {
                    if report {
                        out.push(Diagnostic::new(
                            Code::ValSourceModelMissing,
                            format!("no Source_Model was supplied for document '{}'", doc.id),
                            Location::Document(doc.id.clone()),
                        ));
                    }
                    DocState::Unavailable
                }
                Some(model) if model.sha256() != &doc.sha256 => {
                    if report {
                        out.push(Diagnostic::new(
                            Code::ValDocumentHashMismatch,
                            format!(
                                "document '{}' is declared with SHA-256 {} but its Source_Model has {}",
                                doc.id,
                                doc.sha256,
                                model.sha256()
                            ),
                            Location::Document(doc.id.clone()),
                        ));
                    }
                    DocState::Unavailable
                }
                Some(model) => DocState::Verified(model),
            };
            d.docs.insert(&doc.id, state);
        }
        d
    }

    /// The verified Source_Span of `span`, or `None` when the document is
    /// unavailable or the span does not verify. No span is built without that
    /// check (Req 7.14, 7.15).
    pub(crate) fn verified(&self, span: &Span) -> Option<SourceSpan> {
        match self.docs.get(&span.document) {
            Some(DocState::Verified(model)) => {
                SourceSpan::new(*model, span.start, span.end, span.line, span.column).ok()
            }
            _ => None,
        }
    }
}

/// Where the diagnostics of one rule point: its first span (primary) and the
/// others (related), as verified Source_Spans where possible and as
/// Document_Locations otherwise (Req 6.24).
pub(crate) struct Site {
    pub rule: RuleRef,
    primary: Option<Location>,
    related: Vec<Location>,
}

impl Site {
    pub(crate) fn new(
        id: &RuleId,
        revision: Option<RevisionNumber>,
        spans: &[Span],
        decls: &Decls<'_>,
    ) -> Self {
        let mut locs = spans.iter().map(|s| {
            decls
                .verified(s)
                .map_or_else(|| Location::Document(s.document.clone()), Location::Source)
        });
        let primary = locs.next();
        Site {
            rule: RuleRef {
                rule: id.clone(),
                revision,
            },
            primary,
            related: locs.collect(),
        }
    }

    /// A Diagnostic about the rule. A rule without any span has no location
    /// (`Location::None`).
    pub(crate) fn diag(&self, code: Code, message: String) -> Diagnostic {
        let primary = self.primary.clone().unwrap_or(Location::None(code));
        Diagnostic::new(code, message, primary)
            .with_related(self.related.clone())
            .with_rules(vec![self.rule.clone()])
    }

    /// The first span location, if any (used by cycle diagnostics).
    pub(crate) fn primary(&self) -> Option<&Location> {
        self.primary.as_ref()
    }
}
