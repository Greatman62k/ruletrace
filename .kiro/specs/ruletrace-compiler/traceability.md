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
- Req 23.35: RESOLVED (UD-29). Earlier batches are kept, the run is marked incomplete (23.36 to 23.38, RT-AI-0306), and the exit code is that of the failing request. The one exception to the state before the failing request is a failed directory sync after the link (RT-AI-0303): its whole batch file stays and is listed as stored with durability not confirmed (23.35, 23.36).
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

## Stage C review amendment (8.6: credential scrub, canonical drafts, stored finding locations)

User decisions on the 8.6 stage C review, all accepted:

1. A stored rule draft is the canonical compact print of the decoded Rule_Content (`rt_ir::print_draft`), so equal content gives equal stored bytes (UD-36, canonical proposals). The provider's key order and spacing are not kept. The retained response payload still holds the provider's exact bytes.
2. The draft keeps the document hash of the verified Source_Model. Adoption recomputes the Draft hash against the Rulebook and refuses a mismatch with `RT-APR-0213`. A test in `rt-review` covers it.
3. A validator finding on a proposed rule is stored with location `Field("rule.<field>")`, or `Field("rule")` when no field is known.
4. Exception to "the earliest failing stage decides the code": a provenance rejection (exit 3) that echoes the credential, whole or as a run of 8 or more characters, is replaced by `RT-AI-0206` with exit 6. The replacement must not carry the echoed text, and a different code would reveal which stage held it. Logged as an intentional security exception. Design section 3.7.4 step 5 states it.

Defect fixed with the amendment: a rejected batch could return Diagnostics that quoted the credential from an untrusted key, kind or document id. `accept_batch` now replaces such a rejection by one `RT-AI-0206` (design section 3.7.4 step 5). No requirement criterion was changed, and no safety criterion was removed or weakened.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | User decisions on the stage C review refine design section 3.7.4 only |

Other sections changed: design section 3.7.4 (step 4 stored findings and draft, new step 5, with the 8-character guarantee and the structural field path).

## Stage C follow-up amendment (8.6: partial credential echoes in accepted batches)

User decision: an accepted batch is refused when it holds a partial echo of the credential. Requirement 28 criterion 9 now says that a Provider_Response payload that contains the credential value, or any run of at least 8 consecutive characters of it, is not stored. Design section 3.7.4 step 5 states the guarantee, the field names that production emits and the limits.

- The check covers the raw payload and every decoded text that the store keeps, in the plain, JSON-escaped and control-escaped forms. It refuses with `RT-AI-0206` (exit 6) at the structural field that holds the match.
- Stated limits, each pinned by a test: the match is case-sensitive, and a credential split into pieces shorter than 8 characters is not found.
- Trade-off: a low-entropy credential such as `password1` can cause a legitimate batch to be refused, because a window of 8 of its characters (`password`) occurs in ordinary rationale or quote text. The refusal is loud (`RT-AI-0206`, exit 6, nothing stored). The alternative of matching only the whole value was rejected because a long key shown in part by a rationale would then be stored.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 28 | AI Configuration, Credentials, and Transmission Consent | Changed | 9 (a payload with a run of 8 characters of the credential is not stored) | - | User decision on the stage C follow-up review |

Other sections changed: design section 3.7.4 step 5. No safety criterion was removed or weakened.

## Proposal_Store fail-closed amendment (8.7: no rename fallback)

User decision of 2026-11-15: the Proposal_Store writer fails closed, as the Approval_Ledger readers do (UD-15). There is no `create_new` `.lock` marker and no `rename` fallback. If `hard_link` is unsupported, or fails for any reason other than "already exists", the write is refused with `RT-AI-0303` (exit 70). The store is left unchanged and the temp file is removed. If the link target already exists, the outcome is `RT-AI-0301`.

- Design section 3.7.5 step 3 and risk R-D5 state the rule. The 8.7 task text no longer names the fallback.
- No requirement criterion was changed. Req 29.19 (no partial batch file) is met by the refusal. No safety criterion was removed or weakened.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 29 | Provider Call Policy and Proposal_Store | Unchanged | - | - | User decision on the 8.7 store design |

Other sections changed: design section 3.7.5, risk R-D5, task 8.7 text.

## Proposal_Store writer details (8.7)

Choices made while implementing the writer. Each is stated in design section 3.7.5 or 3.7.7 and pinned by a test. None changes a requirement criterion.

- A temp file left by an interrupted run is never reused or removed, because a live run may own it. The writer takes the first free name from `<id>.tmp` to `<id>.7.tmp` with `create_new`. A rerun after a crash therefore stores the batch.
- A failed directory sync after the link keeps the whole batch file and reports `RT-AI-0303`. Removing it could contradict a concurrent run that already reported `RT-AI-0301`. Req 29.19 forbids a partial batch file, and the file is whole.
- `RT-AI-0306` follows a failure only when this run stored an earlier batch. A request that found its batch already stored wrote nothing and does not count.
- Writers that copy text from inputs (Transmission_Log, Run_Summary, run Diagnostics) use `Redactor::redact_fragments`, which also removes any run of 8 or more characters that shares an 8-character window with the credential (Req 28.9).

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 29 | Provider Call Policy and Proposal_Store | Unchanged | - | - | Implementation choices of the store writer |

## Stale temporary files of the store writer (8.7 follow-up)

User decision of 2026-11-15. The writer keeps its eight temporary names. A temporary file that cannot be removed is no longer dropped or folded into the Error message.

- New catalogue code `RT-AI-0307` (`AI_STORE_TEMP_NOT_REMOVED`, Warning, exit 0, `ruletrace-ai`). It covers the one failure mode "a temporary file of the writer could not be removed", whether the write succeeded, lost a race or was refused. No existing code fitted: `RT-AI-0301` means a batch exists and `RT-AI-0303` is an Error. The code is in the Req 7.17 table, the design catalogue table (row marked N) and `catalogue/diagnostics.v1.json`, and `rt-xtask gen-catalogue` regenerated `code_generated.rs`.
- When all eight names are taken, the `RT-AI-0303` message lists the eight paths so the operator can delete the stale ones. Control characters in the paths are escaped. No requirement criterion changes.
- A removal error after the link lost a race (`AlreadyExists`) was `RT-AI-0303` and is now `RT-AI-0307` with the outcome `RT-AI-0301`. The batch exists, so the write did not fail.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostic_Catalogue | Changed | 7.17 table | RT-AI-0307 row | User decision on stale temporary files |

Other sections changed: design catalogue table (section 4.3).

## Directory sync failure after the hard link (8.7 follow-up)

User decision of 2026-11-15. The store writer keeps the whole batch file when the directory sync after the link fails, and the code stays `RT-AI-0303` (exit 70). The Run_Summary now says so.

- The clauses of that batch are listed under a fourth heading, `stored, durability not confirmed`, and not as unprocessed. A separate list was chosen over a marker on `stored` entries, because the `stored` list keeps its meaning (a durable batch, subset of processed) and the section is absent from every run that has no such batch. The summary says `incomplete` while the list is not empty.
- `RT-AI-0305` follows such a batch with blocking findings, because the batch is stored (Req 30 criterion 3). `RT-AI-0306` does not follow because of it: Req 23 criterion 37 names the batch of an earlier request, and the failing request's own file is announced by the `RT-AI-0303` message.
- Amended: Req 23 criteria 36 and 38, Req 29 criterion 19, the `RT-AI-0303` row of the Req 7.17 table. Design 3.7.5 (step 4 and the failure paragraph), 3.7.7 (failure table, Run_Summary paragraph) and the property on incomplete runs.
- Not amended, for a user decision: Req 23 criterion 35 ("byte-identical to its state after the last successful write of the run"), the byte-identical rules of Req 18.17, 27.8, 29.17 and 30.1 and 30.11, and the mirrored text in `.kiro/steering/technical-architecture.md`. They hold for every `RT-AI-0303` except a failed directory sync, and none of them names that case.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.36, 23.38 | - | User decision on the directory sync failure |
| 29 | Provider Call Policy and Proposal_Store | Changed | 29.19 | - | User decision on the directory sync failure |
| 7 | Diagnostic_Catalogue | Changed | 7.17 row RT-AI-0303 | - | Same decision |

Other sections changed: design 3.7.5, 3.7.7 and the incomplete-run property.


## Rule draft `markers` is optional in the local parser (8.8, from the 8.11 review)

A `rule_suggestion` whose rule draft has no `markers` member is accepted and stored with empty markers. The Rule_IR decoder (`rt-ir`, `parse.rs`) treats an absent `markers` as an empty list, and the stored draft is the canonical text with `"markers":[]`, equal in text and Draft hash to the draft that spells the empty list. Test: `tests_accept::a_rule_with_no_markers_member_is_stored_as_the_rule_with_empty_markers`. A mutant that makes `markers` required is killed by it.

Decision (user, 8.8 review): keep the local parser lenient. Req 23.5 now states that the local parser is authoritative (design 3.7.3) and accepts an absent `markers` as empty, while the provider strict schema requires it. The requirement text gained one note and no criterion changed its verdict.


## Task 8.15 final follow-ups

User decisions and corrections of the 8.15 review.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | 23.17: every Provider_Failure message states the attempts made. RT-AI-0104, 0105 and 0108 had no count and now read `<n> attempt(s) failed; the last was <MNEMONIC> (<detail>)`. Code fix, no criterion change |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | 23.33 row 4 and 16.17 step 4: a batch whose file stem is not its internal `request_id` is a malformed store entry, `RT-AI-0302` at that file, exit 2, no provider call (user decision). An oversize batch file is `RT-AI-0302`, not `RT-LIM-0001`. Design 3.7.5 |
| 16 | Minimum End-to-End Vertical Slice | Unchanged | - | - | 16.17 step 4 is "Input load and idempotency check": an input file with a Req 22 key ends the run before the store is read. The 23.33 row order (store before Req 22) is not read as a precedence. Design 3.7.5. Needs user confirmation of this reading |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | `--dry-run` does not apply the provider-link check or the store check: it sends nothing. Design 3.9.2 |
| 5 | Approval and Review State | Unchanged | - | - | User decision (8.16 review): a proposal that reads a sibling's declaration is stored with RT-VAL-0002 and RT-AI-0305 and refused by `review approve` with RT-APR-0205 until the sibling is adopted. Documented in `docs/user-guide/review-order.md`; sibling-aware checking is listed in the design as a possible later feature; task 8.17 carries a note |
| 7 | Diagnostics Model | Unchanged | - | - | User decision: new task 8.22 removes the invented `position: 0` of `CliLocation::Argument` in review diagnostics (Req 7.14). Cli_Location has no numbered requirement of its own: it is a Glossary term, governed by 7.1, 7.13 and 20.13 |


## Task 8.21: revision requests of `ruletrace-ai propose`

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | 23.16: the three options give the `revise` member of design 3.7.2 item 4. Adoption already assigns the next Revision_Number of the rule that the draft names (rt-review `prospective_adoption`), so no review change was needed. Nothing checks that the provider's draft names the revised rule, because 23.16 does not ask for it. Reported to the user |
| 28 | AI Configuration, Credentials, and Transmission Consent | Unchanged | - | - | 28.7 and 28.14: the feedback is not a Source_Document, so `--consent` names no extra identifier for it, and 28.14 lists only the Clause_Context and the task as request content. 23.16 adds the revision content and the feedback to that context. 28.14 should say so. Task 8.21 now lists 28.7, 28.8 and 28.14 |

## Task 8.21 follow-up: oversize batch file in the Proposal_Store

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 19 | Safety of Document Content | Unchanged | - | - | User decision (8.15 review): 19.8 applies 19.3 to Proposal_Store files, so an oversize batch file is `RT-LIM-0001` with the true size from file metadata, exit 2. This reverses the 8.15 reading that listed `RT-AI-0302` for it. Design 3.7.5. `parse_batch` is unchanged and still reports a size fault of the bytes it is given as `RT-AI-0302`, because the review path hands it bytes cut at the limit plus one. That path is a follow-up |

## Task 8.21 follow-up: row order of the Req 23.33 table

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | User decision (8.15 review): the row order of the 23.33 table is not a precedence among failures of one Req 16.17 step, and inputs load before the store check (design 3.7.1, 3.7.5). A note after the table says so. This confirms the reading recorded under "Task 8.15 final follow-ups" and changes no verdict |

## Task 8.21 follow-up: oversize feedback file

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.16 | - | User decision: the feedback file has an 8,192-byte limit, recorded in 23.16 and in design 3.7.2 item 4 and 3.9.2. A larger file is `RT-LIM-0001` (Req 19.3) with the size from file metadata, exit 2, not `RT-AI-0304` with a message that names no size. Expected values changed: code `RT-AI-0304` to `RT-LIM-0001`, message `the feedback file given by --feedback-file is larger than 8192 bytes` to `input file '<path>' is <size> bytes, larger than the effective limit of 8192 bytes`. Design 3.9.2 also says that a Reviewer may paste text into the feedback file and that the tool cannot detect text from a document that the consent does not name |

## Task 8.21 follow-up: wording of RT-AI-0304

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | 7.17 | - | User decision: the meaning of `RT-AI-0304` names an unknown revision target (Rule identifier or Revision_Number) and invalid feedback (empty or not UTF-8). Same code, severity, exit code and executable. The catalogue, the generated enum, the Req 7.17 table and the design table carry the new text. An oversize feedback file is `RT-LIM-0001` and is not part of this meaning |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.33 | - | Row 1 of the table (step 4) carries the same wording |

## Task 8.21 follow-up: credential inside a Provider_Request

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | 7.17 | - | User decision (blocking): new code `RT-AI-0308` `AI_REQUEST_CONTAINS_CREDENTIAL`, Error, exit 2, `ruletrace-ai`. It is in the catalogue, the generated enum, the Req 7.17 table and the design table |
| 16 | Executable Exit Codes | Changed | 16.17 | - | Row 4 of the order table lists `AI_REQUEST_CONTAINS_CREDENTIAL`. The check runs on the built requests, after the input load and before the Proposal_Store check. Exit 2 |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.33 | - | A new row at step 4: no provider call, no Transmission_Log entry, no store write. The note after the table puts the credential check after the input load and before the store check, and says that a dry run does not apply it |
| 28 | AI Configuration, Credentials, and Transmission Consent | Changed | 28.14 | - | 28.14 says that a Provider_Request carries no credential. Until now a credential in a clause text, the feedback or the revised rule content was sent. The AI_Tool now refuses such a request before the log entry and any send. A dry run reads no credential and is not subject to the check. Design 3.9.2. Existing test `a_credential_in_the_feedback_and_in_the_response_reaches_no_output_or_file`: exit 6 and `RT-AI-0206` become exit 2 and `RT-AI-0308`, because its feedback now holds the credential and the request check precedes any response. It gains the assertion that the provider got no request. The response-echo case keeps its own test, `a_credential_only_in_the_response_is_refused_as_rt_ai_0206`, with a clean feedback. Trade-off: a short or low-entropy credential that occurs in the fixed request text refuses every run |

## Task 8.21 follow-up: a revise run whose draft names another rule

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | 7.17 | - | User decision: new code `RT-AI-0207` `AI_REVISION_TARGET_MISMATCH`, Error, exit 6, `ruletrace-ai`, in the catalogue, the generated enum, the Req 7.17 table and the design table |
| 16 | Executable Exit Codes | Changed | 16.17 | - | Row 8 (schema validation) lists the new code. Exit 6 |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.16 | - | A rule suggestion that names a Rule identifier other than `--revise-rule` rejects the whole batch. The check fills the gap that design 3.7.2 item 4 recorded as "nothing checks that". Place in the order: after the checks of criterion 5 (0203, 0201, 0205, 0204, 0202, which are decided inside `parse_response`), before provenance (RT-PRV) and before the credential check (RT-AI-0206). Design 3.7.4 step 2a. A response with a schema violation stays `RT-AI-0202`, and a draft of another rule with bad provenance is `RT-AI-0207` (exit 6) and not `RT-PRV-0104` (exit 3), because it is a protocol failure of the batch. The message does not repeat the identifier of the draft |

## Task 8.21 follow-up: review notes on RT-AI-0206, RT-AI-0207 and RT-AI-0308

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | 7.17 | - | `RT-AI-0206` `AI_RESPONSE_CREDENTIAL_ECHO` was in the catalogue and the design table and missing from the Req 7.17 table. It is added with the catalogue meaning, exit 6 and `ruletrace-ai`. No code, severity or exit code changes |
| 30 | Proposal Batch Acceptance and Protocol Failures | Changed | 30.1 | - | The list of Protocol_Failures that reject the complete batch names `RT-AI-0206` (credential in the response) and `RT-AI-0207` (revision target). Criterion 29.1 is the call policy and holds no such list. Both codes already reject the whole batch, exit 6, and store nothing. The behavior is unchanged |
| 23 | AI-Assisted Proposal Pipeline | Unchanged | - | - | Design 3.9.2 no longer says that `RT-AI-0308` can name "the name of another request member". The scan covers all canonical bytes, keys included. A match in a key or in the fixed head names `requests[n]`. The message of `RT-AI-0308` adds a note for a match outside a clause, the feedback and the revised content. No criterion or catalogue meaning changes |

## UD-15 amendment: macOS runner label (task 8.16, 2026-11-15)

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 21 | Cross-Platform CI | Changed | 21.10 | - | GitHub rejected `macos-14` on run 43720278618 (every macOS job cancelled with zero steps). User decision: the macOS label is `macos-15`. `ubuntu-24.04`, `windows-2022` and the toolchain pin 1.99.1 are unchanged. No macOS job ever ran. Updated: `ci.yml`, design 3.6.3 and 7.4 text, tasks 1.1 and the CI task, UD-15 section 5 |

## Windows directory sync (8.16 platform verification)

User decision: skip the directory fsync on Windows, consistently with UD-27, and keep the status explicit.

- `StoreFs::sync_dir` returns `DirSync::Synced` or `DirSync::SkippedByPlatform`, or an error. `StdFs` syncs on Unix and skips elsewhere. A skip is a successful store, exit 0, no diagnostic. `RT-AI-0303` stays reserved for a real failure, and a PermissionDenied from a Unix sync is still `RT-AI-0303`.
- The skipped clauses are processed and are listed in their own section, `stored, directory durability not confirmed (skipped by platform)`, and not in `stored`. This keeps the earlier decision that `stored` lists durable batches, and mirrors the failed-sync section. The section does not make the run incomplete. No Warning code is added, because the Run_Summary on standard output is the status channel and a constant per-platform notice would need a catalogue entry and change no outcome.
- `RT-AI-0306` counts a skipped batch as kept, like any batch the run wrote.
- Amended: Req 23 criteria 35, 36 and 38, Req 29 criterion 19, the `RT-AI-0303` row of the Req 7.17 table, design 3.7.5 step 4, 3.7.7 (failure table, Run_Summary paragraph) and the property on incomplete runs.
- Windows limit, stated plainly: a power loss shortly after a store write may lose the directory entry of the batch, and RuleTrace does not claim otherwise. File content is still flushed before the link.
- `rt-cli` `Fs::sync_dir` for the ledger commit already returns `Ok(())` on Windows without syncing. It has the same skipped-versus-confirmed ambiguity in type, but the ledger commit reports no durability status, so nothing is claimed. It was not changed.

## Directory sync skip Warning and summary header (8.16 platform verification)

User decisions: the skip gets a Warning code, the Run_Summary header says so, and CI runs every test on every platform.

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| 7 | Diagnostics Model | Changed | 7.17 | - | New row `RT-AI-0309` `AI_STORE_DIRECTORY_SYNC_SKIPPED`, Warning, exit 0, `ruletrace-ai`. It is in the catalogue, the generated enum, the Req 7.17 table and the design table |
| 23 | AI-Assisted Proposal Pipeline | Changed | 23.36, 23.38 | - | 23.36 adds the Warning, its location, the once-per-run rule, the header text and the order. 23.38 adds the Warning and the header to the test of the skipped outcome |
| 29 | Provider Call Policy | Changed | 29.19 | - | The reason for the skip is now stated accurately: the standard library cannot open a directory for a flush on Windows, so RuleTrace does not attempt one. The Warning is named |

- This supersedes the earlier line "No Warning code is added" of the Windows directory sync section. The Run_Summary stays the full status. The Warning makes the skip visible on standard error for a reader who does not look at the summary.
- Location: the command `propose`, as `RT-AI-0306`, because the skip belongs to the run and not to one file. It is emitted once per run, also when a later request fails, and never when no stored batch was skipped. The exit code does not change.
- Order of Warnings: the Diagnostic ordering sorts by location, then code, then message. `RT-AI-0301`, `RT-AI-0305` and `RT-AI-0307` point at input files and come first. `RT-AI-0306` and `RT-AI-0309` point at the command, `RT-AI-0306` first.
- Header: `run summary: complete, directory durability not confirmed` when the run is complete and the skip list is not empty. An incomplete run keeps `run summary: incomplete` with or without a skip list. The failed-sync heading and the skip heading already name the cause, and a second clause on the header would not change what a reader must do.
- Design 3.7.5 step 4 and 3.7.7 (failure table, Run_Summary paragraph, three rendered texts) and the property on incomplete runs are amended.
- Real-file CLI tests that expect the Unix text (`stored:` holding the batch, no Warning) were written for a platform that syncs the directory. They are not changed here and will differ on Windows.

## Task 8.16 audit: model identity masked in the self-approval test

No requirement text changes. In `slice_ai_attacks.rs` the configuration listed the model in `ai_identities`, so a mutant that dropped the model from the Origin identities (`AiIdentitySet::from_origin`) survived. The model is no longer listed, so Req 23.15 and 26.17 refusal of a Reviewer equal to the model rests on the revision Origin. The mutants for the model, provider, proposer, the whole identity check and the `status` forbidden field are killed (Req 23.15, 26.17, 18.14, 20.20).

Open: "green on three platforms" of the 8.16 observable is pending the CI run of the current main and is not claimed. Req 20.20 "no credential value in any output" is not exercised by the mock path, which reads no credential; it stays covered by the tool-level canary tests, not by `slice-ai`.

## Task 8.17: the `ai` profile and the full Coverage_Statement

No requirement text changes. Evidence in `crates/rt-demo/tests/acceptance.rs` (run with `cargo test -p rt-demo --test acceptance ai`), `crates/rt-demo/tests/coverage_store.rs` and `demo/stand-in-ai/`.

| Req | What is shown | Evidence |
| --- | --- | --- |
| 33.6 | Every Candidate_Clause once, by start offset: three `rule_approved`, one `unsupported` (`RT-VAL-0003`, capability `prize_allocation`), one `proposal_rejected` with its reason. Offsets are found in the committed document by the test, not read from the tools. A statement without the store, or without the rejection, fails naming clause 4 or 5 | `ai_profile_acceptance`, `ai_profile_coverage_names_the_unclassified_clause_without_the_store_or_the_rejection`, `coverage_store.rs` |
| 33.7 | Steps s01, s02, s02b, s03a, s03c to s03f, s04 to s07, s06c and s07b of the `ai` profile (the id s03b is retired), each with its command and expected exit code, executed as written | `RUNBOOK.md` block, `ai_profile_acceptance` |
| 33.8 | Both script entries are labelled `authored` in the file, in the manifest `origins` (authored 2, recorded 0) and in `captured`. Each `response_sha256` equals the payload hash, and a changed payload is reported with its Request_Identifier. No credential-looking string. A request the script does not hold gives `RT-AI-0105`, exit 5, and keeps the batch of the earlier request | `ai_profile_acceptance`, `ai_profile_a_script_entry_whose_hash_differs_is_reported_with_its_request_identifier`, `ai_profile_a_credential_looking_string_is_found`, `ai_profile_a_request_the_script_does_not_hold_is_a_provider_error_and_keeps_earlier_batches` |
| 33.9 | The proposal store, the Transmission_Log and every other artifact are compared byte for byte with the committed goldens. No network, no credential, empty environment | `ai_profile_acceptance` |
| 33.11 | The ledger holds three approvals and one rejection, written by `ruletrace review`, with reviewer `Demo Reviewer`, which no batch lists as an AI identity | `ai_profile_acceptance` |
| 33.12 | The runbook block is the executed flow | `ai_profile_acceptance` |
| 23.25 | The accepted response payload and its hash sit in the stored batch of the matching script entry | `ai_profile_acceptance` |
| 8.21 / 8.16 note | `rank.standing` approved before `score.weighted` is refused with `RT-APR-0205`, exit 3, and ledger and Rulebook are unchanged | `ai_profile_a_rule_that_reads_an_unadopted_target_is_refused_and_nothing_changes` |

Decisions and deviations, all for the user:

- A new demonstration directory, `demo/stand-in-ai`. The five-clause document keeps the three clauses of `demo/stand-in-hand` at the same offsets and adds clause 4 (prize money, unsupported) and clause 5 (junior by age, rejected). The `hand` document and its goldens are untouched (confirmed with `git status`). Because the document hash is part of every content hash, the `ai` and `hand` compiled artifacts differ in bytes, so the origin-independence assertion of task 8.18 ("equal Rule_Content") needs either a shared document or a decision on what is compared. The `ai` and `hand` Outcomes are equal and the test asserts it.
- Order of `review list`. Resolved by user decision and applied: `review list` on a missing ledger acts as a read-only operation over an empty ledger, shows prospective revision numbers and creates no file (Req 26.6 amended; tests in `rt-cli` `tests_review_list_missing.rs`). Only `approve` or `reject` with `--create-ledger` may create a ledger (Req 33.11). The `ai` runbook now runs the first listing as step s02b, before the first approval; the former s03b (listing after the first approval) is removed. `ai_profile_acceptance` 5a2 asserts the order, the five listed rules and that the listing creates and changes no file. The `hand` runbook has no `review list` step and is unchanged.
- The unsupported proposal is neither approved nor rejected: approving is refused for a blocking Diagnostic, and a rejection would class the clause `proposal_rejected`. It stays pending and is classed `unsupported`.
- The validator message of an unrecognized draft names the kind (`unrecognized`), not the capability. The generator takes the capability from the `name` of the stored draft.
- Content hashes in the approve steps come from `review list`; the tool refuses a stale one (`RT-APR-0203`). They are not independent of the tool.
- The Windows output of s02 differs (Warning `RT-AI-0309`, Run_Summary header and heading). `golden/steps/s02.stdout.windows` and `s02.stderr.windows` were derived by hand from the `RT-AI-0309` wording and ordering stated above and are checked on Linux only against hand-written literals. They have not run on Windows.
- `ai/mock-replay.toml` is made owner-only in the work copy on Unix, so `RT-AI-0009` does not depend on the checkout umask.
- CI: the `core-only` job skips tests named `ai_profile_` (the `ai` profile needs `ruletrace-ai`); `demo-offline` runs `--test acceptance` on the three platforms. Also moved `actions/checkout` to v5 and `actions/upload-artifact` to v6 (Node 20 deprecation).

Open: "green on three platforms" is pending the CI run and is not claimed. The Proof_Report replay check is asserted in the `hand` acceptance test only; the `ai` test checks that every section is verified.

## Task 8.18: AI independence and origin independence

No requirement text changes. Evidence in `crates/rt-demo/tests/ai_independence.rs`, `crates/rt-demo/tests/origin_independence.rs`, the block 5e2 of `crates/rt-demo/tests/acceptance.rs` and `demo/stand-in-ai/rulebook/hand-authored.json`.

| Req | What is shown | Evidence |
| --- | --- | --- |
| 17.7, 17.8, 18.1, 27.9 | Property 41, first half. Eight cells (`ruletrace-ai` beside `ruletrace` and on `PATH`, an AI_Configuration file in the working and home directories and named by `RULETRACE_AI_CONFIG`, a canary value in three credential variables). Each cell runs the eleven steps of the `hand` runbook and five more commands (exit 2 for an unknown command, a missing input file and `review approve` on a missing ledger without `--create-ledger`, exit 3 for a Rulebook with no Effective_Revision, and a read-only `review list` on a missing ledger with exit 0 that creates no file). Every step is compared with its committed golden files (stdout, stderr, exit code, output files) and every cell with the first cell, including the set and bytes of all files written. The canary never appears in an output | `ruletrace_output_is_identical_in_all_eight_combinations`, `the_cells_differ_in_what_is_visible_to_ruletrace` (the cells really differ in what is visible) |
| 20.16 | Property 41, second half. The same test runs in the `core-only` job (its `cargo test` and the explicit `RT_BIN_DIR` loop) against a build without the AI crates, and in the `full` pass against a build with them. Both compare with the same goldens, so equal goldens give equal bytes. Without `ruletrace-ai` the "present" cells use an inert file of that name (printed by the test) | `ci.yml` `core-only`, local simulation with a `ruletrace`-only build |
| 21.8, 21.13, 27.5, 27.6 | `check-tree` over all 25 workspace crates (including `rt-cli` and every core crate) and the explicit `rt-cli` tree grep in `core-only`. Run by the acceptance tests and in the gate set. `ruletrace` starting `ruletrace-ai` is shown only through the cells (a launched tool would change the bytes); the absence of a process spawn is covered by the source scan | `ai_profile_acceptance` 5f, `rt-xtask check-tree`, `scan-sources` |
| 16.18 | A hand-authored Rulebook over the `stand-in-ai` document (three rules, origin `manual`, same Revision_Numbers and Rule_Content, other name) compiles and evaluates to bytes equal to the `ai` goldens, and equal to the live AI-origin run. The three ledger records are byte equal to the first records of the `ai` ledger. A changed threshold is refused with `RT-APR-0203` (content hash binds content) | `hand_authored_rulebook_over_the_ai_document_gives_the_ai_bytes`, `a_changed_rule_is_refused_with_the_original_content_hash`, `ai_profile_acceptance` 5e2 |

Decisions and deviations:

- Property 41 is an exhaustive 2x2x2 matrix over fixed commands, not a property test with 256 generated cases. The domain is three booleans, so exhaustion is complete; no generated input would add cases. The user approved this (demo follow-up). The Outcome-equality assertion between the `ai` and `hand` profiles remains.
- Origin independence (orchestrator decision): the `hand` and `ai` profiles use different documents and their compiled bytes differ legitimately, because the document hash is part of the content hash. 16.18 is tested with `hand-authored.json` in `demo/stand-in-ai`. The file is synthetic and hand authored (stand-in document, no new demonstration).
- The golden-based origin test needs no `ruletrace-ai`, so it runs in `core-only`. The live comparison with the AI-origin flow is in `ai_profile_acceptance`, which `core-only` skips.
- Mutant evidence (all reverted): `ruletrace` writes to stderr when `OPENAI_API_KEY` is set, writes to stdout when `ai.toml` exists, writes to stdout when `ruletrace-ai` is on `PATH`: each fails the matrix naming the cell and step. Removing the origin normalisation of the Rulebook hash, or hashing AI-origin differently, fails the `ai` goldens (`ai_profile_acceptance`). Hashing differently when an origin is `manual` fails `hand_authored_rulebook_over_the_ai_document_gives_the_ai_bytes` (first difference at byte 92) and block 5e2, with the `ai` goldens still passing. The origin tests alone do not detect an AI-only change that also changes the `ai` goldens; the goldens detect it.

Open: the `core-only` job and the three-platform `demo-offline` job have not run on the new tests. A Windows run is untested (the test compiles for `x86_64-pc-windows-gnu`; it sets `HOME` and `USERPROFILE` and clears the rest of the environment as the other demo tests do). `ruletrace` is checked not to launch `ruletrace-ai` only by output equality and the source scan.


## Checkpoint evidence (run 43743980937, commit b818ca3, 2026-11-15)

CI run 43743980937 (workflow `ci`, commit b818ca3) concluded `success`; `gh run view 43743980937 --json jobs` lists 22 jobs, all `success`. Ids: fmt 132903111605 (ubuntu), 132903111580 (windows), 132903111614 (macos); clippy 132903111686, 132903111660, 132903111832; test 132903111547, 132903111641, 132903111636; golden 132903111682, 132903111847, 132903111629; toolchain 132903111628, 132903111595, 132903111355; demo-offline 132903111734, 132903111741, 132903111787; graph (linux) 132903111684; deny (linux) 132903111755; core-only (linux) 132903111620; ci-gate 132903985425.

Local, same session, exit 0 each: `cargo fmt --all -- --check`; `cargo run -p rt-xtask -- check-graph check-tree scan-sources scan-api check-catalogue check-justification check-decisions`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` (1420 passed, 0 failed, 0 ignored).

| Task | Result | Evidence |
| --- | --- | --- |
| 2.14 | Complete | Properties 4 (`rt-foundation/tests/diagnostic_state_flags.rs`) and 7 (`diagnostic_order.rs`) pass. `graph` job and local xtask run pass, including the negative fixtures of rt-xtask. Three-platform CI green |
| 4.18 | Complete | Properties 1, 5, 9, 12, 13, 14 pass (`span_reproduction`, `contradictory_flags*`, `status_derivation_properties`, `hash_sensitivity_tests`, `hash_invariance_tests`, `ledger_properties`). `worked_example_matches_golden_hash` passes. `rt-ledger` test scans its source for `fn append` and `fn push_record`. `golden` job green on three platforms |
| 5.28 | Complete | Properties 2, 8, 10, 11, 20, 23, 24, 25, 26, 30, 40 pass (`stage_determinism`, `locations_never_fabricated`, `unresolved_blocks_compile`, `compiled_set_properties`, `missing_values`, `ranking`, `input_order`, `trace_replay`, `provenance`, `exit_code_stage_order`, `isolation`). `core-only` and `golden` jobs green. Five slice fixtures compared on three platforms |
| 6.12 | Open | Demonstration checkpoint: needs the user to see the printed summary and review. Not claimed. Update (demo follow-up): the command in the task text was wrong and is corrected; the summary was extended, see the section below |
| 8.16 | Complete | `slice_ai` and `slice_ai_attacks` ran in the `test` jobs on ubuntu, windows and macos (jobs above), both attack fixtures leave the ledger unchanged |
| 8.17 | Open | Update (demo follow-up): the runbook now lists before the first approval (s02b); see the section below. Earlier text: The `ai` profile acceptance test (7 tests) is green in `demo-offline` on three platforms, so the `.windows` goldens have now run on Windows. The order of `review list` is decided: `list` may precede the first approval (Req 26.6 amended). Missing: the demo `ai` profile and runbook still list after the first approval; they change in a separate step |
| 8.18 | Open | Update (demo follow-up): the user approved the 8-cell matrix and the origin test; both were strengthened, see the section below. Earlier text: `ai_independence` and `origin_independence` pass in `core-only` and `demo-offline`. Missing: user confirmation that an exhaustive 8-cell matrix replaces the 256 generated cases (policy section 3 and 7 forbid reducing the count without a specification decision), and for the origin test using a hand-authored Rulebook over the `ai` document |
| 8.19 | Open | Update: still blocked by the review of 8.17 and 8.18 and by a CI run of the changed tests. Earlier text: Blocked by the open 8.17 and 8.18. Other criteria below are met |

8.19 criteria checked:

- Properties 31 (`rt-ai-tool` `tests_identifier_property`, `rt-proposals` `tests_identifier_property`), 32 (`rt-ai-adapter` `tests_retry`), 33 (`tests_batch_property::every_single_mutation_rejects_the_whole_batch_and_stores_nothing`), 34 (`rt-tests-ai/tests/origin_property.rs`), 37 (`tests_redaction_property`), 38 (`tests_config_order`) pass. Property 41 (`ai_independence::ruletrace_output_is_identical_in_all_eight_combinations`) passes as an 8-cell matrix, not 256 cases.
- Script entry keyed by the identifier `--dry-run` prints: `replay_script_is_keyed_by_the_request_identifier` and `ai_profile_acceptance` (Req 33.8). Accepted response bytes in the stored batch: `ai_profile_acceptance` (Req 23.25).
- Every clause classified or named: `ai_profile_acceptance` and `ai_profile_coverage_names_the_unclassified_clause_without_the_store_or_the_rejection`.
- Steering items 1 to 4 and 9 of design 9.3 are merged: commits b57c9e7 and 6500ffa, text in `.kiro/steering/technical-architecture.md` (AI bullet, revisions bullet) and `rust-engineering-and-testing.md`. Item 5 is deliberately untouched (DD-2).
- `core-only` job green with the AI crates in the workspace.

Known limitations:

- macOS golden `NotFound` flake: mitigated by bf25ce2 (one build of the `ruletrace` binary per test process; the cause was not reproduced on Linux), not proven. It was seen once, in run 43727951886 (`golden (macos-15)`, `test (macos-15)`). In the four runs after the mitigation (43735573572, 43736966780, 43739958693, 43743980937) all 6 macOS jobs of each run succeeded (checked with `gh run view`). Run 43735573572 failed on `test (windows-2022)` only (a Windows file-size test and `slice_ai` tests, fixed later), not on macOS. Four clean runs do not prove the cause.
- Property 41 is an 8-cell matrix, pending user confirmation.
- The `.windows` goldens of the `ai` profile are hand-derived. They pass on windows-2022 in run 43743980937 but have no independent source.
- The live AI-origin comparison runs only in `ai_profile_acceptance`, which `core-only` skips.

## Demo follow-up (user decisions on 8.17, 8.18, the demo exit codes and 6.12 and 8.20)

No requirement text changes. Product direction unchanged: no IR expansion, no new feature, no new dependency. 6.12, 8.17, 8.18, 8.19 and 8.20 are not marked complete.

**Review list (8.17).** The `ai` runbook runs `ruletrace review list` as step s02b, after `propose` and before the first approval. The step s03b (listing after the first approval) is removed and its id is retired. The ids s03a and s03c to s03f are unchanged. The `hand` runbook has no `review list` step and did not change. Golden changes of the `ai` profile (all in `demo/stand-in-ai/golden/steps/`, blessed with `RT_DEMO_BLESS=1` after comparing):

| Golden | Change | Reason |
| --- | --- | --- |
| `s03b.stdout`, `s03b.stderr` | Removed (old content: four pending proposals, `score.weighted`, `rank.standing`, `prize.split`, `junior.flag`; stderr empty) | The step is retired |
| `s02b.stdout` | New. Equal to the old `s03b.stdout` with four lines added in front: the pending proposal `a632f94f...` for `elig.entry`, its origin line, its prospective content hash `9edaa59e...` and its span `161..241` | `elig.entry` is still pending before the first approval |
| `s02b.stderr` | New, empty | As the old `s03b.stderr` |
| `s03a`, `s03c` to `s03f`, `s02` (and `s02.*.windows`), `s04` to `s07`, all artifacts | Unchanged bytes (`git status` shows no modified golden) | The listing writes nothing, so nothing downstream changes |

Platform: s02b runs `ruletrace` only and prints no path, so it has no `.windows` variant. The `s02` goldens and their `.windows` variants are unchanged because `propose` runs before the listing and the listing does not change the store. No new golden needs a hand-derived Windows variant.

**Complete data and happy path.** New `data/canonical/data-complete.json` in both profiles (same file bytes, sha256 `8034be20...`): the existing data with `"age": "20"` for `p2`. The manifests list it (synthetic, `seeded_difference` false). New steps s06c (`evaluate`, exit 0) and s07b (`evaluate --render --proof-report`, exit 0) in both profiles. New goldens only: `s06c.stdout`, `s06c.stderr` (both empty), `s07b.stdout` (the rendered traces), `s07b.stderr` (empty), `results-complete.json`, `results-complete.proof-run.json`, `proof-complete.txt`, `proof-complete.json`. No existing golden changed. Both profiles' `results-complete.json` differ only in the document hash, as the existing results do. The new steps run no `ruletrace-ai`, so they are platform neutral. Indeterminate scenario (s06, s06b, s07, exit 4) is unchanged and stays separate.

Expected values, worked out by hand before the run (score = 0.6 judges + 0.4 technical): p1 4.8 + 2.8 = 7.6, p2 4.2 + 3.6 = 7.8, p3 3.6 + 2.4 = 6, p4 5.4 + 2.4 = 7.8, p5 3.6 + 2.4 = 6. Eligible (age 16 or more and licence): p1, p2, p4, p5; p3 (15) is not. Standing: p2 (7.8, technical 9), p4 (7.8, 6), p1 (7.6), then p3 and p5 tie on 6 and technical 6. The tool produced exactly these values. `acceptance_hand.rs` 5f and `acceptance.rs` 5e3 assert them as literals (and `acceptance_hand.rs` also recomputes the scores from the data with its independent decimal model), exit 0, all 11 Outcomes decided, an empty standard error, no Diagnostic, 11 of 11 traces verified, `results-complete.proof-run.json` equal to `results-complete.json`, and that the data differ from `data.json` only by the age of `p2`.

**Summary text (6.12, 8.20).** `rt-demo` gains `summary.rs` (`describe_evaluate`, `describe_ledger`, `diagnostic_codes`) and `run_runbook_recorded`. Both acceptance tests print, for every `evaluate` step: each question's Outcome, each Indeterminate reason, the diagnostic codes of standard error and the verification status. The `hand` summary adds the stand-in and no-AI statement (asserted: no `ruletrace-ai` step, `provider_script` null, every rule origin `manual`), the document, and the approvals with reviewer, time and content hash. The `ai` summary adds the stand-in and authored-script statements, each proposal with its Request_Identifier and source span, every review step with the rejection reason, the ledger decisions, the Coverage_Statement and the equality of the AI-origin and hand-authored compiled artifacts and results (bytes and sha256). Task 6.12 now names `--test acceptance_hand`.

**Task 8.18.**
(a) `ai_independence.rs` takes its before/after snapshot of the whole cell directory, so the planted files (`ai.toml` in the working and home directories, the `ruletrace-ai` copies, the demonstration inputs) are included. It asserts the planted files are in the snapshot, unchanged in bytes after the run, and that produced files and directories lie under `work/` and are equal across the eight cells.
(b) The canary (a string, `rtc-Zq7x...`, never a credential) is scanned, whole and as any 8-byte fragment (the width of the redactor), in stdout and stderr of every command, every file of the cell directory, and every path name, in all eight cells. The `ai` profile has a separate test, `ai_profile_a_canary_credential_reaches_no_surface_of_the_runs`: the whole runbook runs with the canary in three credential variables, outputs still equal their goldens, and stdout, stderr, the proposal store, the Transmission_Log, the ledger, the run summary and all other files and paths are scanned. `ai_profile_a_configuration_with_an_inline_credential_is_refused_without_echo` runs `propose` with the canary as an inline `api_key`: exit 2, `RT-AI-0003`, nothing echoed, nothing stored. The mock provider names no credential, so the canary cannot be read by the real credential path in this profile; the real redaction is covered by the `rt-ai-tool` tests, and no provider was contacted.
(c) The 8-cell matrix stays exhaustive (user approved). The CI step "Core-only build holds no ruletrace-ai" fails when `target/core-only/debug` holds `ruletrace-ai` or an AI tool artifact (checked locally on a core-only build in a separate target directory: exit 0; with a planted file: exit 1). The core-only simulation ran the four demo tests with `RT_BIN_DIR` pointing to it: all pass with the inert file.
(d) `origin_independence.rs` has a second test that asserts, field by field: the document bytes and sha256 (manifest, Source_Model, both Rulebooks, compiled artifact, ledger binding), Source_Model and Candidate_Clauses bytes, rule identifiers, Revision_Numbers, Rule_Content (every rule member except `origin`) and spans of the two Rulebooks (they differ in `origin`, asserted as `manual` and `ai_proposal`, and in the Rulebook `name`), spans against offsets derived from the document text, the three ledger records, the compiled bytes and rule spans, and the Outcomes over both data files. Previously asserted only as byte equality of compiled and results plus ledger lines.

**Baseline defect found.** `ruletrace_output_is_identical_in_all_eight_combinations` failed in the tree before this work: its third failing command expected `review list` on a missing ledger to exit 2, which the amended Req 26.6 (commits d50a8ce, 2834071) no longer does. The expected result is changed on the specification's authority, with strength kept: the command now is `review approve` on a missing ledger without `--create-ledger` (still exit 2, `RT-CLI-0003`), and `review list` on a missing ledger is a separate command asserted to exit 0 with `status Proposed` on stdout, empty stderr and no file created.

**Mutants (all reverted, `git status` clean afterwards):**

| Mutant | Result |
| --- | --- |
| `data-complete.json` of `hand` with `p2` age 14 and bless mode (goldens bypassed) | `acceptance_hand` fails at 5f, `eligible p2` is `false` where `true` is required |
| `data-complete.json` of `ai` with `p4` judges 8 and bless mode | `ai_profile_acceptance` fails at 5e3 ("s06c has diagnostics", the published result differs) |
| `describe_evaluate` counting non-verified traces | unit test `evaluate_description_names_every_outcome_reason_code_and_verification` fails |
| `ruletrace review list` writes `work/stray.txt` | `ai_profile_acceptance` 5a2 fails: "`review list` changed or created a file" (the goldens do not see the file) |
| `ruletrace` writes `OPENAI_API_KEY` to `work/stray-leak.txt` | the matrix fails in cell `tool=0 config=0 credential=1`: `file root/work/stray-leak.txt at byte 0` |
| `ruletrace` appends to `ai.toml` in the working directory | the matrix fails in cell `tool=0 config=1 credential=0`: `planted file root/ai.toml changed` (the previous test excluded planted files and could not see this) |
| `first_leak` start condition `<` to `<=` | property `search_agrees_with_the_reference_on_random_inputs` fails (512 cases) |
| hand-authored span `242` to `243` | both origin tests fail (`RT-APR-0203`, the content hash binds the span) |
| injected leaks in the test's own copy of the surfaces (stderr, file, path; seven bytes not a leak; planted file changed, removed) | detected with the exact surface name, `the_canary_check_detects_...`, `the_planted_file_check_detects_...`, `ai_profile_a_canary_...` |

One test fixture of mine was wrong at first (`rtc-Zq7x` is eight bytes, not seven); the fixture was corrected to a real seven-byte piece, and no assertion was weakened.

**Limitations and open points.** No CI run of these changes yet (three platforms, `core-only`, `demo-offline`). Windows: the new code compiles for `x86_64-pc-windows-gnu` (`cargo check --all-targets`); nothing ran on Windows. `ai_independence` takes about 30 to 45 s on Linux in debug (it reads and scans the executables copied into each cell). A refused `review approve` on a missing ledger leaves a `<ledger>.lock` file next to it (seen in a scratch run; the matrix treats it as produced output and compares it across cells); whether this is intended is a question for the owner of `rt-cli`. In the `rank.standing` ranking, p2 (Indeterminate eligibility) ranks first and p3 (ineligible) ranks last in the tie, because the ranking is not gated by eligibility; the documents do not say otherwise, and this is not changed here.

## Closure of 8.17, 8.18 and 8.19 (run 43751816991, commit 15be01d, 2026-11-15)

- CI: all 22 jobs succeeded on ubuntu-24.04, windows-2022 and macos-15.
- Independent reviews: `docs/reviews/2026-11-15-200623-review-ddfdb6d-c46d61a.md`, `2026-11-16-120000-review-f4e0250-9724798.md`, `2026-11-16-130000-review-a5dc07b-2834071.md` and `2026-11-15-221856-review-2834071-fda9f37.md`, all with no open blocking finding.
- User decisions confirmed: `review list` before the first approval (Req 26.6 amended), Property 41 as an exhaustive 8-cell matrix, origin independence against a hand-authored rulebook over identical source bytes.
- Still open: 6.12 and 8.20, which need the user's review of the transcripts.
- Known limits: `.windows` goldens are hand-derived; `rank.standing` is not gated by eligibility; a refused `approve` leaves an empty `.lock` file; the macOS golden flake mitigation is unproven (no recurrence in six runs).

## Closure of 6.12 and 8.20 (run 43754895503, commit df167a5, 2026-11-16)

- CI: all 22 jobs succeeded on ubuntu-24.04, windows-2022 and macos-15.
- Transcripts reviewed and accepted by the user: `docs/transcripts/demo-slice-0.txt` (`cargo test -p rt-demo --test acceptance_hand -- --nocapture`) and `docs/transcripts/demo-slice-1.txt` (`cargo test -p rt-demo --test acceptance ai -- --nocapture`), both exit 0.
- Presentation correction: ranking decisions of the Proof_Report carry the line "Score ranking — all submitted participants; eligibility is evaluated independently and is not applied as a ranking filter." Regression test `standings_decision_states_that_ranking_covers_all_submitted_participants` plus demo assertions; review `docs/reviews/2026-11-16-140000-review-58ad25b-af8ffcf.md` (approved).
- Mock-provider disclosure: the Slice 1 summary states "script entries are authored (not provider output); the document is a stand-in". The Slice 0 summary states that no AI was used.

## UD-37 amendment: Meta Model API over the Responses protocol (spec only, 2026-11)

Decision record: `tools/decisions/UD-37.md` (Confirmed, user). At that commit no Rust code, `Cargo.toml`, `deny.toml`, `allowed-edges.v1`, `ci.yml` or catalogue file changed. Nothing is weakened: every earlier check keeps its code, severity and exit code, and Req 29.2 was unchanged then (superseded by the sections below).

| Req | Title | Status | Changed criteria | New criteria | Cause |
| --- | --- | --- | --- | --- | --- |
| Introduction, Scope, Glossary | - | Changed | Scope line on provider integration; AI_Integration_Crate | Provider_Profile, Capability, Provider_Identity, Reported_Metadata, Model_Catalogue | UD-37 D1 |
| 7 | Diagnostics Model | Changed | catalogue table (planned rows RT-AI-0011, 0012, 0013, 0109, 0110) | - | UD-37; the JSON rows are added by task 10.10, because adding them regenerates `code_generated.rs` |
| 16 | Minimum End-to-End Vertical Slice | Changed | 16.15 and 16.17 tables (new codes, `models` command) | - | UD-37 |
| 23 | AI-Assisted Proposal Pipeline | Changed | 29 | 39 (flat provider-neutral schema) | UD-37 D1, D2 |
| 27 | Executables and Responsibilities | Changed | 1 (table), 2, 11 (`models`) | - | UD-37 |
| 28 | AI Configuration, Credentials, and Transmission Consent | Changed | 2, 8, 12, 17 | - | UD-37 D4 (provider id and protocol, check order) |
| 33, 34 | Demonstration | Changed | 33.8, 33.10, 34.5 (provider identifier, protocol, model; Responses profile) | - | UD-37 |
| 35 | Provider Profiles, Capability Negotiation, Model Catalogue, and Proposal_Store Format Version 2 | New | - | 1 to 22 | UD-37 D1 to D5 |

Design: §1.3 row 14, §3.2, §3.3 (renamed crate in the allowed-edges, `forbid-external`, `wrappers` and core-only job text), §3.4.5, §3.7.3 (flat schema, Meta profile, 504 interplay), §3.7.5 (store version 2), §3.9.2 (configuration version 2, `models`), §8.11.6 and §8.11.7 (script `id`), §9.1 (R-D1 resolved, R-D16), §9.2 (UD-37), §9.3 item 11, catalogue table.

Tasks (Phase 10): `rt-ai-openai` is `rt-ai-responses` in 10.1 to 10.9. Task 10.4 is required and first. New tasks: 10.10 (catalogue entries), 10.11 (capability model, identity, usage), 10.12 (Provider_Profiles), 10.13 (configuration version 2, host restrictions, model guard), 10.14 (store version 2), 10.15 (Meta stub tests), 10.16 (`models` and `verify_model`), 10.17 (user review gate before the live run). 10.8 is blocked by 10.17. Waves 44 to 60 were renumbered. The Notes line that listed 10.4 as optional is corrected. Task 10.7 is unchanged and still blocks 10.8. 10.7 is implemented locally (guard in the demo runner, examples, RUNBOOK section) and awaits review and CI (see its status note).

Deferred, with the reason: Modal (UD-37 D6), a Chat Completions adapter (`rt-ai-compatible`, only for a real use case), custom compatible endpoints (need their own trust decision), the OpenAI profile's reasoning effort and usage capabilities (unknown until verified), the catalogue JSON rows (task 10.10).

Resolved: OD-37-1 (HTTP 504 and Req 29.2) by the user's decision of 2026-11-16, option B (see the HTTP 504 note below); the loopback exception for the ids `meta` and `openai` is the existing Req 28.6 behaviour at run time, not a test-only build.

## Phase 10 progress: tasks 10.4, 10.10, 10.11 and 10.14 (local evidence, 2026-11)

Status: implemented and run locally on Linux. All four tasks stay `[-]` until the CI run on the three platforms and the independent review. No requirement text was changed.

| Task | Requirement | Production code | Tests (evidence) |
| --- | --- | --- | --- |
| 10.4 | 23.39, 23.27, 23.4 | `rt-ai-tool` `proposal_schema.rs`, `response/flat.rs`, `response/closed.rs` (`check_flat`), `response.rs` (version "2") | `tests_proposal_schema.rs` (independent counter, golden `tests/fixtures/proposal-schema/proposal-schema-v2.json`), `tests_flat.rs` (depth 16 and 17, 256-case properties, table refusals, schema conformance) |
| 10.10 | 7 (catalogue), 35.3, 35.6, 35.8, 35.18, 35.19 | `catalogue/diagnostics.v1.json`, `rt-foundation/src/code_generated.rs` | `rt-foundation/tests/catalogue_ud37.rs` |
| 10.11 | 35.1, 35.2, 35.11, 29.7, 18.12 | `rt-ai-adapter` `identity.rs`, `capability.rs`, `adapter.rs`, `request.rs`; `rt-ai-mock` `AdapterInfo` | `tests_capability.rs`, `tests_identity.rs`, `tests.rs`, `rt-ai-mock/src/tests_info.rs`, `compile_fail,E0639` doctest |
| 10.14 | 35.15, 35.16, 35.17, 29.10 | `rt-proposals` `model.rs`, `parse.rs`, `print.rs`; `rt-ai-tool` `store.rs`, `run.rs`, `command.rs`, `config.rs` | `rt-proposals/src/tests_v2.rs`, `rt-ai-tool/src/tests_store_v2.rs`, `rt-cli/src/tests_review_adopt.rs`, `tests/fixtures/proposal-store-v1/` |

Changed expected values and goldens, disclosed:

- Two existing tests rejected `schema_version` "2". Version "2" is valid since 10.4, so `tests_response::schema_version_must_be_one_or_two` and the `BadVersion` case of `tests_batch_property` now use "3". No other assertion changed.
- The two Proposal_Store batch goldens of `demo/stand-in-ai` changed from `"version":1` to `"version":2` and gained `"protocol":"mock-replay",` after `adapter_version` (+25 bytes each). Request identifiers, file names, payloads, hashes, proposals and the recorded script are unchanged. The old bytes are kept as the fixtures of `tests/fixtures/proposal-store-v1/`.
- Mechanical migrations without a changed expected value: `ProviderMeta` literals use the constructor, eight `ProviderRequest` literals gain `reasoning_effort: None`, ten `Batch` literals gain `format: Format::V1`, and two mock assertions read `identity.provider.as_str()`.

Deviations and limits:

- The flat schema has no single `root` member. Each expression field of a rule holds its own node index (design 3.7.3, implementation notes).
- `Required` is the name used for what the orders of this task called `Requirements`, as in design 3.4.5.
- The lowered draft of a version "2" response is decoded with a cap of 61 containers (the batch limit of 64 less the three around a stored rule), not the response container limit of 32, so an expression above 16 levels reaches the validator as `RT-LIM-0003` and a rule above 61 containers is refused with `RT-LIM-0003` before storage. A decoder error inside a lowered draft is located at the rule, with the inner field path of the lowered text.
- The depth of the generated schema is 10 when each `anyOf` alternative, `items` and property counts as a level. The Meta documentation does not say how it counts. If it counts differently, the schema stays within 10 in this stricter count.
- Not verified here: the three-platform CI run, the Windows runs of the new tests (a `cargo check` for `x86_64-pc-windows-gnu` was run), and that the Meta API accepts the generated schema (a live step, not run).

## Approvals of the UD-37 golden changes (recorded 2026-11-16)

Two authoritative goldens changed in the UD-37 work: the `protocol` member of the Transmission_Log lines in `demo/stand-in-ai/golden/artifacts/transmission.log.jsonl` (Req 28.8), and the `description` members of `tests/fixtures/proposal-schema/proposal-schema-v2.json` (see `PROVENANCE.md` there).

Both changes were approved by the orchestrator under the user's standing instruction to take decisions during the user's break. The user has not confirmed them yet. User confirmation is pending. The commit texts that say "user pre-approved" for the transmission log golden overstate this: the user gave a standing instruction, not a decision on this diff. Until the user confirms, both goldens count as provisionally approved.

## UD-37 implementation evidence (run 43799230103, commit a2ce236, 2026-11-16)

- CI: all 22 jobs succeeded on ubuntu-24.04, windows-2022 and macos-15, including the Meta stub tests of `rt-ai-responses` and `rt-ai-tool` on loopback. The first run of this series (43797922991) failed on Windows only, in the `network_guard` scan (backslash paths); fixed in a2ce236.
- Independent reviews: waves 1 to 5 under `docs/reviews/*ud37*`. Wave 3, 4 and 5 returned NEEDS_CHANGES; each finding was fixed or recorded as deferred (tasks 10.1, 10.5). The last fix (loopback endpoint with fragment or padded port, commit 8dcd733) had no further independent review beyond a property test of 512 cases.
- Marked complete: 10.1, 10.2, 10.3, 10.4, 10.10, 10.11, 10.12, 10.13, 10.14, 10.16.
- Task 10.15 (2026-11-16, still `[-]`): Req 35.13 to 35.15, 29.2 and 29.3 are covered by `crates/rt-ai-responses/tests/stub_http.rs` (error bodies are never read: `a_stalled_error_body_is_not_waited_for_in_any_non_2xx_status`, `a_401_or_403_is_auth_failed_with_one_attempt_for_every_body_shape`, `a_401_is_not_retried_even_when_a_success_is_waiting`, `an_oversize_error_body_gives_the_same_failure_as_an_empty_one`) and by `crates/rt-ai-tool/src/tests_host_before_connection.rs` (host refused before any adapter exists); the Meta documentation re-read is in `docs/provider-profiles.md`.
- Still open: 10.5 (the `live-openai` job needs the user's secret name), 10.15 (a few mutants and the live re-read of Meta limits), 10.17 (user review gate), 10.6, 10.7, 10.8 (user-run), 10.9.
- Incident: during wiring, one test run made a single real HTTPS request to api.openai.com with a fake test credential and the synthetic clause "Alpha beta." (no real secret or document text). Tests were fixed and a runtime guard plus a source scan now prevent any test from reaching a non-loopback host.
- Golden changes (approved by the orchestrator under the user's standing instruction, user confirmation pending): the stored batches of the `ai` demo (version 2), the Transmission_Log `protocol` member, and the schema golden (descriptions only).

## HTTP 504 final failure (OD-37-1, user decision 2026-11-16, option B)

- Requirements 29.2 and 35.14 amended: HTTP 504 is not a Transient_Failure for any adapter and ends the request after one attempt with RT-AI-0105 (exit 5); the message names the HTTP status and no provider text. Other 5xx stay transient.
- Tests: `rt-ai-adapter` `tests_retry` (hand table rows, oracle without 504, property tests of 1,024 cases with 504 listed in the failure strategies) and `tests::transient_classification`; `rt-ai-responses` `tests_status`, `tests/stub_http.rs` (real transport and real decision, 500/502/503 retried, 504 final); `rt-ai-tool` `tests_e2e_responses` (one attempt, no wait, simulated clock at zero, nothing stored, one log entry, no provider text) and `tests_e2e_models`; `rt-ai-mock` `tests_retry_after` and the script table.
- Changed expectations (disclosed): the rows and tests that asserted the former behaviour (504 transient, three attempts, `RT-AI-0106`) now assert the decision; the `models` exhaustion test uses a 503 so that it still covers exhaustion.
- Evidence: the new tests failed against the unchanged implementation before the change (adapter: four tests; status: one; e2e: two; stub: one) and passed after it. Mutants B1 to B6 (504 transient as `GatewayTimeout`, as `Other`, mapped to `Unavailable`, mock mapping removed, status missing from the message, 505 made final) are each killed.

## Prompt profile 3 (type instructions; 2026-11-16)

- Cause (evidence pinned in `crates/rt-ai-tool/tests/fixtures/live-run1-prompt-2/`): in the first live call (Meta `muse-spark-1.3`, `responses-1/prompt-2`, request `1c574c2d...`) of five proposals, rule 3 was kind ranking with type `number` (RT-VAL-0001) and read `field score` (RT-VAL-0002), rule 5 was kind `derived` with a boolean expression and type `boolean` (RT-VAL-0001 twice: the expression and the rule type), and rule 4 was correctly `unrecognized` (RT-VAL-0003). Rule 5 was not typed `number`: its type was right for its expression and wrong for its kind.
- Req 35.23 amended (semantic validator named; profile 3 rules listed). `PROMPT_PROFILE` 3, adapter version `responses-1/prompt-3`. Request_Identifier unchanged (the pinned identifiers of `tests_plan` and the authored script still pass).
- Preamble: 11 -> 18 lines. Schema descriptions: 87 -> 173 occurrences, `description` members only (proof: PROVENANCE.md of the schema fixture).
- Instruction -> test: type names exact (`type_names`), type equals kind and node `t` (`type_matches_kind`), ranking type (`ranking_type`), boolean predicate (`boolean_predicate`), numeric score (`numeric_score`), kind not content (`kind_not_content`), no guess (`no_guess`), in `tests_prompt_types.rs`, listed in the rows of `tests_prompt_contract.rs`. Live evidence reproduced by `tests_live_run1.rs`. Sibling target by `rt-tests-ai/tests/sibling_reference.rs`.
- Finding not fixed by this change (open decision): the live ranking read `score` with a `field` node. A `field` node reads a declared input, so RT-VAL-0002 never clears when the score rule is approved; only a `ref` node resolves it. Prompt-3 does not tell the model this.

## Task 10.6 export and offline replay (2026-11-16)

- Code: `crates/rt-proposals/src/export.rs`, `crates/rt-xtask/src/export.rs`, `crates/rt-ai-mock/src/replay.rs` (optional `provider.id`; replay declares `reasoning_effort = yes`), `crates/rt-ai-tool/src/profile.rs` (`MOCK_REPLAY` likewise, user decision 2026-11-16).
- Data: `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json` (one recorded entry, capture text `Meta Model API muse-spark-1.3, reasoning_effort low, captured 2026-11-16 by the user, synthetic stand-in document`), `demo/stand-in-ai/ai/meta-replay.toml`, `demo/stand-in-ai/provider/RECORDINGS.md`. Not in the runbook and not a golden file.
- Tests: `rt-proposals` `tests_export` (exact text, order, determinism, hash mismatch, canary, header disagreement, prompt profiles kept apart, replay batches refused, no fabricated metadata); `rt-xtask/tests/export_script.rs` (live store to the committed recording byte for byte, tampered file names file and request, canary, no other file written, no overwrite, version 1 store read); `rt-tests-ai/tests/replay_live_recording.rs` (same proposals, payload and adapter version as the live batch, no connection to a bound loopback listener, no environment, one log entry without document text, idempotent second run); `rt-tests-ai/tests/request_id_pin.rs` (the live configuration still gives `1c574c2d...` under prompt-3).
- Prompt-3 recording: `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-3.json`, `demo/stand-in-ai/ai/meta-replay-prompt-3.toml`, fixture `crates/rt-ai-tool/tests/fixtures/live-run2-prompt-3/`. Tests: `replay_live_recording.rs` runs both recordings (payload, hash, proposals, adapter version, no endpoint, log, idempotence, merged script and two-script configuration refused); `export_script.rs` (prompt-3 export twice equals the committed file, no overwrite, prompt-2 and prompt-3 batches in one store refused).
- Proposer label (Req 23.12, option A): `rt-cli` `origin_text`; tests `tests_review_adopt` (pending and adopted wording, control characters escaped, manual origin unchanged); goldens `demo/stand-in-ai/golden/steps/s02b.stdout` and `s03f.stdout` (wording only); note in `demo/stand-in-ai/provider/RECORDINGS.md`.
- Changed expectations (disclosed): `tests_config_v2` (an effort for `mock-replay` is now accepted; the RT-AI-0011 case is kept with a lowering declaration), `tests_profile` and `rt-ai-mock` `tests_info` (replay capability `yes`).


## Task 10.17 user review gate (2026-11-16)

- Requirements 35.22, 28.21, 29.2. The user closed the gate by explicit instruction. Record and evidence: `tools/decisions/UD-37.md` section 8; task note in `tasks.md` 10.17.
- Evidence: CI run 43799230103 (commit a2ce236), CI run 43847096409 (commit 438b700), `crates/rt-ai-responses/tests/stub_http.rs`, `docs/provider-profiles.md`, the goldens changed by 10.14 and by the proposer label.
- Not satisfied, closed by user decision: the gate text lists blockers 10.5 and 10.15, which remain open (`[-]`). They are not claimed complete.
- The independent code reviews (`docs/reviews/*ud37*`, waves 1 to 7) are separate from the user review and are listed apart in UD-37 section 8.5.

## Task 10.5 `live-openai` CI job (2026-11-16)

- Requirements 21.14, 35.9. Secret `RULETRACE_OPENAI_API_KEY` and variable `RULETRACE_OPENAI_MODEL`: `tools/decisions/UD-15.md` section 6. Job: `.github/workflows/ci.yml` (`live-openai`, not in `ci-gate` needs).
- Tests: `crates/rt-xtask/tests/live_openai_job.rs` (structure rules and mutants, gate script behaviour with a canary, configuration step with hostile model texts). Regression: a model value with an embedded line feed passed a `grep -E` check; the bash regex refuses it.
- Unverified: any live run. Task 10.5 stays `[-]`.
- User decision 2026-11-16: credentialed live execution of `live-openai` is DEFERRED and UNVERIFIED. The skip evidence is run 43864502388 (commit a52d67b, latest green), job `live-openai (linux, optional)` 133289465599: `Gate` step success, all later steps skipped. This is a skip, NOT an authenticated request. `tasks.md` 10.5 observable and note amended; the original text is kept in the observable.

## Task 10.8 recorded script (amended by user decision 2026-11-16)

- Requirements 33.8, 33.10, 28.21, 23.27, 35.22. The user decided that `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json` and `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-3.json` satisfy the recorded-script criterion; no duplicate `recorded-script.json` is created. `tasks.md` 10.8 observable and 10.9 exit criterion are amended with the original text preserved.
- Covered by the decision: the recorded-script criterion for the synthetic stand-in document only.
- Deferred, not satisfied: live recording over the UD-34 documents (Kiroween 2026 Official Rules; ICC Men's T20 World Cup 2026 Playing Conditions 16.10.1 to 16.10.7), which need the snapshots of task 11.1; also the 10.7 guard (open), the observed `max_output_tokens` need in `docs/provider-profiles.md` (still provisional 8192), and the optional `muse-ember-1.0` benchmark. Task 10.8 stays `[ ]`.

| | prompt-2 | prompt-3 |
|---|---|---|
| File | `recorded-meta-muse-spark-1.3-prompt-2.json` | `recorded-meta-muse-spark-1.3-prompt-3.json` |
| `provider.adapter_version` | `responses-1/prompt-2` | `responses-1/prompt-3` |
| Request id | `1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2` | same |
| Response sha256 (recomputed from payload, equals recorded) | `9585ce1dac804dc1c2ceebe5c2221bc67c70245abb7099d918d7afeb123f098e` | `7f3371ca503a4f614dda9c4288e346a1822e9fde88546bc058ecba9294706c26` |
| File sha256 prefix | `628dc20dd933c69a` | `f0a1a377b9e2f476` |
| Offline replay | `replay_live_recording.rs`, 13 tests green (2026-11-16) | same run |

## Task 10.9 checklist (Phase 8 exit), prepared 2026-11-16, not marked complete

`PENDING` means the evidence depends on the final CI run. The orchestrator fills it after the reviews and CI. Tasks 10.5, 10.7, 10.9 and 10.15 are not complete.

| Criterion | Evidence | Status |
| --- | --- | --- |
| Full gate set green | Local: fmt, clippy, tests per task notes. CI run: `PENDING` | PENDING |
| `check-tree` passes for `rt-cli` and core crates | `cargo run -p rt-xtask -- check-tree`: ok (26 workspace crates), exit 0, 2026-11-16 at 8af808c. Final CI run: `PENDING` | local pass, CI PENDING |
| `check-graph` lists `rt-ai-responses`, no `rt-ai-openai`, `rt-ai-meta`, `rt-ai-modal` | `check-graph: ok (26 workspace crates)`, exit 0. `cargo metadata --no-deps --offline` members include `rt-ai-responses`, none of the three others. `tools/allowed-edges.v1` lines 23, 24, 30, 48, 52, 53; `deny.toml` lines 34, 36; `ci.yml` line 251; `Cargo.lock` line 356. grep for the three absent names over `tools/allowed-edges.v1`, `deny.toml`, `ci.yml`, `Cargo.toml`, `Cargo.lock`: no match. Final CI run: `PENDING` | local pass, CI PENDING |
| `check-catalogue` passes | `check-catalogue: ok (152 codes)`, exit 0. Final CI run: `PENDING` | local pass, CI PENDING |
| Stub tests (including Meta profile tests) green, no external network | `crates/rt-ai-responses/tests/stub_http.rs`; CI runs 43799230103 (a2ce236) and 43847096409 (438b700) per UD-37 section 8.3. Final CI run: `PENDING`. Task 10.15 is still `[-]` | PARTIAL, PENDING |
| Canary tests pass | `stub_http.rs`, `rt-xtask/tests/live_openai_job.rs`, `rt-tests-ai`. Final CI run: `PENDING` | PENDING |
| User review gate 10.17 answered | `tools/decisions/UD-37.md` section 8 (user decision; blockers 10.5 and 10.15 stay open, commit 4efccee) | Met by user decision |
| Live job runnable but not required | `ci.yml` job `live-openai`, not in `ci-gate` needs; `live_openai_job.rs`. Run 43864502388 job 133289465599: success, steps skipped. Skip, NOT an authenticated request. Credentialed execution DEFERRED and UNVERIFIED by user decision. Final run: `PENDING` | Skip evidence only |
| Recorded script committed with hashes, or recording deferred | Stand-in: the two recordings above (commits d4f1ca4, 31b3fb3), amended by user decision 2026-11-16. Real-document recordings deferred | Met for stand-in by user decision; real documents deferred |
| Independent reviews of 10.5, 10.6, 10.7, 10.15 | `PENDING` | PENDING |

## Approved deferrals (recorded 2026-11-16)

Records user-approved deferrals only. No new deferral is introduced, no requirement text is changed, and no deferred task is marked complete. The decisions are quoted in `tasks.md`, section "Approved deferrals (recorded 2026-11-16)". Decision dates: "session handoff before 2026-11-15" for user decisions 1 and 2, 2026-11-16 for the later 10.5 and 10.8 decisions. No criterion in this file or in `tasks.md` marks any requirement below as satisfied through a deferred task. Where an earlier section states what an implemented task covers (for example Req 29.19 under task 8.7), that statement concerns the implemented task and not the deferred one.

| Task | Status | Reason | Milestone | Affected requirements | Remains unsatisfied |
| --- | --- | --- | --- | --- | --- |
| 9.3 | Deferred | User decision 1: RN-30 and 9.3 deferred; UD-32 keeps separate ComputedRankReference and ExternalRankReference; computed references only; no external ranking dataset support claimed | post-submission hardening | 14.11 to 14.14, 14.15 to 14.25, 32.3, 32.4, 7.23 | The 9.3 criterion; External_Ranking_Value carrying form (RN-30, Req 14.25 applies); blocks 9.6, 9.15, part of 9.16 and 9.20 |
| 9.9 | Deferred | User decision 2 (no per-task reason recorded) | post-submission hardening | 31.8, 31.9, 31.15, 11.5 | Property 44 |
| 9.12 | Deferred | User decision 2 | post-submission hardening | 22.22, 22.12, 22.13, 22.9, 20.25 | Property 5 for application, published, script and manifest inputs |
| 9.13 | Deferred | User decision 2 | post-submission hardening | 31.7, 32.6, 13.12, 13.20 | Property 46 |
| 9.14 | Deferred | User decision 2 | post-submission hardening | 31.13, 31.14, 32.5, 22.1 | Property 47 |
| 9.15 | Deferred | User decision 2 (also follows from 9.3) | post-submission hardening | 14.12, 14.13, 32.3, 32.4 | Property 45 and the cycle negative test |
| 9.17 | Deferred | User decision 2 | post-submission hardening | 31.11, 31.15, 20.6, 27.15 | Two-application isolation and process-independence tests |
| 9.19 | Deferred | User decision 2 | post-submission hardening | 27.1, 27.15, 24.12, 31.10 | Steering updates of design §9.3 items 6 to 8 |
| 10.4 | Superseded | UD-37 made it required; complete `[x]` | not applicable | 23.39, 23.27, 23.4 | Nothing; the deferral no longer applies |
| 10.7 | Superseded | User required it on 2026-11-16; implemented, review and CI pending, stays `[ ]` | not applicable | 33.10, 28.21, 28.11, 33.2 | Not complete until the review and CI runs; the deferral no longer applies |
| 10.5 | Deferred (part) | 2026-11-16: credentialed live execution of the OpenAI job deferred and unverified; the skip-safe job is implemented. Task stays `[-]` | post-submission hardening | 21.14, 35.9 | Any authenticated request from `live-openai` |
| 10.8 | Deferred (part) | 2026-11-16: real-document (Kiroween, ICC) recordings deferred; stand-in recordings accepted for the recorded-script criterion | M4 | 33.8, 33.10, 28.21, 23.27, 35.22 | Recordings over the UD-34 documents (need 11.1) |
| 11.7 | Reduced | User decision 2: one smoke run | M4 (full checks post-submission) | 33.17, 13.18, 25.6, 26.12, 9.13 | Full M1 to M3 acceptance checks, Properties 49 and 50 |
| 11.8 | Deferred | User decision 2 | post-submission hardening | 34.1, 34.7 | Claims lists, `gen-claims`, `scan-claims` |
| 11.12 | Deferred | User decision 2 | post-submission hardening | 34.7, 33.8 | Remaining steering updates (design §9.3 item 10, item 7 sentence) |
| 12.1 | Deferred | User decision 2 | post-submission hardening | 19.1 to 19.4, 19.6, 19.9, 19.11 | No-panic parser properties |
| 12.2 | Deferred | User decision 2 | post-submission hardening | 3.8, 3.15, 3.17, 4.2, 4.3, 4.4, 4.7, 4.8 | Single-fault parse diagnostics property |
| 12.3 | Deferred | User decision 2 | post-submission hardening | 11.4, 8.5, 8.6 | Property 21 |
| 12.4 | Deferred | User decision 2 | post-submission hardening | 8.15, 8.29, 8.30, 8.31 | Empty-aggregation property, numeric fixtures |
| 12.5 | Deferred | User decision 2 | post-submission hardening | 6.7, 6.8, 6.16 to 6.21, 6.23, 12.10, 14.10 | Conflict and precedence property |
| 12.6 | Deferred | User decision 2 | post-submission hardening | 12.5, 12.11, 12.12, 16.23 | Execution-time conflict property |
| 12.7 | Deferred | User decision 2 | post-submission hardening | 6.4, 6.5, 6.6, 14.6, 14.7, 10.9 | Unsupported and incomplete constructs property |
| 12.8 | Deferred | User decision 2 | post-submission hardening | 29.10, 29.11, 29.17, 29.19, 23.20 | Idempotent and concurrent storage property |
| 12.9 | Deferred | User decision 2 | post-submission hardening | 23.35 to 23.38, 16.24, 27.8 | Run_Summary partition property |
| 12.10 | Deferred | User decision 2 | post-submission hardening | 20.7, 20.8, 20.9, 23.17, 23.33, 23.38, 29.14 | Full failure matrix, simulated-timer tests |
| 12.11 | Deferred | User decision 2 | post-submission hardening | 6.1, 6.2, 6.3, 20.3, 20.12, 20.13 | Negative test for every provokable code; 12.12 is blocked by it |
| 12.13 | Deferred | User decision 2 | post-submission hardening | 14.8, 14.9, 6.22, 26.22, 20.1 | Support-boundary consistency test, `docs/security-limits.md` |
| 12.14 | Reduced | User decision 2: one final green CI run, no skipped required jobs | M4 | 21.3, 21.4, 21.9, 21.10, 21.12, 24.9, 20.26 | Job list, allowed-edges coverage, decision status listing |
| 12.15 | Reduced | User decision 2: one final green CI run, no skipped required jobs | M4 | 20.26, 14.8, 34.8 | Release checklist, baseline, boundary documents |

Kept, not deferred: 9.10 and all essential compiler, provenance, approval, AI trust-boundary and credential-safety tests; 10.6 (offline replay, `[-]`, pending CI).

## Task 9.2 evidence (2026-11-17, still `[-]`)

- Requirements 13.12, 13.20, 31.7 and 32.6 (engine part): the engine classifies each ranking difference as `value_differs`, `tie_broken_without_declared_criterion` or `tie_not_in_computed_result`, with published value, computed value, Decision_Trace entry and group members. Tests: `crates/rt-engine/src/tests_published_ranks.rs` (hand-computed cases, a 256-case property against an independent model, a 256-case equal-means-no-difference property). Eight mutants were killed.
- RN-26 amendment approved by the user on 2026-11-17 (text in `docs/requirements-amendments.md`, applied to Req 13.12). Req 31.7 and 32.6 are met by the engine only; the application reports (9.4 onward) and Property 46 (9.13, deferred) remain open. Independent review and CI: `PENDING`.
- Review follow-up (2026-11-17), same Requirements: tests only in `crates/rt-engine/src/tests_published_ranks.rs` close the review's gaps (Indeterminate beside Known in a published tie, Boolean beside the number 1, entry-order independence through `parse_published`); mutants M4, M6 and M10 killed, no production change.


## Submission sequencing (recorded 2026-11-16)

User decision: finish M2 closure and M3, then M4, then write the submission material; keep engineering documents, decision records, reviews and deferrals current throughout; record Kiro usage factually as it happens (`docs/kiro-usage-log.md`); add no MCP, hook or custom agent for appearance only; do not restructure the workspace before the exact Skeleton Crew folder rule is verified. The ordering, the deferred submission items and the open folder-rule question are in `tasks.md` under "Submission sequencing". No requirement, task or acceptance criterion changed. The tree check at this point: tasks 9.1 (aborted run) left no changes in the working tree, so 9.1 is not started.


## Task 9.2 closed (2026-11-17)

Closed by user instruction on two independent reviews (both APPROVED) and the local workspace run (1941 passed). RN-26 was approved and the difference kind naming and catalogue codes were decided by the user on 2026-11-17 (see the next section). Three-platform CI evidence is pending until M3 is pushed and is checked at task 9.20. Non-blocking gaps N3 and O2 are recorded in `docs/reviews/2026-11-17-review-9.2-tests-62b7b83.md`.


## RN-26 approved and catalogue codes RT-EXE-0012 and RT-EXE-0013 (user decisions, 2026-11-17)

- Req 13.12 now contains the RN-26 text. Open points 1 and 2 were settled as implemented; point 3 by decision 2: one code per difference kind. `RT-EXE-0011` (`PUBLISHED_RESULT_DIFFERS`, `value_differs`), `RT-EXE-0012` (`PUBLISHED_TIE_GROUP_SPLIT`, `tie_broken_without_declared_criterion`) and `RT-EXE-0013` (`PUBLISHED_TIE_NOT_COMPUTED`, `tie_not_in_computed_result`) are Warnings with exit code 0, so a published difference never changes the evaluation exit code. The `RT-EXE-0011` meaning text now says value difference only.
- Files: `catalogue/diagnostics.v1.json`, generated `crates/rt-foundation/src/code_generated.rs`, `crates/rt-cli/src/evaluate.rs`, design §3.6.3, §9.4 and the §4.3 catalogue table.
- Tests: `crates/rt-cli/src/tests_published_kinds.rs` (four evaluate-level tests), `crates/rt-foundation/tests/catalogue_rn26.rs` (four tests). `catalogue_ud37.rs` count assertion raised from 152 to 154 (two new codes).


## Tasks 9.1 and 9.4 (2026-11-17)

Implemented and independently reviewed; no blocking finding open (9.1: three reviews, final APPROVED; 9.4: four reviews, final APPROVED). Both stay `[-]` until the user accepts the open items listed in their status notes, and three-platform CI is pending until M3 is pushed (task 9.20). User decisions applied: trailing CSV delimiter, unrequested published rows (design §8.3, `e240d5f`), difference kind as text plus code with one table, Bracket ranking-scope header.


## Tasks 9.1 and 9.4 closed (2026-11-17)

Closed by the user's acceptance after the final follow-ups (`715db8c`, `ecf9550`) and the focused check `docs/reviews/2026-11-17-review-final-9.4-ecf9550.md` (APPROVED). The four accepted 9.4 limitations are recorded as invariants in the 9.4 task note; invariant (1) (positional trace-to-verification binding) must be revisited before any change that reorders or filters entries. Three-platform CI evidence is pending until M3 is pushed (task 9.20).


## UD-38 amendments (2026-11-18)

Cause: the user decision UD-38 (`tools/decisions/UD-38.md`): category Frankenstein, one integrated local RuleTrace Laboratory, runtime executables `ruletrace`, `ruletrace-ai`, `ruletrace-lab`, `prize-check` and `bracket-check` as library Workflow_Modules. Documentation only. Every amended criterion carries the marker "(Amended by UD-38, 2026-11-18.)" in `requirements.md`. The marker is removed below from the recorded old and new text.

New requirements: Requirement 36 (RuleTrace Laboratory, 24 criteria) and Requirement 37 (Laboratory threat model and local security, 23 criteria). New criteria elsewhere: 24.13, 33.18, 33.19, 34.9, 34.10. No criterion was renumbered. Criteria that were not amended keep their text.

Amended and new text, old wording then new wording:

### 1. Introduction, paragraph 3 (exclusion)

Old: RuleTrace never invents a rule and never treats an ambiguous rule as having a definite interpretation. The compiler is the product. This specification excludes any dashboard, chatbot, or hosted service.

New: RuleTrace never invents a rule and never treats an ambiguous rule as having a definite interpretation. The compiler is the product. This specification excludes any dashboard, chatbot, or hosted service, with the single exception of the local RuleTrace_Laboratory (Requirements 36 and 37; UD-38).

### 2. Introduction, Demonstration applications, first sentence

Old: The first version contains two thin demonstration Application_Layers, Prize_Check and Bracket_Check (Requirements 31 and 32), which prove that one Rules_Engine powers two different applications. Each Application_Layer maps its own input format to Competition_Data, calls the shared Rules_Engine through the library interface, and renders a report. Each Application_Layer contains input mapping and report layout only, and no eligibility, scoring, ranking, or tie-breaking logic (Requirement 15 criteria 5 and 6). Prize_Check demonstrates eligibility, weighted scoring, ranking with Tie_Groups, and a published-result consistency check. Bracket_Check demonstrates standings (ranking with ordered tie-breaking criteria), qualification as defined by the Qualification_Predicate of Requirement 14 criterion 11, Tie_Groups, and a published-standings consistency check.

New: The first version contains two thin demonstration Workflow_Modules (also named Application_Layers), Prize_Check and Bracket_Check (Requirements 31 and 32), which prove that one Rules_Engine powers two different workflows and which the RuleTrace_Laboratory presents as the selectable Prize workflow and Tournament standings workflow (Requirement 36 criterion 18). Each Application_Layer maps its own input format to Competition_Data, calls the shared Rules_Engine through the library interface, and renders a report. Each Application_Layer contains input mapping and report layout only, and no eligibility, scoring, ranking, or tie-breaking logic (Requirement 15 criteria 5 and 6). Prize_Check demonstrates eligibility, weighted scoring, ranking with Tie_Groups, and a published-result consistency check. Bracket_Check demonstrates standings (ranking with ordered tie-breaking criteria), qualification as defined by the Qualification_Predicate of Requirement 14 criterion 11, Tie_Groups, and a published-standings consistency check.

### 3. Scope, in scope: workspace and executables

Old: - A Cargo workspace of Rust crates covering the seven pipeline stages, with two RuleTrace executables: the deterministic ruletrace (the CLI), which has four pipeline commands and the review command group, and the separate optional ruletrace-ai (Requirement 27), plus two thin Demonstration_Executables, prize-check and bracket-check (Requirements 27 and 31; UD-33).

New: - A Cargo workspace of Rust crates covering the seven pipeline stages, with three runtime executables: the deterministic ruletrace (the CLI), which has four pipeline commands and the review command group, the separate optional ruletrace-ai (Requirement 27), and the local RuleTrace_Laboratory ruletrace-lab (Requirements 27, 36, and 37; UD-38), plus the library packages prize-check and bracket-check (Requirement 31), which have no binary target.

### 4. Scope, in scope: Application_Layers

Old: - Two thin demonstration Application_Layers, Prize_Check and Bracket_Check, limited to input mapping and report layout over the shared Rules_Engine (Requirements 15, 31, and 32).

New: - Two thin demonstration Workflow_Modules (Application_Layers), Prize_Check and Bracket_Check, limited to input mapping, Question construction, and report presentation over the shared Rules_Engine (Requirements 15, 31, and 32).

### 5. Scope, in scope: new bullet (Laboratory)

Old: (none)

New: - The local RuleTrace_Laboratory ruletrace-lab: one integrated local application with a loopback browser interface in front of ruletrace and ruletrace-ai, with the Prize workflow and the Tournament standings workflow, an offline demo mode, and the local security requirements of Requirement 37 (UD-38).

### 6. Scope, out of scope: full applications

Old: - The full Prize Check and Bracket Check applications, their user interfaces, and their domain-specific workflows beyond the thin Application_Layers of Requirements 31 and 32; and any prize allocation, slot-based qualification, seeding, bracket generation, progression, or multi-stage competition logic in any crate, including the Application_Layers.

New: - The full Prize Check and Bracket Check applications, their user interfaces (other than the single local RuleTrace_Laboratory of Requirements 36 and 37), and their domain-specific workflows beyond the thin Application_Layers of Requirements 31 and 32; and any prize allocation, slot-based qualification, seeding, bracket generation, progression, or multi-stage competition logic in any crate, including the Application_Layers.

### 7. Scope, out of scope: dashboards

Old: - Dashboards, chatbots, web services operated by RuleTrace, databases, and microservices.

New: - Dashboards, chatbots, web services operated by RuleTrace, hosted application services, cloud dashboards, user accounts and remote authentication services, hosted databases, general-purpose workflow services, databases, microservices, and any replacement of the compiler or the Rules_Engine by user-interface logic. The single local RuleTrace_Laboratory (Requirements 36 and 37) is the only user interface in scope.

### 8. Glossary: Prize_Check

Old: - **Prize_Check**: The first-version thin demonstration Application_Layer of Requirement 31, which demonstrates eligibility, weighted scoring, ranking with Tie_Groups, and a published-result consistency check through the shared Rules_Engine, and performs no prize allocation. The full Prize Check application remains a future application.

New: - **Prize_Check**: The first-version thin demonstration Workflow_Module (Application_Layer) of Requirement 31, presented by the RuleTrace_Laboratory as the Prize workflow, which demonstrates eligibility, weighted scoring, ranking with Tie_Groups, and a published-result consistency check through the shared Rules_Engine, and performs no prize allocation. The full Prize Check application remains a future application.

### 9. Glossary: Bracket_Check

Old: - **Bracket_Check**: The first-version thin demonstration Application_Layer of Requirements 31 and 32, which demonstrates standings, Qualification_Predicates, Tie_Groups, and a published-standings consistency check through the shared Rules_Engine, and performs no seeding, bracket generation, progression, or multi-stage logic. The full Bracket Check application remains a future application.

New: - **Bracket_Check**: The first-version thin demonstration Workflow_Module (Application_Layer) of Requirements 31 and 32, presented by the RuleTrace_Laboratory as the Tournament standings workflow, which demonstrates standings, Qualification_Predicates, Tie_Groups, and a published-standings consistency check through the shared Rules_Engine, and performs no seeding, bracket generation, progression, or multi-stage logic. The full Bracket Check application remains a future application.

### 10. Glossary: Application_Layer

Old: - **Application_Layer**: A crate that contains only the input mapping of its own Application_Input_Format to Competition_Data, the construction of Questions, and report layout, and that calls the shared Rules_Engine through the library interface (Requirement 31). Prize_Check and Bracket_Check are the only Application_Layers of the first version.

New: - **Application_Layer**: A Workflow_Module; the two terms are synonyms and Workflow_Module is preferred. A library crate without a binary target that contains only the input mapping of its own Application_Input_Format to Competition_Data, the construction of Questions, and report layout, and that calls the shared Rules_Engine through the library interface (Requirement 31). Prize_Check and Bracket_Check are the only Application_Layers of the first version.

### 11. Glossary: Demonstration_Executable

Old: - **Demonstration_Executable**: A small executable that wraps one Application_Layer for file input and report output at its binary edge. The Demonstration_Executables are prize-check and bracket-check (UD-33).

New: - **Demonstration_Executable**: A small executable that wraps one Application_Layer for file input and report output at its binary edge. Superseded by UD-38: no executable of the first version is a Demonstration_Executable. The packages prize-check and bracket-check are library-only Workflow_Modules, and the term remains defined only so that the historical text of UD-33 is readable.

### 12. Glossary: new entry

Old: (none)

New: - **Workflow_Module**: A library crate that contains only the input mapping of an Application_Input_Format to Competition_Data, the construction of Questions, and the presentation of a report, with no binary target and no business logic (Requirement 31). The Workflow_Modules are prize-check and bracket-check. The RuleTrace_Laboratory invokes them.

### 13. Glossary: new entry

Old: (none)

New: - **RuleTrace_Laboratory** (also the Laboratory): The one integrated local application of Requirements 36 and 37, served by the executable ruletrace-lab on the loopback interface. It orchestrates ruletrace and ruletrace-ai as subprocesses and displays their results. It holds no AI credential, writes no Approval_Ledger record, and makes no decision, approval, or computation of its own (UD-38).

### 14. Glossary: new entry

Old: (none)

New: - **Laboratory_Session**: The period between the exchange of a Launch_Secret for a session cookie and the invalidation of that cookie, with its own temporary directory and its own CSRF token (Requirement 37).

### 15. Glossary: new entry

Old: (none)

New: - **Launch_Secret**: A random value of at least 128 bits that ruletrace-lab generates at each start and gives to the user only through the terminal, and that the user exchanges once for a session cookie (Requirement 37 criteria 5 and 6).

### 16. Glossary: new entry

Old: (none)

New: - **Hand_Off_Artifact**: A file that one subprocess of the RuleTrace_Laboratory produces and the next subprocess consumes, whose SHA-256 hash the Laboratory records and verifies before use (Requirement 36 criterion 9). A valid hash does not make the content trusted.

### 17. Glossary: new entry

Old: (none)

New: - **Runtime_Executable**: An executable that the RuleTrace delivers to a user: ruletrace, ruletrace-ai, and ruletrace-lab. A development or test utility that the workspace builds (for example rt-xtask) is not a Runtime_Executable (Requirement 27 criterion 1).

### 18. 7.23

Old: 23. THE RuleTrace SHALL list in the Diagnostic_Catalogue the code RANK_REFERENCE_CYCLE (Requirement 14 criterion 12, WHERE the option Rank_Reference of UD-32 is selected) and the Diagnostic codes that Requirement 31 criterion 9 names (APP_INPUT_MALFORMED, APP_INPUT_FIELD_MISSING, APP_INPUT_DUPLICATE_PARTICIPANT, APP_INPUT_VALUE_INVALID, APP_TARGET_NOT_DECLARED, APP_TARGET_KIND_MISMATCH), each with exactly one catalogue identifier, a Severity, a meaning, an exit code, and an executable (the Demonstration_Executable that returns it), and SHALL bind each of these mnemonics to its identifier under criterion 16. The design assigns the family of the application codes (RT-APP is proposed). The checks of Requirement 33 and 34 are test failures and not Diagnostics, and need no catalogue code.

New: 23. THE RuleTrace SHALL list in the Diagnostic_Catalogue the code RANK_REFERENCE_CYCLE (Requirement 14 criterion 12, WHERE the option Rank_Reference of UD-32 is selected) and the Diagnostic codes that Requirement 31 criterion 9 names (APP_INPUT_MALFORMED, APP_INPUT_FIELD_MISSING, APP_INPUT_DUPLICATE_PARTICIPANT, APP_INPUT_VALUE_INVALID, APP_TARGET_NOT_DECLARED, APP_TARGET_KIND_MISMATCH), each with exactly one catalogue identifier, a Severity, a meaning, an exit code, and an executable (ruletrace-lab, which reports the Diagnostic that a Workflow_Module returns), and SHALL bind each of these mnemonics to its identifier under criterion 16. The design assigns the family of the application codes (RT-APP is proposed). The checks of Requirement 33 and 34 are test failures and not Diagnostics, and need no catalogue code.

### 19. 24.12

Old: 12. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of each Application_Layer crate and Demonstration_Executable, which are limited to the Rules_Engine crate, the crates for the Compiled_Rulebook and Competition_Data types and their parsers, the Explainer crate, the Source_Model crate, and the shared Diagnostic, Location, and Exact_Number crates, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN an Application_Layer or Demonstration_Executable depends, directly or transitively, on the ruletrace binary crate, on the review library crate, on a crate that contains the Approval_Ledger append function, on the Validator, Compiler, Ingestor, or Extractor crate, on an Auxiliary_Crate, or on a network client library, or WHEN a Core_Crate or Auxiliary_Crate depends on an Application_Layer crate or a Demonstration_Executable.

New: 12. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of each Application_Layer crate, which are limited to the Rules_Engine crate, the crates for the Compiled_Rulebook and Competition_Data types and their parsers, the Explainer crate, the Source_Model crate, and the shared Diagnostic, Location, and Exact_Number crates, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN an Application_Layer depends, directly or transitively, on the ruletrace binary crate, on the review library crate, on a crate that contains the Approval_Ledger append function, on the Validator, Compiler, Ingestor, or Extractor crate, on an Auxiliary_Crate, or on a network client library, or WHEN a Core_Crate or Auxiliary_Crate depends on an Application_Layer crate.

### 20. 24.13 (new)

Old: (none)

New: 13. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of the ruletrace-lab crate, which are limited to the two Application_Layer crates, the shared Diagnostic, Location, and Exact_Number crates, and one HTTP server library with a written justification, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN ruletrace-lab depends, directly or transitively, on an AI_Crate, an AI provider library, the review library crate, a crate that contains the Approval_Ledger append function, the Validator, Compiler, Ingestor, or Extractor crate, or WHEN a Core_Crate, an Auxiliary_Crate, an Application_Layer crate, ruletrace, or ruletrace-ai depends on ruletrace-lab or on its HTTP server library, and SHALL fail the source scan of Requirement 36 criterion 5 WHEN a source file of ruletrace-lab calls a Rules_Engine evaluation function (UD-38).

### 21. 27 user story

Old: **User Story:** As a maintainer, I want exactly two RuleTrace executables with separated responsibilities, so that no network or AI code can reach the deterministic workflow or the approval ledger.

New: **User Story:** As a maintainer, I want exactly three runtime executables with separated responsibilities, so that no network or AI code can reach the deterministic workflow or the approval ledger, and so that the browser interface can reach neither the AI credential nor the approval ledger directly.

### 22. 27.1

Old: 1. THE RuleTrace SHALL deliver exactly two RuleTrace executables, ruletrace and ruletrace-ai (the AI_Tool), and, in addition, exactly two Demonstration_Executables, prize-check and bracket-check (Requirement 31 criterion 3), and no other executable. The responsibilities of the two RuleTrace executables are those of the following table. This criterion supersedes the earlier statement of exactly two executables and no third (UD-33).

New: 1. THE RuleTrace SHALL deliver exactly three Runtime_Executables, ruletrace, ruletrace-ai (the AI_Tool), and ruletrace-lab (the Laboratory executable of Requirements 36 and 37), and no other Runtime_Executable, and SHALL build prize-check and bracket-check as library packages without a binary target (Requirement 31 criterion 3). A development or test utility that the workspace builds and does not deliver, for example rt-xtask, is not a Runtime_Executable and SHALL appear in a documented allow-list of such utilities. The responsibilities of ruletrace and ruletrace-ai are those of the following table, and those of ruletrace-lab are those of Requirements 36 and 37. This criterion supersedes the earlier statements of exactly two executables (UD-22) and of exactly two Demonstration_Executables (UD-33).

### 23. 27.15

Old: 15. THE RuleTrace SHALL include a test that the set of executables that the workspace builds equals exactly ruletrace, ruletrace-ai, prize-check, and bracket-check, and that fails with the name of any other executable, and criteria 5 to 7 and 9 to 13 SHALL apply to ruletrace and ruletrace-ai unchanged, so that neither Demonstration_Executable is a command of ruletrace, is started by ruletrace or ruletrace-ai, or appears in the dependency tree of ruletrace or ruletrace-ai.

New: 15. THE RuleTrace SHALL include a test that the set of executables that the workspace builds equals exactly ruletrace, ruletrace-ai, and ruletrace-lab, together with the development and test utilities of the allow-list of criterion 1, and that fails with the name of any other executable (including a binary target of prize-check or bracket-check), and criteria 5 to 7 and 9 to 13 SHALL apply to ruletrace and ruletrace-ai unchanged, so that ruletrace-lab is not a command of ruletrace, is not started by ruletrace or ruletrace-ai, and does not appear in the dependency tree of ruletrace or ruletrace-ai.

### 24. 31.3

Old: 3. THE RuleTrace SHALL deliver each Application_Layer with one Demonstration_Executable of the same name as its crate (prize-check and bracket-check), which performs the reading of its named input files and the writing of its named report files at its binary edge and calls the library of its Application_Layer for everything else (UD-33).

New: 3. THE RuleTrace SHALL deliver each Application_Layer as a library crate named prize-check or bracket-check with no binary target, and THE RuleTrace_Laboratory (ruletrace-lab) SHALL perform the reading of its named input files and the writing of its named report files within the roots of Requirement 37 criterion 13 and SHALL call the library of the Application_Layer for input mapping, Question construction, and report presentation (UD-33, UD-38).

### 25. 31.10

Old: 10. THE RuleTrace SHALL include an automated source scan of the Application_Layer crates and the Demonstration_Executables that fails with the crate, file, and line WHEN a source file contains a call or an operator that adds, subtracts, multiplies, divides, rounds, compares, or aggregates Exact_Number values, a sorting, ordering, minimum, maximum, or ranking function applied to participants, scores, or Outcomes, a tie-breaking rule, a literal weight, threshold, or rounding mode, or a field name of a specific competition, and SHALL include a negative test fixture for each of these violations that makes the scan fail with the expected file and line, so that no ranking, scoring, eligibility, or tie-breaking logic exists in an Application_Layer (Requirement 15 criterion 6).

New: 10. THE RuleTrace SHALL include an automated source scan of the Application_Layer crates and of the ruletrace-lab crate that fails with the crate, file, and line WHEN a source file contains a call or an operator that adds, subtracts, multiplies, divides, rounds, compares, or aggregates Exact_Number values, a sorting, ordering, minimum, maximum, or ranking function applied to participants, scores, or Outcomes, a tie-breaking rule, a literal weight, threshold, or rounding mode, or a field name of a specific competition, and SHALL include a negative test fixture for each of these violations that makes the scan fail with the expected file and line, so that no ranking, scoring, eligibility, or tie-breaking logic exists in an Application_Layer (Requirement 15 criterion 6).

### 26. 31.11

Old: 11. THE Application_Layer library crates SHALL perform no file, network, clock, random, or environment access and start no subprocess, THE Demonstration_Executables SHALL open only the input files and the report files that their command-line arguments name, SHALL open no network connection, SHALL read no environment variable, no AI_Configuration, and no credential, SHALL start no subprocess, and SHALL write byte-identical standard output, standard error, report files, and exit code whether or not ruletrace, ruletrace-ai, an AI_Configuration file, or a credential environment variable is present, as verified by the scan of criterion 10, the dependency check of Requirement 24 criterion 12, and a test under at least two values of the clock, the working directory, and the locale.

New: 11. THE Application_Layer library crates SHALL perform no file, network, clock, random, or environment access and start no subprocess, and SHALL return byte-identical reports and the same result classification of criterion 12 whether or not ruletrace, ruletrace-ai, an AI_Configuration file, or a credential environment variable is present, as verified by the scan of criterion 10, the dependency check of Requirement 24 criterion 12, and a test under at least two values of the clock, the working directory, and the locale.

### 27. 31.12

Old: 12. THE Demonstration_Executables SHALL exit with exactly these codes: 0 WHEN the report is complete and no Outcome is Indeterminate, 4 WHEN at least one Outcome is Indeterminate, 2 for invalid input (criterion 9, a contradictory diagnostic or lifecycle state under Requirement 22, a missing or unreadable input file, an unknown argument, a Source_Model that is missing for a document or whose hash differs, as Requirement 16 criterion 8 defines it), and 70 for an internal error, and each Demonstration_Executable SHALL return the same exit code as `ruletrace evaluate` returns for the same Compiled_Rulebook, Competition_Data, Questions, Published_Result, and Source_Models, and SHALL write no partial report as if the run had succeeded (Requirement 16 criteria 9 and 12).

New: 12. THE Application_Layer SHALL return a result classification that ruletrace-lab maps to exactly these exit codes: 0 WHEN the report is complete and no Outcome is Indeterminate, 4 WHEN at least one Outcome is Indeterminate, 2 for invalid input (criterion 9, a contradictory diagnostic or lifecycle state under Requirement 22, a missing or unreadable input file, an unknown argument, a Source_Model that is missing for a document or whose hash differs, as Requirement 16 criterion 8 defines it), and 70 for an internal error, and each result classification SHALL equal the exit code that `ruletrace evaluate` returns for the same Compiled_Rulebook, Competition_Data, Questions, Published_Result, and Source_Models, and THE Application_Layer SHALL write no partial report as if the run had succeeded (Requirement 16 criteria 9 and 12).

### 28. 32.1

Old: 1. THE Bracket_Check Application_Layer SHALL report Standings for each requested ranking Target, listing each participant with its Rank_Position or its Tie_Group, listing each Tie_Group as one entry with its members, listing each Indeterminate position with the missing input, and listing for each ranking comparison the tie-breaking criteria applied in order and the compared values that the Decision_Trace records (Requirement 12 criterion 7).

New: 1. THE Bracket_Check Application_Layer SHALL report Standings for each requested ranking Target, listing each participant with its Rank_Position or its Tie_Group, listing each Tie_Group as one entry with its members, listing each Indeterminate position with the missing input, and listing for each ranking comparison the tie-breaking criteria applied in order and the compared values that the Decision_Trace records (Requirement 12 criterion 7), and THE RuleTrace_Laboratory SHALL name this workflow Tournament standings and SHALL describe it as standings and tie groups only, with no bracket, seeding, or progression (Requirement 36 criterion 18).

### 29. 33.7

Old: 7. THE RuleTrace SHALL document the demonstration flow as these ordered steps, each with its command, its inputs, and its expected exit code: (1) ingest the demonstration Source_Documents with the --candidates option; (2) run ruletrace-ai propose on the Candidate_Clauses; (3) list and review the Proposals with ruletrace review list, approve, and reject using a caller-supplied Reviewer and time; (4) validate the Rulebook with the Approval_Ledger; (5) compile; (6) evaluate on the demonstration Competition_Data with the Source_Models and a Published_Result; (7) render the Decision_Traces and the Proof_Report through the Explainer; (8) run Prize_Check and Bracket_Check on the same Compiled_Rulebook or on the Compiled_Rulebook of each demonstration document (the Demo_Runbook states which), and compare their Outcomes and Decision_Traces with the output of step 6 under Requirement 31 criterion 5.

New: 7. THE RuleTrace SHALL document the demonstration flow as these ordered steps, each with its command, its inputs, and its expected exit code: (1) ingest the demonstration Source_Documents with the --candidates option; (2) run ruletrace-ai propose on the Candidate_Clauses; (3) list and review the Proposals with ruletrace review list, approve, and reject using a caller-supplied Reviewer and time; (4) validate the Rulebook with the Approval_Ledger; (5) compile; (6) evaluate on the demonstration Competition_Data with the Source_Models and a Published_Result; (7) render the Decision_Traces and the Proof_Report through the Explainer; (8) run the Prize workflow and the Tournament standings workflow through the RuleTrace_Laboratory (Requirement 36 criterion 18; the machine-readable block gives the Laboratory command or test-harness command) on the same Compiled_Rulebook or on the Compiled_Rulebook of each demonstration document (the Demo_Runbook states which), and compare their Outcomes and Decision_Traces with the output of step 6 under Requirement 31 criterion 5.

### 30. 34.1

Old: 1. THE RuleTrace SHALL state in every demonstration document, in the Demo_Runbook, in the user-facing documentation, and in the header of the Proof_Report and of each application report only capabilities that lie within the First_Version_Support_Boundary, and SHALL NOT claim support for arbitrary or universal rulebooks, PDF ingestion, prize allocation, slot-based qualification, seeding, bracket generation, tournament progression, or multi-stage competitions, and SHALL NOT describe the Prize_Check and Bracket_Check Application_Layers as the full Prize Check and Bracket Check applications.

New: 1. THE RuleTrace SHALL state in every demonstration document, in the Demo_Runbook, in the user-facing documentation, and in the header of the Proof_Report and of each application report only capabilities that lie within the First_Version_Support_Boundary, and SHALL NOT claim support for arbitrary or universal rulebooks, PDF ingestion, prize allocation, slot-based qualification, seeding, bracket generation, tournament progression, or multi-stage competitions, and SHALL NOT describe the Prize_Check and Bracket_Check Workflow_Modules as the full Prize Check and Bracket Check applications, and SHALL NOT describe the RuleTrace_Laboratory as the source of any decision or approval.

### 31. Requirement 33 new criterion

Old: (none)

New: 18. THE RuleTrace SHALL run the demonstration of criteria 7 and 9 through the RuleTrace_Laboratory in demo mode (Requirement 36 criterion 20) with the Recorded_Provider_Script, so that every artifact that the Laboratory writes equals the golden file of criterion 9, and THE Demo_Runbook SHALL state the commands that build and start the Laboratory, the steps that a user performs in the browser, and which Ledger_Records are replayed recorded decisions.

### 32. Requirement 33 new criterion

Old: (none)

New: 19. THE demonstration acceptance test of criterion 15 SHALL include a run that drives the HTTP interface of ruletrace-lab (Requirement 36 criterion 24) and SHALL compare the artifacts of that run with the same golden files.

### 33. Requirement 34 new criterion

Old: (none)

New: 9. THE RuleTrace SHALL state in the Demo_Runbook, in the user-facing documentation, and on the first screen of the RuleTrace_Laboratory that the Laboratory decides nothing and approves nothing by itself, that every Outcome is the result of `ruletrace evaluate`, that every approval is a recorded human act through `ruletrace review`, and that every Proposal shown is untrusted AI output.

### 34. Requirement 34 new criterion

Old: (none)

New: 10. THE RuleTrace SHALL state in the Demo_Runbook and in the user-facing documentation that the stage names Brain, Spine, Heart, Lightning, Ghost, and Autopsy name the components of Requirement 36 criterion 17, that the Prize workflow reports eligibility and ranking as separate Outcomes of separate Rules and that eligibility does not filter rankings, that Tournament standings are not brackets, seeding, or progression, and that a demo-mode run replays recorded provider output and recorded Reviewer decisions, and THE required-statement list and the prohibited-phrase list of criterion 7 SHALL include these statements and the phrases "AI approved", "AI decided", "automatically approved", and "bracket generator".

### 35. Register UD-22

Old: - **UD-22 Packaging** (Resolved-by-user 2026-11-14; amended by UD-33; affects 5.12, 17.1, 18.13, 24.5, 27.x): Two RuleTrace executables, ruletrace and ruletrace-ai, plus (UD-33, Recommended-pending) two Demonstration_Executables. Human approval is an explicit operation in the main workflow through the Review_Group, which supersedes the earlier separate Review_Tool.

New: - **UD-22 Packaging** (Resolved-by-user 2026-11-14; amended by UD-33; affects 5.12, 17.1, 18.13, 24.5, 27.x): Two RuleTrace executables, ruletrace and ruletrace-ai, plus (UD-33, Recommended-pending) two Demonstration_Executables. Superseded by UD-38: the runtime executable set is ruletrace, ruletrace-ai, and ruletrace-lab. Human approval is an explicit operation in the main workflow through the Review_Group, which supersedes the earlier separate Review_Tool.

### 36. Register UD-33 entry

Old: - **UD-33 Packaging of the Application_Layers** (Recommended-pending; decided by the user; affects 27.1, 27.15, 31.3): Compared options: (A) a third RuleTrace executable with prize-check and bracket-check subcommands; (B) two separate small Demonstration_Executables, prize-check and bracket-check, each a thin binary over its library crate; (C) library crates only, with Cargo example binaries and tests as the only entry points. Option A couples both applications into one binary and one dependency tree, which weakens the demonstration that two different applications use one engine, and adds a third entry point to the RuleTrace executables. Option C leaves 27.1 unchanged but offers no ordinary command for a demonstration and for a reviewer. Option B keeps the applications independent, keeps ruletrace and ruletrace-ai unchanged (their dependency trees contain no Application_Layer, 24.12, 27.15), and costs one amendment of 27.1. Recommendation: Option B, applied in this revision: 27.1 now says exactly two RuleTrace executables plus exactly two Demonstration_Executables. If the user chooses Option C, 27.1 reverts to its earlier wording and 31.3 and 27.15 change accordingly.

New: - **UD-33 Packaging of the Application_Layers** (Recommended-pending; decided by the user; affects 27.1, 27.15, 31.3): Compared options: (A) a third RuleTrace executable with prize-check and bracket-check subcommands; (B) two separate small Demonstration_Executables, prize-check and bracket-check, each a thin binary over its library crate; (C) library crates only, with Cargo example binaries and tests as the only entry points. Option A couples both applications into one binary and one dependency tree, which weakens the demonstration that two different applications use one engine, and adds a third entry point to the RuleTrace executables. Option C leaves 27.1 unchanged but offers no ordinary command for a demonstration and for a reviewer. Option B keeps the applications independent, keeps ruletrace and ruletrace-ai unchanged (their dependency trees contain no Application_Layer, 24.12, 27.15), and costs one amendment of 27.1. Recommendation: Option B, applied in this revision: 27.1 now says exactly two RuleTrace executables plus exactly two Demonstration_Executables. If the user chooses Option C, 27.1 reverts to its earlier wording and 31.3 and 27.15 change accordingly. Superseded by UD-38 (2026-11-18): option B is replaced by library Workflow_Modules and the executable ruletrace-lab; the four binding constraints stay for the libraries.

### 37. Register UD-38 (new)

Old: (none)

New: - **UD-38 RuleTrace Laboratory, category Frankenstein** (Confirmed by the user 2026-11-18, see `tools/decisions/UD-38.md`; affects the Introduction, Scope, Glossary, 7.23, 24.12, 24.13, 27, 31.3, 31.10 to 31.12, 32.1, 33.7, 33.18, 33.19, 34.1, 34.9, 34.10, 36, 37): One integrated local application served by ruletrace-lab; runtime executables ruletrace, ruletrace-ai, ruletrace-lab; prize-check and bracket-check as library Workflow_Modules; process separation, credential isolation, loopback-only local web interface, and metaphors that name real behaviour. The credential mechanism, the session mechanism, and the HTTP server library are decided by the Laboratory threat model (task L1), not by this specification.

### Requirements 36 and 37 detailed (2026-11-18)

The acceptance criteria of Requirements 36 and 37 were refined by a requirement-detailing step. Headings and user stories are unchanged, and criterion numbers are stable.

Criteria changed in Requirement 36: 2, 4, 6, 7, 8, 9, 13, 14, 18, 21.

Criteria changed in Requirement 37: 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 18, 21, 23, and new criterion 24 (single current session and single session cookie).

Settled points:

- Requirement 36 criterion 4 keeps "and the Rulebook", because Requirement 27 criterion 12 already lets `ruletrace review approve` write the Rulebook during adoption.
- Requirement 36 criterion 14 counts `ruletrace evaluate` exit code 4 (Indeterminate) as an executed evaluation.

OPEN POINTS for the user:

- (a) Requirement 37 criterion 6 now limits the Laboratory to one Laboratory_Session at a time. This is an addition and must be confirmed against UD-38 open item 11 (two browser tabs).
- (b) Requirement 37 criterion 11 forbids inline styles, which constrains the front end (no style attributes).
- (c) Requirement 37 criterion 3 refuses the Host value `localhost` (only `127.0.0.1:<port>` or `[::1]:<port>`), so the user opens the literal address.
- (d) Requirement 37 criterion 21 leaves open for L1 whether the credential mechanism changes Requirement 28.

### Requirements checkpoint clarifications (2026-11-18)

The user reviewed Requirements 36 and 37 and chose "Continue to the design phase". The requirements phase was not restarted. Decisions on the open points above:

| Point | Decision | Effect on requirements.md |
| --- | --- | --- |
| (a) 37.6 single session | Approved: at most one active session. Several tabs with the same cookie are the same session and are allowed. No second independent session. State-changing requests are serialized or guarded against stale revisions. Approval always rechecks the exact revision and hash, whichever tab sent it. Tests: two tabs, concurrent state changes, stale hashes, a second session attempt | 37.6 clarified; new 37.25 |
| (b) 37.11 no inline styles | Approved. Locally served style sheets, restrictive CSP, no remote assets, no `unsafe-inline` | none |
| (c) 37.3 Host | Approved: only the exact Host of the bound address and port (`127.0.0.1:<port>`, `[::1]:<port>`); `localhost`, wildcard, non-loopback and non-matching values refused; binding only to loopback | none (37.1, 37.3, 37.4 already state it) |
| (d) 37.21 credential | **Not closed.** The design and the L1 threat model must reconcile "the Laboratory must not inherit, read, expose, persist or log the credential" with "the Laboratory orchestrates `ruletrace-ai` as a subprocess", examine an OS credential store and a credential broker, and show five properties (Laboratory cannot read; AI process can obtain; not in the approval process; not recoverable from logs, artifacts or the browser session; works on the supported platforms). Process separation alone is not proof | none; design §11.10 and the threat model §6 |
| 34.10 honest claims | **Kept open** pending a source-backed review that quotes the criterion and compares each statement with the engine and its tests, with attention to evaluate exit 4 (executed with Indeterminate Outcomes). No rewrite on assumptions | none; design §11.14, RN-31 and RN-34 to RN-38 |
| Limits | Approved: 30-minute idle timeout, 1 MiB control request, 16 MiB combined subprocess output, 300 s per subprocess. Added: a separate explicit upload maximum, chosen after checking the planned snapshots and fixtures; no unbounded bodies; output overflow terminates with a Diagnostic and no silent truncation; a timed-out subprocess is terminated, reaped and cleaned up | 37.14 clarified; new 37.26; 37.23 now covers 24 to 26 |

Old and new text of the changed criteria:

**37.6.** Old (last sentence): "THE ruletrace-lab executable SHALL hold at most one Laboratory_Session at a time." New: "THE ruletrace-lab executable SHALL hold at most one Laboratory_Session at a time. Several browser tabs that carry the same valid session cookie belong to that one Laboratory_Session and SHALL be served, and THE ruletrace-lab executable SHALL create no second, independent Laboratory_Session while one exists. (Clarified at the requirements checkpoint, 2026-11-18.)"

**37.14.** Old: ends with "(a value with a NUL byte is refused before any subprocess starts)." New: the same text, followed by: "WHEN a subprocess exceeds the output limit or the run-time limit, THE ruletrace-lab executable SHALL terminate it, SHALL wait for its exit so that no child process remains, SHALL display a Diagnostic that names the limit and the workflow step, SHALL display none of the captured output as if it were complete (no silent truncation), SHALL use no output file of that step, and SHALL leave no temporary file of that step in the session directory after the session ends. The 1 MiB limit applies to ordinary control requests; file uploads have the separate limits of criterion 26. (Clarified at the requirements checkpoint, 2026-11-18.)"

**37.23.** Old: "tests of criteria 1 to 22 and 24". New: "tests of criteria 1 to 22 and 24 to 26".

**37.25** (new) and **37.26** (new): see requirements.md. Old: (none).

Note by the design author, not part of the user's decision: the check of the planned snapshots and fixtures found no snapshot and no CSV fixture yet. The upload values themselves are in design §11.9 with status Recommended-pending (LT-8), because the real document snapshots of task 11.1 do not exist yet.

### UD-39 amendments (2026-11-18)

The user's final design decisions are recorded in `tools/decisions/UD-39.md`. Each changed criterion carries the marker "(Amended by UD-39, 2026-11-18.)" or "(Added by UD-39, 2026-11-18.)". Old and new text:

#### 7.16

Old: 16. THE RuleTrace SHALL give every Diagnostic code a catalogue identifier of the form RT-family-number with four digits (families RT-DGN, RT-PRV, RT-APR, and RT-AI, and the families that the design assigns to the Diagnostics of Requirements 1 to 14, 19, and 21) and a mnemonic name, SHALL treat the identifier as the Diagnostic code that tests assert and that the CLI renders, and SHALL bind every mnemonic used in this document to exactly one identifier of the table in criterion 17.

New: 16. THE RuleTrace SHALL give every Diagnostic code a catalogue identifier of the form RT-family-number with four digits (families RT-DGN, RT-PRV, RT-APR, RT-AI, and RT-LAB for the Diagnostics of Requirements 36 and 37, and the families that the design assigns to the Diagnostics of Requirements 1 to 14, 19, and 21) and a mnemonic name, SHALL treat the identifier as the Diagnostic code that tests assert and that the CLI renders, and SHALL bind every mnemonic used in this document to exactly one identifier of the table in criterion 17. (Amended by UD-39, 2026-11-18.)

#### 24.13

Old: 13. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of the ruletrace-lab crate, which are limited to the two Application_Layer crates, the shared Diagnostic, Location, and Exact_Number crates, and one HTTP server library with a written justification, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN ruletrace-lab depends, directly or transitively, on an AI_Crate, an AI provider library, the review library crate, a crate that contains the Approval_Ledger append function, the Validator, Compiler, Ingestor, or Extractor crate, or WHEN a Core_Crate, an Auxiliary_Crate, an Application_Layer crate, ruletrace, or ruletrace-ai depends on ruletrace-lab or on its HTTP server library, and SHALL fail the source scan of Requirement 36 criterion 5 WHEN a source file of ruletrace-lab calls a Rules_Engine evaluation function (UD-38). (Amended by UD-38, 2026-11-18.)

New: 13. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the direct edges of the ruletrace-lab crate, which are limited to exactly these crates, each for the reason that the design records for it: the two Application_Layer crates (input mapping, Question construction, and report presentation); the shared Diagnostic and Location crate and the Exact_Number crate (types); the SHA-256 crate (Hand_Off_Artifact hashes); the strict data-format parser crate (reading the files that ruletrace writes); the Competition_Data crate (canonical serialization of mapped inputs); the Compiled_Rulebook types crate (the input schema); the Outcome and Decision_Trace types crate (types of the parsed results); the Explainer crate (report rendering and the results-file parser); and the Source_Model crate (display of source text); and no HTTP server library, because the server uses the standard library (UD-39). A further edge needs a written justification per edge and an amendment of this criterion. THE RuleTrace SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN ruletrace-lab has a direct edge outside this list, WHEN ruletrace-lab depends, directly or transitively, on an AI_Crate, an AI provider library, a network client library, the review library crate, a crate that contains the Approval_Ledger append function, the Validator, Compiler, Ingestor, or Extractor crate, the ruletrace binary crate, or a development or test utility, or WHEN a Core_Crate, an Auxiliary_Crate, an Application_Layer crate, ruletrace, or ruletrace-ai depends on ruletrace-lab, and SHALL fail the source scan of Requirement 36 criterion 5 WHEN a source file of ruletrace-lab calls a Rules_Engine evaluation function or a function of an Application_Layer that calls one, so that every displayed Outcome originates from `ruletrace evaluate`. (Amended by UD-39, 2026-11-18.)

#### 34.9

Old: 9. THE RuleTrace SHALL state in the Demo_Runbook, in the user-facing documentation, and on the first screen of the RuleTrace_Laboratory that the Laboratory decides nothing and approves nothing by itself, that every Outcome is the result of `ruletrace evaluate`, that every approval is a recorded human act through `ruletrace review`, and that every Proposal shown is untrusted AI output. (Amended by UD-38, 2026-11-18.)

New: 9. THE RuleTrace SHALL state in the Demo_Runbook, in the user-facing documentation, and on the first screen of the RuleTrace_Laboratory that the Laboratory decides nothing and approves nothing by itself, that every Outcome is the result of `ruletrace evaluate`, that every approval is a recorded human act through `ruletrace review` under a Reviewer identity that RuleTrace records and does not authenticate (R-8), and that every unapproved Proposal or Rule_Revision is non-authoritative and is shown with its origin as AI-generated (labelled untrusted), manually authored, or an authored offline demonstration entry. (Amended by UD-38, 2026-11-18.) (Amended by UD-39, 2026-11-18.)

#### 34.10

Old: 10. THE RuleTrace SHALL state in the Demo_Runbook and in the user-facing documentation that the stage names Brain, Spine, Heart, Lightning, Ghost, and Autopsy name the components of Requirement 36 criterion 17, that the Prize workflow reports eligibility and ranking as separate Outcomes of separate Rules and that eligibility does not filter rankings, that Tournament standings are not brackets, seeding, or progression, and that a demo-mode run replays recorded provider output and recorded Reviewer decisions, and THE required-statement list and the prohibited-phrase list of criterion 7 SHALL include these statements and the phrases "AI approved", "AI decided", "automatically approved", and "bracket generator". (Amended by UD-38, 2026-11-18.)

New: 10. THE RuleTrace SHALL state in the Demo_Runbook and in the user-facing documentation that the stage names Brain, Spine, Heart, Lightning, Ghost, and Autopsy name the components of Requirement 36 criterion 17, that the Prize workflow reports eligibility and ranking as separate Outcomes of separate Rules and that eligibility does not filter rankings, that Tournament standings are not brackets, seeding, or progression, that a demo-mode run replays hand-authored scripted responses and not recorded provider output, that the Ledger_Records of the demonstration corpus are decisions that the demonstration Reviewer recorded through `ruletrace review` and not new decisions, and that live provider proposals are generated outside the Laboratory by the user's own `ruletrace-ai` run and imported, and THE required-statement list and the prohibited-phrase list of criterion 7 SHALL include these statements and the phrases "AI approved", "AI decided", "automatically approved", and "bracket generator". (Amended by UD-38, 2026-11-18.) (Amended by UD-39, 2026-11-18.)

#### 36.2

Old: 2. THE RuleTrace_Laboratory SHALL perform every authoritative operation (ingestion, candidate extraction, proposal generation, validation, compilation, evaluation, review listing, review approval, review rejection, and provenance verification) only by starting a ruletrace or ruletrace-ai subprocess, and SHALL perform none of these operations in its own process, as verified by a test that replaces each of the two executables with a recording stand-in and asserts that each such operation requested through the interface reaches exactly one stand-in invocation and no other invocation.

New: 2. THE RuleTrace_Laboratory SHALL perform every authoritative operation that it offers (ingestion, candidate extraction, offline proposal generation in demo mode, validation, compilation, evaluation, review listing, review approval, review rejection, and provenance verification) only by starting a ruletrace or ruletrace-ai subprocess, and SHALL perform none of these operations in its own process, as verified by a test that replaces each of the two executables with a recording stand-in and asserts that each such operation requested through the interface reaches exactly one stand-in invocation and no other invocation. In this version proposal generation by a live AI_Provider is external to the RuleTrace_Laboratory: the user runs ruletrace-ai in a terminal of their own with the credential mechanism of Requirement 28, which is unchanged, and THE RuleTrace_Laboratory SHALL start ruletrace-ai only with an AI_Configuration of the provider kind mock-replay that the demonstration workspace contains or with the --dry-run option, SHALL read and hold no other AI_Configuration, and SHALL integrate the import of the resulting proposals (criterion 25) and every later step (review, approval, validation, compilation, evaluation, and proof). (Amended by UD-39, 2026-11-18.)

#### 36.4

Old: 4. THE RuleTrace_Laboratory SHALL write neither the Approval_Ledger nor the Rulebook from its own process, and SHALL change the bytes of either file only through a ruletrace review approve or ruletrace review reject subprocess, as verified by a test that hashes both files before and after every Laboratory operation and asserts that a change occurs only after an approval or rejection request that passed the checks of criterion 6 and Requirement 37 criterion 19.

New: 4. THE RuleTrace_Laboratory SHALL operate on the Approval_Ledger, the Rulebook, and the Proposal_Store of the durable workspace directory named at start, SHALL display that directory's absolute path on every page, SHALL keep no copy of the Approval_Ledger in the session directory that is its only copy, and SHALL use the session directory only for temporary artifacts that may be deleted, so that every approval, rejection, and adopted Rule_Revision survives the end of the session, the deletion of the session directory, and a restart of ruletrace-lab; THE RuleTrace_Laboratory SHALL write neither the Approval_Ledger nor the Rulebook from its own process, and SHALL change the bytes of either file only through a ruletrace review approve or ruletrace review reject subprocess, as verified by a test that hashes both files before and after every Laboratory operation and asserts that a change occurs only after an approval or rejection request that passed the checks of criterion 6 and Requirement 37 criterion 19. (Amended by UD-39, 2026-11-18.)

#### 36.11

Old: 11. THE RuleTrace_Laboratory SHALL show in the Brain stage each Pending_Proposal with the label untrusted, with its Origin metadata (provider identifier, protocol, model identifier, and token usage where the Proposal_Store records them), and with each validation Diagnostic (code, Severity, message, and Location) exactly as `ruletrace validate` and `ruletrace-ai` returned it, and SHALL show no Rule_Revision as approved in the Brain stage.

New: 11. THE RuleTrace_Laboratory SHALL show in the Brain stage each Pending_Proposal and each unapproved Rule_Revision as non-authoritative with its origin identified as AI-generated (from a live provider, with the label untrusted), an authored offline demonstration entry (an entry whose script origin is authored), or manually authored (Origin Manual), with its Origin metadata (provider identifier, protocol, model identifier, and token usage where the Proposal_Store records them), and with each validation Diagnostic (code, Severity, message, and Location) exactly as `ruletrace validate` and `ruletrace-ai` returned it, and SHALL show no Rule_Revision as approved in the Brain stage. (Amended by UD-39, 2026-11-18.)

#### 36.14

Old: 14. WHILE no `ruletrace evaluate` subprocess has completed with exit code 0 or 4 for the displayed Compiled_Rulebook, THE RuleTrace_Laboratory SHALL show the Lightning stage as inactive, and WHEN such a subprocess completes, THE RuleTrace_Laboratory SHALL show the Lightning stage as active, bound to the identifier of that Compiled_Rulebook and that evaluation's result, and SHALL return it to inactive when a different Compiled_Rulebook is displayed, so that an AI response, a Proposal storage, an approval, a validation, and a compilation never activate the Lightning stage, as verified by a test for each of these events.

New: 14. WHILE no `ruletrace evaluate` subprocess for the displayed Compiled_Rulebook has been classified as an executed evaluation under criterion 21, THE RuleTrace_Laboratory SHALL show the Lightning stage as inactive, and WHEN such a subprocess completes, THE RuleTrace_Laboratory SHALL show the Lightning stage as active, bound to the identifier of that Compiled_Rulebook and that evaluation's result, and SHALL return it to inactive when a different Compiled_Rulebook is displayed, so that an AI response, a Proposal storage, an approval, a validation, and a compilation never activate the Lightning stage, as verified by a test for each of these events. (Amended by UD-39, 2026-11-18.)

#### 36.20

Old: 20. WHERE demo mode is selected, THE RuleTrace_Laboratory SHALL run the proposal step through `ruletrace-ai` with the Mock_Provider that replays the Recorded_Provider_Script, SHALL need no network access, no credential, and no AI_Configuration of a real provider, SHALL label every demo-mode result as a recorded replay with the provider identifier, the protocol, the model identifier, and the capture date of the script, and SHALL present each Ledger_Record loaded from the Demo_Corpus as a replayed recorded Reviewer decision and not as a new human decision (Requirement 33 criterion 11).

New: 20. WHERE demo mode is selected, THE RuleTrace_Laboratory SHALL run the proposal step through `ruletrace-ai` with the Mock_Provider that replays the Recorded_Provider_Script, SHALL need no network access, no credential, and no AI_Configuration of a real provider, SHALL label every demo-mode result as a replay of a hand-authored script, stating for each script entry the origin that the script records and describing no authored entry as provider output, together with the provider identifier, the protocol, the model identifier, and the capture statement of the script, and SHALL present each Ledger_Record loaded from the Demo_Corpus as a replayed recorded Reviewer decision and not as a new human decision (Requirement 33 criterion 11). (Amended by UD-39, 2026-11-18.)

#### 36.21

Old: 21. IF a subprocess exits with a nonzero exit code, exceeds a limit of Requirement 37 criterion 14, or produces output that is not well formed for its documented format, THEN THE RuleTrace_Laboratory SHALL display the exit code and each Diagnostic that the subprocess wrote to standard error, escaped under Requirement 37 criterion 16, SHALL display no success state for the affected stage, SHALL start no further subprocess for the affected workflow step, and SHALL treat an exit code outside the set of the executable (Requirement 27 criterion 10) as an internal error.

New: 21. THE RuleTrace_Laboratory SHALL classify the end of every subprocess as exactly one of: (a) launch failure, when the executable could not be started; (b) timeout or abnormal termination, when a limit of Requirement 37 criterion 14 was exceeded or the process ended by a signal or without an exit code; (c) an exit code outside the set of the executable (Requirement 27 criterion 10), treated as an internal error; (d) missing or invalid output, when an output file that the step requires is absent from the fresh output directory of the step or is not well formed for its documented format; or (e) a completed run with an exit code of its set. FOR `ruletrace evaluate`, THE RuleTrace_Laboratory SHALL classify a completed run that wrote a well-formed results file as an executed evaluation, whatever its exit code (0, 4, or 2 with a written results file, Requirement 16 criteria 9 and 11 and Requirement 13 criterion 18), and SHALL report for it, as read from the Outcomes, Diagnostics, and verification section of that file and not inferred from the exit code, the number of Indeterminate Outcomes, each Error Diagnostic of the evaluation, and each Decision_Trace marked not verified, and SHALL classify a completed run that wrote no results file as not executed. FOR each run in the classes (a) to (d), for each completed run of a command other than `ruletrace evaluate` whose exit code is not 0, and for each completed `ruletrace evaluate` run that is not an executed evaluation, THE RuleTrace_Laboratory SHALL display the classification, the exit code where one exists, and each Diagnostic that the subprocess wrote to standard error, escaped under Requirement 37 criterion 16, SHALL display no success state for the affected stage, and SHALL start no further subprocess for the affected workflow step. THE RuleTrace_Laboratory SHALL NOT describe an executed evaluation with exit code 4 as a failure to execute, SHALL NOT describe an Indeterminate Outcome as decided, and SHALL NOT state that exit code 4 means only Indeterminate Outcomes. (Amended by UD-39, 2026-11-18.)

#### 36.25

Old: (none)

New: 25. WHEN the user requests the import of proposal batches that a ruletrace-ai run outside the RuleTrace_Laboratory wrote into the Proposal_Store of the workspace (the RuleTrace_Laboratory shows the command, with no credential and no environment value), THE RuleTrace_Laboratory SHALL record the SHA-256 hash of each new batch file as a Hand_Off_Artifact, SHALL have the Proposal_Store validated by a `ruletrace review list` subprocess (Requirement 35 criterion 17), SHALL display for each batch its Request_Identifier, its hash, and its provider identifier, protocol, model identifier, adapter version, reported model, and usage exactly as the batch records them, SHALL treat every Proposal of it as untrusted until a human approval under criterion 6, SHALL offer for approval no Proposal of a store that failed validation, and SHALL write no Proposal_Store file of its own. (Added by UD-39, 2026-11-18.)

#### 37.8

Old: 8. WHEN a Laboratory_Session has had no request for 30 minutes or the session is ended by the user, THE ruletrace-lab executable SHALL invalidate the session identifier so that every later request that carries it receives HTTP status 401. A test with a simulated timer SHALL verify the 30-minute boundary (a request at 29 minutes 59 seconds succeeds, a request at 30 minutes 0 seconds receives 401).

New: 8. WHEN a Laboratory_Session has had no request for 30 minutes or the session is ended by the user, THE ruletrace-lab executable SHALL invalidate the session identifier so that every later request that carries it receives HTTP status 401, and SHALL accept no Launch_Secret until it is restarted; a restart generates a new Launch_Secret under criterion 5, and no Launch_Secret is used for more than one Laboratory_Session. A test with a simulated timer SHALL verify the 30-minute boundary (a request at 29 minutes 59 seconds succeeds, a request at 30 minutes 0 seconds receives 401). (Amended by UD-39, 2026-11-18.)

#### 37.14

Old: 14. THE ruletrace-lab executable SHALL start each subprocess through an argument vector with no shell, SHALL pass each value from a request as exactly one argument element that follows an explicit end-of-options marker or that is part of an option of the form name=value, SHALL resolve the ruletrace and ruletrace-ai executable paths only from the directory of the ruletrace-lab executable or from a path named at start (never from the PATH variable or a request), SHALL pass an explicit allow-list of environment variables, SHALL limit the request body to 1 MiB (an oversized request is refused with HTTP status 413), the captured output of a subprocess to 16 MiB, and the run time of a subprocess to 300 seconds, and SHALL terminate the subprocess and display an error WHEN a limit is exceeded, as verified by tests with values that contain shell metacharacters, a leading hyphen, a newline, and a NUL byte (a value with a NUL byte is refused before any subprocess starts). WHEN a subprocess exceeds the output limit or the run-time limit, THE ruletrace-lab executable SHALL terminate it, SHALL wait for its exit so that no child process remains, SHALL display a Diagnostic that names the limit and the workflow step, SHALL display none of the captured output as if it were complete (no silent truncation), SHALL use no output file of that step, and SHALL leave no temporary file of that step in the session directory after the session ends. The 1 MiB limit applies to ordinary control requests; file uploads have the separate limits of criterion 26. (Clarified at the requirements checkpoint, 2026-11-18.)

New: 14. THE ruletrace-lab executable SHALL start each subprocess through an argument vector with no shell, SHALL pass each value from a request as exactly one argument element placed immediately after the option name that takes it, in the grammar that the ruletrace and ruletrace-ai command-line interfaces accept (Requirement 17, design §3.9), SHALL use no end-of-options marker and no option of the form name=value, which neither interface supports, SHALL refuse before any subprocess starts a value that begins with a hyphen or contains a NUL byte, a line break, or another control character, SHALL resolve the ruletrace and ruletrace-ai executable paths only from the directory of the ruletrace-lab executable or from a path named at start (never from the PATH variable or a request), SHALL start every subprocess, ruletrace and ruletrace-ai alike, with an environment built only from an explicit allow-list and forward no inherited variable, SHALL limit the request body to 1 MiB (an oversized request is refused with HTTP status 413), the captured output of a subprocess to 16 MiB, and the run time of a subprocess to 300 seconds, and SHALL terminate the subprocess and display an error WHEN a limit is exceeded, as verified by tests with values that contain shell metacharacters, a leading hyphen, a newline, and a NUL byte (a value with a NUL byte is refused before any subprocess starts). WHEN a subprocess exceeds the output limit or the run-time limit, THE ruletrace-lab executable SHALL terminate it, SHALL wait for its exit so that no child process remains, SHALL display a Diagnostic that names the limit and the workflow step, SHALL display none of the captured output as if it were complete (no silent truncation), SHALL use no output file of that step, and SHALL leave no temporary file of that step in the session directory after the session ends. The 1 MiB limit applies to ordinary control requests; file uploads have the separate limits of criterion 26. (Clarified at the requirements checkpoint, 2026-11-18.) (Amended by UD-39, 2026-11-18.)

#### 37.17

Old: 17. WHEN the user ends the session, WHEN the ruletrace-lab process receives an interrupt or terminate signal, or WHEN the process starts an orderly shutdown, THE ruletrace-lab executable SHALL close the listener, SHALL invalidate every session, SHALL terminate each running subprocess, SHALL delete the temporary directory of the Laboratory_Session, and SHALL exit with code 0.

New: 17. WHEN the user ends the session, WHEN the ruletrace-lab process receives an interrupt or terminate signal, or WHEN the process starts an orderly shutdown, THE ruletrace-lab executable SHALL close the listener, SHALL invalidate every session, SHALL terminate each running subprocess, SHALL delete the temporary directory of the Laboratory_Session, and SHALL exit with code 0. THE RuleTrace SHALL ensure, by a process-containment mechanism that the design selects for each platform of Requirement 21 criterion 10, that every ruletrace and ruletrace-ai subprocess terminates and is reaped also WHEN ruletrace-lab terminates abnormally (a crash or a kill signal that it cannot handle), as verified on each such platform by a test that kills ruletrace-lab while a subprocess runs and asserts that no subprocess of it remains after a bounded time that the design states, and WHERE this cannot be demonstrated on a platform, THE RuleTrace SHALL report the limitation, SHALL NOT claim the protection for that platform, and SHALL revise the architecture before the Laboratory is released on it. (Amended by UD-39, 2026-11-18.)

#### 37.20

Old: 20. THE RuleTrace_Laboratory SHALL NOT contain the AI provider credential in its process environment, in any argument vector, in any file that it writes, in any log line, or in any HTTP response, and THE RuleTrace SHALL include a test that supplies a canary credential through the mechanism of criterion 21, runs a complete proposal workflow against a loopback stub provider that records the Authorization header, and asserts that the stub received the canary from ruletrace-ai and that the canary appears nowhere in the environment of the ruletrace-lab process, the files of the workspace and the session directory, the standard output and standard error of ruletrace-lab, and the bodies and headers of every HTTP response.

New: 20. THE RuleTrace_Laboratory SHALL NOT contain the AI provider credential in its process environment, in any argument vector, in any file that it writes, in any log line, or in any HTTP response, and THE RuleTrace SHALL include a test that supplies a canary credential through the mechanism of criterion 21, runs ruletrace-ai outside the Laboratory against a loopback stub provider that records the Authorization header, imports the resulting batch into the Laboratory under Requirement 36 criterion 25, runs the complete workflow from import through review, validation, compilation, evaluation, and proof, and asserts that the stub received the canary from ruletrace-ai and that the canary appears nowhere in the environment of each ruletrace subprocess or in the environment of the ruletrace-lab process, the files of the workspace and the session directory, the standard output and standard error of ruletrace-lab, and the bodies and headers of every HTTP response. (Amended by UD-39, 2026-11-18.)

#### 37.21

Old: 21. THE RuleTrace SHALL record in a Laboratory threat model, before the implementation of ruletrace-lab starts, the mechanism by which ruletrace-ai receives its credential when the RuleTrace_Laboratory starts it, and SHALL select only a mechanism under which the credential is not reachable by the ruletrace-lab process. Whether that mechanism requires a change to Requirement 28 (credential only from the environment variable named in the AI_Configuration) is OPEN for the L1 threat model and needs an explicit user decision before any change to Requirement 28. IF the AI_Configuration that a user names names a credential environment variable that is present in the environment of the ruletrace-lab process, THEN THE ruletrace-lab executable SHALL start no subprocess, SHALL write an Error Diagnostic that names the variable and not its value, and SHALL exit with code 2.

New: 21. THE RuleTrace SHALL keep one registry file of the names, and never the values, of the provider credential environment variables (each name that a committed AI_Configuration, example configuration, or continuous integration workflow uses, and each name that a Provider_Profile documents), with a test that fails WHEN a committed configuration or workflow names a credential variable that the registry lacks; SHALL deliver launch scripts for the platforms of Requirement 21 criterion 10 that start ruletrace-lab with an environment built only from an explicit allow-list that contains no registered name, with a test per script that sets every registered name to a canary value and asserts that no registered name and no canary reaches the ruletrace-lab process; and IF a registered name is present in the environment of the ruletrace-lab process, THEN THE ruletrace-lab executable SHALL start no subprocess, SHALL write an Error Diagnostic that names the variable and not its value, and SHALL exit with code 2. THE RuleTrace_Laboratory SHALL use a registered variable only to test its presence and SHALL NOT retain, display, transmit, or log its value. The security boundary is that no code path of ruletrace-lab receives a credential; this does not isolate a credential from another process of the same operating-system user, and THE RuleTrace SHALL state this limit in the Laboratory documentation. Requirement 28 is unchanged. (Amended by UD-39, 2026-11-18.)

#### 37.23

Old: 23. THE RuleTrace SHALL include tests of criteria 1 to 22 and 24 to 26 that assert the exact HTTP status, the exact header values, the exit code, and the byte-level state of the Approval_Ledger and of the workspace files, and SHALL include for each of criteria 3, 4, 7, 9, 13, 14, 19, and 20 at least one negative test that fails when the check is removed, demonstrated by a mutant that removes the check.

New: 23. THE RuleTrace SHALL include tests of criteria 1 to 22 and 24 to 27 that assert the exact HTTP status, the exact header values, the exit code, and the byte-level state of the Approval_Ledger and of the workspace files, and SHALL include for each of criteria 3, 4, 7, 9, 13, 14, 19, and 20 at least one negative test that fails when the check is removed, demonstrated by a mutant that removes the check. (Amended by UD-39, 2026-11-18.)

#### 37.26

Old: 26. THE ruletrace-lab executable SHALL accept a file upload only through an upload request that names the kind of the file, SHALL apply to each upload an explicit maximum size for that kind that the design states and that is not greater than the size limit of that kind of input in Requirement 19 and the design limits, SHALL apply an explicit maximum to the total size of the uploads of one Laboratory_Session, SHALL refuse an upload above a maximum with HTTP status 413 before it reads more of the body than the maximum allows, SHALL accept an upload whose size equals the maximum, and SHALL accept no request body, upload or other, without a maximum size. (Added at the requirements checkpoint, 2026-11-18.)

New: 26. THE ruletrace-lab executable SHALL accept a file upload only through an upload request that names the kind of the file, SHALL apply to each upload a maximum of 16 MiB (16,777,216 bytes) or the lower size limit of that kind of input in Requirement 19 and the design limits, SHALL apply a maximum of 64 MiB (67,108,864 bytes) to the total size of the accepted uploads of one Laboratory_Session, SHALL enforce each maximum while the body streams in and before it buffers or writes to disk more than the maximum allows, SHALL refuse an upload above a maximum with HTTP status 413, SHALL delete every partial file of a refused or interrupted upload, SHALL accept an upload whose size equals the maximum, and SHALL accept no request body, upload or other, without a maximum size. (Added at the requirements checkpoint, 2026-11-18.) (Amended by UD-39, 2026-11-18.)

#### 37.27

Old: (none)

New: 27. THE ruletrace-lab executable SHALL hold at most 16 open connections and SHALL process at most 4 requests at the same time, SHALL apply finite timeouts that the design states to reading the request line and headers, to reading the body, and to writing the response, SHALL answer a connection beyond either limit with HTTP status 503 and a Retry-After header WHERE a response can be written and otherwise close it without a response, SHALL release every resource of a closed, refused, or timed-out connection (thread, buffer, partial upload), and THE RuleTrace SHALL include tests for each limit and timeout at and beyond its value and for the release of those resources. (Added by UD-39, 2026-11-18.)

#### register UD-39

Old: (none)

New: - **UD-39 RuleTrace Laboratory final design decisions** (Confirmed by the user 2026-11-18, see `tools/decisions/UD-39.md`; affects 7.16, 24.13, 34.9, 34.10, 36.2, 36.4, 36.11, 36.14, 36.20, 36.21, 36.25, 37.8, 37.14, 37.17, 37.20, 37.21, 37.23, 37.26, 37.27): live provider generation external with proposal import; launch scripts and a credential-name registry; standard-library server; restart after session end; upload and connection limits; durable workspace for approvals; honest origin labels; `RT-LAB` family; 9.6 partial. LT-6 (cross-port cookie exposure) and LT-7 (process containment on crash) stay OPEN.

Not changed by UD-39: Requirement 28 (all criteria), 37.5 and 37.6 (LT-6 is open and may change them later), 37.23's mutant list (RN-48 open).

Corrected after the review `docs/reviews/2026-11-18-review-ud39-6c85779.md` (F4, F5, F13): the New text of 36.2, 36.21 and 37.21 above is the corrected text.

### UD-40 clarifications (2026-11-18)

Record: `tools/decisions/UD-40.md`.

#### 36.25

Old: 25. WHEN the user requests the import of proposal batches that a ruletrace-ai run outside the RuleTrace_Laboratory wrote into the Proposal_Store of the workspace (the RuleTrace_Laboratory shows the command, with no credential and no environment value), THE RuleTrace_Laboratory SHALL record the SHA-256 hash of each new batch file as a Hand_Off_Artifact, SHALL have the Proposal_Store validated by a `ruletrace review list` subprocess (Requirement 35 criterion 17), SHALL display for each batch its Request_Identifier, its hash, and its provider identifier, protocol, model identifier, adapter version, reported model, and usage exactly as the batch records them, SHALL treat every Proposal of it as untrusted until a human approval under criterion 6, SHALL offer for approval no Proposal of a store that failed validation, and SHALL write no Proposal_Store file of its own. (Added by UD-39, 2026-11-18.)

New: 25. WHEN the user requests the import of proposal batches that a ruletrace-ai run outside the RuleTrace_Laboratory wrote into the Proposal_Store of the workspace (the RuleTrace_Laboratory shows the command, with no credential and no environment value), THE RuleTrace_Laboratory SHALL refuse, before reading it, a batch path that is outside `<workspace>/proposals`, contains a parent-directory component, or is or passes through a symbolic link, SHALL record the SHA-256 hash of each new batch file as a Hand_Off_Artifact, SHALL check the structure, the schema, the content hash, and the source metadata of each batch before presenting it, SHALL have the Proposal_Store validated by a `ruletrace review list` subprocess (Requirement 35 criterion 17), SHALL display for each batch its Request_Identifier, its hash, and its provider identifier, protocol, model identifier, adapter version, reported model, and usage exactly as the batch records them, SHALL treat every Proposal of it as untrusted until a human approval under criterion 6 and SHALL NOT present a valid hash as approval or as evidence that the proposed interpretation is correct, SHALL offer for approval no Proposal of a store that failed validation, and SHALL write no Proposal_Store file of its own. (Added by UD-39, 2026-11-18.) (Clarified by UD-40, 2026-11-18.)

#### 36.21

Old: 21. THE RuleTrace_Laboratory SHALL classify the end of every subprocess as exactly one of: (a) launch failure, when the executable could not be started; (b) timeout or abnormal termination, when a limit of Requirement 37 criterion 14 was exceeded or the process ended by a signal or without an exit code; (c) an exit code outside the set of the executable (Requirement 27 criterion 10), treated as an internal error; (d) missing or invalid output, when an output file that the step requires is absent from the fresh output directory of the step or is not well formed for its documented format; or (e) a completed run with an exit code of its set. FOR `ruletrace evaluate`, THE RuleTrace_Laboratory SHALL classify a completed run that wrote a well-formed results file as an executed evaluation, whatever its exit code (0, 4, or 2 with a written results file, Requirement 16 criteria 9 and 11 and Requirement 13 criterion 18), and SHALL report for it, as read from the Outcomes, Diagnostics, and verification section of that file and not inferred from the exit code, the number of Indeterminate Outcomes, each Error Diagnostic of the evaluation, and each Decision_Trace marked not verified, and SHALL classify a completed run that wrote no results file as not executed. FOR each run in the classes (a) to (d), for each completed run of a command other than `ruletrace evaluate` whose exit code is not 0, and for each completed `ruletrace evaluate` run that is not an executed evaluation, THE RuleTrace_Laboratory SHALL display the classification, the exit code where one exists, and each Diagnostic that the subprocess wrote to standard error, escaped under Requirement 37 criterion 16, SHALL display no success state for the affected stage, and SHALL start no further subprocess for the affected workflow step. THE RuleTrace_Laboratory SHALL NOT describe an executed evaluation with exit code 4 as a failure to execute, SHALL NOT describe an Indeterminate Outcome as decided, and SHALL NOT state that exit code 4 means only Indeterminate Outcomes. (Amended by UD-39, 2026-11-18.)

New: 21. THE RuleTrace_Laboratory SHALL classify the end of every subprocess as exactly one of: (a) launch failure, when the executable could not be started; (b) timeout or abnormal termination, when a limit of Requirement 37 criterion 14 was exceeded or the process ended by a signal or without an exit code; (c) an exit code outside the set of the executable (Requirement 27 criterion 10), treated as an internal error; (d) missing or invalid output, when an output file that the step requires is absent from the fresh output directory of the step or is not well formed for its documented format; or (e) a completed run with an exit code of its set. FOR `ruletrace evaluate`, THE RuleTrace_Laboratory SHALL classify a completed run that wrote a well-formed results file as an executed evaluation, whatever its exit code (0, 4, or 2 with a written results file, Requirement 16 criteria 9 and 11 and Requirement 13 criterion 18), and SHALL report for it, as read from the Outcomes, Diagnostics, and verification section of that file and not inferred from the exit code, the number of Indeterminate Outcomes, each Error Diagnostic of the evaluation, and each Decision_Trace marked not verified, and SHALL classify a completed run that wrote no results file as not executed. FOR each run in the classes (a) to (d), for each completed run of a command other than `ruletrace evaluate` whose exit code is not 0, and for each completed `ruletrace evaluate` run that is not an executed evaluation, THE RuleTrace_Laboratory SHALL display the classification, the exit code where one exists, and each Diagnostic that the subprocess wrote to standard error, escaped under Requirement 37 criterion 16, SHALL display no success state for the affected stage, and SHALL start no further subprocess for the affected workflow step. FOR every executed evaluation, THE RuleTrace_Laboratory SHALL show the execution status (executed; results available and validated) separately from the provenance status (verified, or not verified with each Diagnostic), SHALL label no result or proof as verified whose Decision_Trace the Explainer did not verify, SHALL show every Error Diagnostic of the evaluation beside the Outcomes so that no Indeterminate Outcome conceals it, and SHALL NOT state that an executed evaluation means that every decision is decided or trustworthy. THE RuleTrace_Laboratory SHALL NOT describe an executed evaluation with exit code 4 as a failure to execute, SHALL NOT describe an Indeterminate Outcome as decided, and SHALL NOT state that exit code 4 means only Indeterminate Outcomes. (Amended by UD-39, 2026-11-18.) (Clarified by UD-40, 2026-11-18.)

RN-51 accepted (UD-40 section 1); RN-52 decided as an ownership rule with an audit (`docs/rn-52-ownership-audit.md`); RN-38 accepted as a Rule_IR version 1 invariant with regression coverage.

### UD-41 amendment (2026-11-18)

#### 24.12

Old: 12. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of each Application_Layer crate, which are limited to the Rules_Engine crate, the crates for the Compiled_Rulebook and Competition_Data types and their parsers, the Explainer crate, the Source_Model crate, and the shared Diagnostic, Location, and Exact_Number crates, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN an Application_Layer depends, directly or transitively, on the ruletrace binary crate, on the review library crate, on a crate that contains the Approval_Ledger append function, on the Validator, Compiler, Ingestor, or Extractor crate, on an Auxiliary_Crate, or on a network client library, or WHEN a Core_Crate or Auxiliary_Crate depends on an Application_Layer crate. (Amended by UD-38, 2026-11-18.)

New: 12. THE RuleTrace SHALL add to the allowed-dependency list of criterion 5 the edges of each Application_Layer crate, which are limited to the Rules_Engine crate, the crates for the Compiled_Rulebook and Competition_Data types and their parsers, the Explainer crate, the Source_Model crate, and the shared Diagnostic, Location, and Exact_Number crates, and SHALL fail the automated check of criterion 5, with the offending crate name and edge, WHEN an Application_Layer depends, directly or transitively, on the ruletrace binary crate, on the review library crate, on a crate that contains the Approval_Ledger append function, on the Validator, Compiler, Ingestor, or Extractor crate, on an Auxiliary_Crate, or on a network client library, or WHEN a Core_Crate or Auxiliary_Crate depends on an Application_Layer crate. (Amended by UD-38, 2026-11-18.) THE Competition_Data crate MAY depend on the Compiled_Rulebook types crate only to read compiled rulebook declarations (input and collection types, enumeration members, and the rule body kind of each Target) in order to derive the input-mapping schema and the Target kinds, and THE RuleTrace SHALL fail the automated check of criterion 5 WHEN the Competition_Data crate depends, directly or transitively, on the Rules_Engine, the Compiler, the Validator, the Explainer, the review library crate, the ruletrace binary crate, or an Auxiliary_Crate. (Amended by UD-41, 2026-11-18.)

Also: design.md §3.1 citation "Req 15.9" corrected (UD-41).
