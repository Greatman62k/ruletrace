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
