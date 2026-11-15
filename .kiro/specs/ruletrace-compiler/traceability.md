# Traceability Record: ruletrace-compiler requirements revision of 2026-11-14

Analysis artifact, not a spec document. Criterion numbers are stable and new criteria are appended at the end of each requirement. Table edits (16.15 table, 17.1 table, 24.1 table) are listed under their criteria.

Cause keys: UD-n = unresolved decision n as decided or reassessed by the user on 2026-11-14. Approval items 1 to 8 are the user's APPROVAL AND COMPILATION list. The steering note is the user's rule that contradictory diagnostic states are rejected (Requirement 22).

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 1 | Source Ingestion | Unchanged | - | - | - |
| 2 | Clause Extraction Interface | Unchanged | - | - | - |
| 3 | Typed Versioned Rule IR | Changed | 6 | 14 | UD-3 (literal syntax), approval item 1 (no stored lifecycle field) |
| 4 | Rule_IR Parsing and Printing | Changed | 4 | 10 | UD-3 (literal limits), approval item 1 and 22.17 (lifecycle fields) |
| 5 | Approval and Review State | Changed | 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12 | 14, 15 | Approval items 1, 3, 4, 7, 8; UD-22; UD-12 |
| 6 | Semantic Validation | Changed | 13, 14 | - | Approval items 4 and 8 (blocking set); UD-12 |
| 7 | Diagnostics Model | Changed | 15, 17 (catalogue rows RT-DGN-0004, RT-DGN-0005, RT-PRV-0007, RT-AI-0306) | 16, 17, 18, 19, 20 | Consistency work: catalogue of new codes; UD-19, UD-20 |
| 8 | Exact Numeric Semantics | Changed | 4, 5, 7, 11, 13, 15, 16, 19 | 20, 21, 22, 23, 24, 25, 26, 27, 29, 30, 31 | UD-3 reassessment (options D and R; preferred bounded rational, pending design documentation), UD-30 (empty collection) |
| 9 | Compilation | Changed | 10, 11 | 12, 13 | Approval items 1, 6, 7 (atomic compilation) |
| 10 | Deterministic Execution | Unchanged | - | - | - |
| 11 | Missing Information Handling | Unchanged | - | - | - |
| 12 | Conflict and Tie Handling at Execution | Changed | - (criterion 5 retained) | 11, 12 | UD-13 (retained, exit code 4) |
| 13 | Decision Trace and Provenance | Changed | 11, 12 (17 confirmed) | 22, 23 | UD-3 (replay, published comparison), UD-31 |
| 14 | First-Version Support Boundary | Unchanged | - | - | - |
| 15 | Shared Foundation | Unchanged | - | - | - |
| 16 | Minimum End-to-End Vertical Slice | Changed | 1, 8, 10, 15, 16, 17, 18 | 19, 20, 23, 24 | UD-22, UD-20, UD-19, UD-26, approval items 4 and 7, stage ordering, UD-13, UD-29 |
| 17 | Command-Line Interface | Changed | 1 | 11 | UD-22 (review command group), approval item 1 |
| 18 | Optional AI Boundary and Trust Rules | Changed | 1, 10, 13, 15, 17 | - | UD-22, UD-17, UD-18 (consent), approval item 1 (ledger crate) |
| 19 | Safety of Document Content | Changed | 8 | - | UD-22 (Review_Tool retired), ledger as untrusted input |
| 20 | Quality, Testing, and Regression Control | Changed | 4, 15, 16, 22 | 23 | Approval item 1 (ledger fixtures), UD-3 (both options), UD-22 (tree test) |
| 21 | Build, Formatting, Linting, and CI | Changed | 13 | - | UD-22 (ruletrace dependency tree) |
| 22 | Canonical Diagnostic State | Changed | 2, 12, 16, 17 | 18, 19 | Approval item 8 (blocking set), steering note on contradictory states, UD-20 |
| 23 | AI-Assisted Proposal Pipeline | Changed | 1, 5, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 24, 26, 29, 30, 32, 35 | 36, 37, 38 | UD-17, UD-18, UD-19, UD-20, UD-22, approval items 1 and 4 |
| 24 | Pipeline Contracts and Crate Boundaries | Changed | 1, 4, 5, 8 | - | UD-22, approval item 1 (ledger as typed input) |
| 25 | Rule Revisions and Approval Lifecycle | Changed | 6, 7, 8, 11, 13, 14, 16 | 19 | Approval items 1 to 5 and 8; UD-6; UD-12 |
| 26 | Approval Ledger and Approval Gate | New | - | all (1-23), 28, 29, 30, 31 | Approval items 1 to 8; UD-6; UD-12; UD-21 |
| 27 | Executables and Responsibilities | New | 8 (incomplete-run exception) | all (1-10) | UD-22, UD-29 |
| 28 | AI Configuration, Credentials, and Transmission Consent | New | - | all (1-16) | UD-17, UD-18 |
| 29 | Provider Call Policy | New | 6, 17 (incomplete-run exception) | all (1-14) | UD-19 (confirmed), UD-29 |
| 30 | Proposal Batch Acceptance and Protocol Failures | New | 1, 11 (incomplete-run exception) | all (1-8) | UD-20, UD-29 |

Other sections changed: Introduction (pipeline table rows 3 and 4, scope, vertical slice); Glossary (new: Approval_Ledger, Ledger_Record, Approval_Binding, Unresolved_Approval_State, Warning_Diagnostic, Review_Group, ruletrace, ruletrace-ai, Transmission_Consent, Transmission_Log, Request_Identifier, Transient_Failure, Protocol_Failure, Semantic_Outcome, Exact_Number_Limits; changed: Rule, Effective_Revision, Approval_Status, Approval_Record, Blocking_Diagnostic, Validation_Result, Exact_Number, Auxiliary_Crate, Core_Crate, AI_Tool, AI_Configuration, Provider_Failure, Proposal_Store, Review_Decision, CLI, Diagnostic_Catalogue; Review_Tool retired); Unresolved Decisions register (all entries, UD-26 and UD-27 new); Exact_Number reassessment and UD-3 impact table (new); Risks (R-7, R-8, R-11 changed, R-14 to R-16 new); Supersession Record (extended).

## Unchanged requirements

1, 2, 10, 11, 14, 15.

## Requirements that MUST be re-detailed in the refinement pass

Run the per-requirement detailer on: 3, 4, 5, 6, 7, 8, 9, 13, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30.

Priority criteria (new or materially changed): 3.6, 3.14, 4.4, 4.10, 5.1 to 5.15, 6.13, 6.14, 7.15 to 7.20 (including the catalogue table), 8.4, 8.5, 8.7, 8.11, 8.13, 8.16, 8.19 to 8.27, 9.10 to 9.13, 13.11, 13.12, 16.1, 16.8, 16.10, 16.15 to 16.20, 17.1, 17.11, 18.1, 18.10, 18.13, 18.15, 18.17, 20.4, 20.15, 20.16, 20.22, 20.23, 21.13, 22.2, 22.12, 22.16 to 22.19, 23.1, 23.5, 23.9 to 23.19, 23.21, 23.22, 23.24, 23.26, 23.29, 23.30, 23.32, 24.1, 24.4, 24.5, 24.8, 25.6 to 25.8, 25.11, 25.13, 25.14, 25.16, 25.19, and all of Requirements 26 to 30.

Requirements 1, 2, 10, 11, 12, 14, 15 need no re-detailing. Re-read Requirements 12 and 14 once UD-3 is confirmed.

## Detailing pass (2026-11-14)

The per-requirement refinement pass was re-run on Requirements 3, 4, 5, 6, 7, 8, 9, 13, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29 and 30. Existing criterion numbers were kept; new criteria were appended.

| Requirement | Appended criteria |
| --- | --- |
| 3 | 15, 16, 17 |
| 4 | 11 |
| 6 | 15 (split from 1), 26 |
| 7 | 21, 22 |
| 8 | 28 |
| 9 | 14 |
| 13 | 20, 21 |
| 16 | 21, 22 |
| 17 | 12, 13 |
| 19 | 9, 10, 11 |
| 20 | 24, 25 |
| 22 | 20, 21 |
| 23 | 33, 34, 35 |
| 24 | 9, 10, 11 |
| 25 | 20, 21, 22 |
| 26 | 24, 25, 26, 27 |
| 29 | none |
| 30 | 9, 10, 11 |

Unchanged by the pass: Requirements 1, 2, 10, 11, 12, 14, 15. (Requirement 12 changed in the confirmation round below.)

Points the detailing pass raised that need a decision (not decided by the pass):

- Ordering of Rule identifiers ("ascending Rule identifier order") is bytewise UTF-8 in Req 3.11 and Req 7.22. Req 6 and Req 9 rely on the same ordering but do not restate it.
- Req 8.15: RESOLVED (UD-30). Count and sum of an empty collection are 0. Minimum and maximum are undefined and give an Indeterminate Outcome with RT-DGN-0005 when required (8.29 to 8.31).
- Req 25.1: RESOLVED (UD-28). The Revision_Number range 1 to 4,294,967,295 is confirmed.
- Req 28.2: RESOLVED (UD-28). Default 20 and range 1 to 100 for maximum Candidate_Clauses per request are confirmed.
- Req 16.16, 30.3: RESOLVED (UD-26). Exit 0 with RT-AI-0305 for a stored batch with blocking semantic findings; see tools/decisions/UD-26.md.
- Req 13.17: RESOLVED (UD-31). Confirmed and extended by 13.22 and 13.23.
- Req 23.35: RESOLVED (UD-29). Earlier batches are kept, the run is marked incomplete (23.36 to 23.38, RT-AI-0306), and the exit code is that of the failing request.
- Req 30: no catalogue code exists for a wrong line or column claim (mapped to RT-PRV-0101). When several elements fail in one batch, only the earliest-stage code decides the exit code. Confirm whether all failures are reported.
- Req 22.7 and 22.9: whether a supplied flag that agrees with the derived value is accepted and discarded or rejected is not stated (UD-11).
- Req 19.4 defines nesting depth for JSON-like containers, which assumes the UD-2 format choice.
- Req 7.17 lists codes by RT-family identifiers, while criteria elsewhere cite mnemonics. Req 7.16 binds each mnemonic to one identifier.

## Confirmation round (2026-11-14)

Decisions applied: UD-21 (adopt on approve; Pending_Proposals do not block), UD-13 (retain 12.5; exit code 4), UD-3 (preferred bounded rational, pending design documentation), UD-19 and UD-28 (defaults and bounds confirmed), UD-29 (incomplete multi-request run), UD-30 (empty-collection aggregation), UD-31 (trace with no resolvable source references).

| Requirement | Changed criteria | New criteria | New Diagnostic codes |
| --- | --- | --- | --- |
| Glossary | Exact_Number, Exact_Number_Limits | Pending_Proposal, Run_Summary | - |
| 7 | 17 (catalogue rows) | - | RT-DGN-0004, RT-DGN-0005, RT-PRV-0007, RT-AI-0306 |
| 8 | 15, 19 | 29, 30, 31 | RT-DGN-0005 |
| 12 | - (5 retained) | 11, 12 | RT-DGN-0004 |
| 13 | - (17 confirmed) | 22, 23 | RT-PRV-0007 |
| 16 | - | 23, 24 | - |
| 18 | 17 | - | - |
| 23 | 35 | 36, 37, 38 | RT-AI-0306 |
| 26 | - (19 kept) | 28, 29, 30, 31 | - |
| 27 | 8 | - | - |
| 29 | 17 | - | - |
| 30 | 1, 11 | - | - |

Register: UD-3, UD-13, UD-19, UD-21, UD-25 updated; UD-28 to UD-31 added.

## Scope amendment: thin Application_Layers and end-to-end demonstration (2026-11-14)

Cause: the user's decision to add Prize_Check and Bracket_Check as two thin demonstration layers over the shared Rules_Engine, and a reproducible demonstration over real public documents. This supersedes the earlier statements that Prize Check and Bracket Check are out of scope for the first version. Criterion numbers are stable and new criteria are appended. Requirements 31 to 34 are new. Design and tasks are not yet updated (design section 8 and the phase and task lists describe the earlier scope and must be revised in the design phase).

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | - | 23 | New codes RANK_REFERENCE_CYCLE and APP_* |
| 14 | First-Version Support Boundary | Changed | 5, 6, 8 | 11, 12, 13, 14 | Definition of qualification (Qualification_Predicate); rank reference gap (UD-32) |
| 15 | Shared Foundation for Prize Check and Bracket Check | Changed | 8 | 10 | Two Application_Layer crates now exist; 15.5, 15.6, 15.7 become testable |
| 20 | Quality, Testing, and Regression Control | Changed | - | 26, 27 | Demonstration acceptance test as release criterion; required safety tests kept |
| 22 | Canonical Diagnostic State | Changed | - | 22 | New input kinds (application input, published files, recorded script, manifest) |
| 24 | Pipeline Contracts and Crate Boundaries | Changed | - | 12 | Allowed edges of Application_Layers |
| 27 | Executables and Responsibilities | Changed | 1 (and user story) | 15 | Two Demonstration_Executables (UD-33) |
| 28 | AI Configuration, Credentials, and Transmission Consent | Changed | - | 21 | Consent applies to demonstration documents |
| 31 | Thin Demonstration Application Layers over the Shared Engine | New | - | all (1-15) | Scope amendment (a, b) |
| 32 | Bracket_Check Standings and Qualification | New | - | all (1-7) | Scope amendment (a, b); UD-32 |
| 33 | Reproducible End-to-End Demonstration over Real Public Documents | New | - | all (1-17) | Scope amendment (c); UD-34, UD-35, UD-36 |
| 34 | Honest Claims and Stated Limits of Demonstration Material | New | - | all (1-8) | Scope amendment (d); R-3, R-12 |

Other sections changed: Introduction (demonstration applications paragraph), Scope (in and out of scope), Glossary (Prize_Check and Bracket_Check changed; new Application_Layer, Application_Input_Format, Demonstration_Executable, Qualification_Predicate, Standings, Demo_Corpus, Coverage_Statement, Recorded_Provider_Script, Demo_Runbook, Proof_Report), Unresolved Decisions register (UD-22 amended; UD-32 to UD-36 new), Risks (R-2 changed; R-17 to R-20 new), Supersession Record (new entries for this amendment).

Unchanged requirements: 1 to 6, 8 to 13, 16 to 19, 21, 23, 25, 26, 29, 30 (except where listed above). No safety criterion was removed or weakened.

Mapping of the user's items to the requirements: (a) Introduction, Scope, Glossary, 14.5, 14.11 to 14.14, 15.8, 15.10; (b) 24.12, 27.1, 27.15, 31, 32; (c) 28.21, 33; (d) 34, R-17; (e) 20.26, 20.27.

New decisions: UD-32 (Confirmed, Rank_Reference), UD-33 (Recommended-pending, applied in 27.1), UD-34 (Open), UD-35 (Open, recommendation given), UD-36 (Confirmed, option 1; see tools/decisions/UD-36.md).

## Gate 1.10 amendment (UD-34 and exit-code ordering)

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 13 | Decision Trace and Provenance | Changed | 18 (exit 2 only when no earlier stage failed; exit 4 outranks a stage 7 provenance Error) | 24 (test of simultaneous Indeterminate execution and provenance failure) | Conflict with 16.17 and Property 30 |
| 33 | Reproducible End-to-End Demonstration over Real Public Documents | Unchanged | - | - | UD-34 confirmed (see `tools/decisions/UD-34.md`) |

Other sections changed: Unresolved Decisions register (UD-34 entry now Confirmed; its "no document is named" sentence refers to the record); Supersession Record (two entries); design §3.5, §6.1, §8.4 (exit derivation) and the status table row for UD-34.

UD-32 is not changed by this amendment. See the gate 1.11 amendment below.

## Gate 1.11 amendment (UD-32)

UD-32 is Confirmed with option Rank_Reference, recorded in `tools/decisions/UD-32.md`. It was applied under the gate rule from the UD-34 scope (top-two qualification) because the user did not name UD-32, and is reported back for confirmation. The Req 14.12 extension applies; Req 14.13 and 32.4 do not. Criteria 14.11 to 14.14 are unchanged except for a status note on 14.12 and 14.13.

## Req 23.5 amendment (closed schema and excluded content kinds)

User decision: a provider field that would carry a script, a general-purpose expression, a shell command or an external reference (for example `script`, `command`, `reference`) is an unknown field under the closed Proposal_Schema and is AI_RESPONSE_SCHEMA_VIOLATION (RT-AI-0202). AI_RESPONSE_FORBIDDEN_FIELD (RT-AI-0204) stays a security diagnostic for the thirteen approval and validity names of design section 3.7.4 and is not used for ordinary schema violations.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Changed | 5 (the FORBIDDEN_FIELD class is the approval and validity names only; excluded content kinds are SCHEMA_VIOLATION) | - | User decision on the stage A review |

Other sections changed: design section 3.7.3 (closed objects at every level, rule kind and node outside the request vocabulary is RT-AI-0202, unknown but well-formed `op` or `fn` is carried and reaches the validator as Unsupported, no provider field or value is executed) and section 3.7.4 step 1 (the thirteen names are the whole of RT-AI-0204). Criterion 4 is unchanged. No safety criterion was removed or weakened.

## Req 23.7 amendment (claimed line and column check)

User decision: criterion 7 lists the check of the claimed line and column between the bounds and boundary check and the clause-containment check, as design section 3.7.4 step 4 and the Diagnostic_Catalogue (RT-PRV-0101) already state. A line or column that differs from the Source_Model value is AI_PROPOSAL_SPAN_UNRESOLVED.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Changed | 7 (the claimed line and column check is listed, in the order of design section 3.7.4) | - | User decision on the stage B review |

Other sections changed: design section 3.7.4 step 3 (locations: RT-PRV-0103 at the evidence item, `start >= end` at `end`). No safety criterion was removed or weakened.
