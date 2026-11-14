# Consistency Analysis: requirements revision of 2026-11-14

Analysis artifact, not a spec document. Criterion references are Requirement.criterion.

## 1. Conflicts found and resolved

| # | Earlier criterion | Conflicting new decision or criterion | Resolution and reason |
| --- | --- | --- | --- |
| 1 | 17.1 (exactly four commands) | UD-22 review inside ruletrace | 17.1 now says four pipeline commands plus the review group (list, approve, reject). Recommended wording adopted. |
| 2 | 5.12 (no CLI command performs a transition; Review_Tool only) | UD-22 | 5.12 now names the Review_Group as the only code path that appends to the ledger. Review_Tool retired everywhere. |
| 3 | 5.5 (Warning for Proposed or Rejected) | Approval item 4 | Proposed (unresolved) is an Error APPROVAL_UNRESOLVED (5.5, 26.12). Rejected keeps a Warning (5.14, 26.13), so visibility is kept. |
| 4 | 5.7 (empty Compiled_Rulebook with Warning) | Approval item 7 | Error APPROVAL_NO_EXECUTABLE_RULES, no artifact (5.7, 9.13). |
| 5 | 25.13 (Proposed higher revision, Effective stays executable) | Approval item 4, UD-12 | The unresolved revision blocks compilation (25.13). |
| 6 | 5.1, 25.16 (stored Approval_Status vs record) | UD-6 ledger | Status is derived (5.1, 26.10). A stored status is an invalid input under 22.17. 25.16 now covers ledger contradictions. |
| 7 | 5.4, 25.11 (Approved without matching record is an Error) | Derived status | Non-matching approval derives Proposed, so APPROVAL_UNRESOLVED (Error) plus the stale Warning. Blocking and the no-fallback rule are kept. Chosen over "Error with Approved status" because that state is not constructible. |
| 8 | 23.10 (reject batch if any check of 5 to 9 fails, includes semantic validation) | UD-20 | 23.10 limited to 5 to 8. Semantic outcomes are stored (30.2). Malformed batches are still never partially accepted. |
| 9 | 23.17, 23.18 (retry kinds, "return last failure", design defaults) | UD-19 | Replaced by Requirement 29. Per-attempt Warnings plus distinct final Errors for exhaustion and deadline. |
| 10 | 16.16 (shared AI_Tool and Review_Tool exit table) | UD-22, UD-20 | Review rows join the ruletrace table (16.15). ruletrace-ai has its own table (16.16). Exit 3 no longer means semantic rejection. |
| 11 | 16.17 (earliest step) | Config load, provider call, parse, provenance | Stage order per executable, with a table for ruletrace-ai (16.17). |
| 12 | 22.2 (blocking set) | Unresolved approval is Blocking | Set unchanged by Severity (Error). 22.2 now names the approval Errors. No new Severity, so 22.4 and 22.5 need no change. 22.18 derives the unresolved flag from codes. |
| 13 | 22.16 (provider flags rejected under 22.9 and 22.10 with exit 2) | Provider_Response is a protocol failure | Whole-batch rejection with AI_RESPONSE_FORBIDDEN_FIELD, exit 6. Other inputs of 22.16 keep exit 2. |
| 14 | 23.16 (Review_Tool revises) | Review group has no revise | Revision is authored in the Rulebook (25.10) or proposed by ruletrace-ai. |
| 15 | UD-18 earlier text (env var or file path) | UD-18 decision | Env var name only (28.2, 28.4). |
| 16 | 17.8 (ruletrace reads no AI config) vs reviewer must not equal AI identities | UD-22 | AI identities are recorded in Origin metadata and the Proposal_Store by ruletrace-ai (23.11), so review reads no AI_Configuration (23.15, 17.11). |
| 17 | 18.13, 24.5 (AI crates must not depend on the approval crate) | Ledger crate | Extended to the ledger-append crate and to the ruletrace dependency tree (24.5, 27.4, 27.5). |
| 18 | 4.3 (unknown field is Unsupported) | 22.17 | Lifecycle and flag field names are Errors with RT-DGN codes and take precedence (4.10). |
| 19 | 8.4, 8.5, 8.7, 8.11, 8.13, 8.16 | UD-3 pending | Reworded to hold under both options. Option-specific content in 8.20 to 8.27, tagged UD-3-dependent. Literal limits identical (8.27). |
| 20 | 9.5 (hash excludes Approval_Records) | Ledger bindings | Consistent. Approvals are outside the Compiled_Rulebook hash, and 9.12 re-derives them. 16.18 byte identity with a manual Rulebook holds. |
| 21 | 20.4 fixtures contain an approved Rulebook | Ledger | Fixture now has a Rulebook and an Approval_Ledger. |
| 22 | 21.8 (no listener) | Loopback stub | Already handled by the earlier exception. Unchanged. |
| 23 | Rejected terminal (23.14) | Content edit after rejection | 25.19 and APPROVAL_REJECTED_REVISION_MODIFIED keep it Rejected and blocking when its content changes. |

## 2. Approval, atomicity and Rulebook completeness

- Atomicity (9.10): Compile output is the complete set of Effective_Revisions or nothing. Omitting an unresolved revision to compile the rest is forbidden explicitly.
- Completeness (26.15): every Rule identifier has an Effective_Revision with no unresolved higher revision, or only Rejected and superseded revisions, and at least one Effective_Revision exists.
- Superseded Approved revisions: retained, excluded, structural checks only (25.12). They never block.
- Rejected revisions: retained with a reason in the ledger, excluded, never block. A reference to a Rule identifier whose revisions are all Rejected is an undeclared reference Error (6.3).
- Non-matching approval: Proposed again, unresolved, blocks. The lower Approved revision is not used as a fallback (25.11).
- Lower Proposed revision under a higher Approved or Rejected one: not unresolved, by the definition the user gave (26.11). Accepted consequence: a Proposed rev 1 beneath a Rejected rev 2 does not block. It cannot execute anyway.
- Pending unadopted Proposal_Store entries are not Rules of the Rulebook and do not block (UD-21, resolved by the user 2026-11-14).
- Authoring cost: validate fails while any revision awaits a decision. This follows approval item 4.

## 3. Checklist of the eleven checks

| # | Check | Result | Where |
| --- | --- | --- | --- |
| 1 | Criteria independently testable | Yes. Each new criterion names its code, exit code, and a test criterion exists per new requirement (26.23, 27.10, 28.16, 29.14, 30.8). Weak spot: 28.4 credential-looking detection is defined by name and prefix lists, to be finalized in design. | 26 to 30 |
| 2 | Every AI failure mode explicit | Yes. Config, inline secret, endpoint, limits, provider, credential, consent, log, each transport kind, retry exhaustion, deadline, TLS, malformed, schema, forbidden field, too large, incomplete, provenance (4), store, request, duplicate batch. | 7.17 |
| 3 | Human approval not bypassable | Yes. No flag, config, env var, Rulebook or Proposal_Store field, or response sets Approved (26.18). Ledger written only by the Review_Group (5.12, 26.19). Blocking approval refused (26.16). Reviewer rules (23.15, 26.17). ruletrace-ai has no dependency on the ledger crate (27.4). | 5, 23, 26, 27 |
| 4 | Provenance verified independently of AI | Yes. Hash, span, quote recomputed from Source_Models. AI line, column, hash, quote, status claims never trusted (30.4, 23.7, 23.8). | 30.4 |
| 5 | Provider failure cannot affect deterministic execution | Yes. ruletrace loads no provider code, its tree has no network crate (27.5, 27.6). Failed ruletrace-ai leaves Rulebook, ledger, Proposal_Store unchanged (27.8, 18.17). | 27 |
| 6 | Executable responsibilities | Yes. Table in 27.1. | 27.1 |
| 7 | Config and credential security | Yes. Env var name only, inline secret rejection, https only except loopback, no TLS bypass option, redaction, no secrets in fixtures, permission warning, opt-in consent, log without text. | 28 |
| 8 | Retry cannot cause duplicates | Yes. Sequential attempts, one parsed response, late responses discarded, deterministic Request_Identifier and Proposal identifiers, atomic idempotent store, concurrent runs yield one batch. | 29.7 to 29.12 |
| 9 | Catalogue contains all new codes | Yes, with the mnemonic to identifier mapping. Validator codes of Requirements 1 to 14 remain design-assigned. | 7.16 to 7.20 |
| 10 | Exit codes consistent with earliest failing stage | Yes. 16.15 to 16.17 and 16.19. Warnings never change the code (7.19, 22.19). | 16 |
| 11 | Requirements, steering, architecture terminology consistent | Yes after this revision. Steering files updated to two executables, ledger, Review_Group, blocking classes, retry policy, consent, config rules, pending numeric choice. Residual: the design phase must name the crates. | steering |

## 4. Open points that need the user

UD-3 final choice and sub-option (R-128 or R-Big; preferred bounded rational, pending design documentation). UD-14. UD-23. UD-26. UD-27.

## 5. Scope amendment of 2026-11-14: Application_Layers and demonstration

### 5.1 Conflicts found and resolved

| # | Earlier criterion or statement | Conflicting new decision | Resolution and reason |
| --- | --- | --- | --- |
| 24 | 14.5 (qualification unsupported) | Bracket_Check demonstrates qualification | 14.5 now excludes slot-based qualification only. Qualification is defined as a Qualification_Predicate, an eligibility predicate over fields, derived values, and weighted scores (14.11). No capability is added. |
| 25 | 14.2 (exactly these expression operations) | Rank-based qualification needs a rank reference | 14.2 is unchanged. 14.12 adds one operation only WHERE the user selects Rank_Reference (UD-32, Open). 14.6 and 14.8 name criterion 12. Under Threshold_Only, a rank-based clause is Unsupported (14.13). |
| 26 | 15.8 (no Prize_Check or Bracket_Check crate) | Two Application_Layer crates | 15.8 rewritten: exactly two Application_Layer crates and no allocation, slot-based qualification, seeding, bracket, or progression code. 15.1, 15.5, 15.6, 15.7 kept and made testable by 31.4, 31.5, 31.10, 31.11, 24.12. |
| 27 | 27.1 (exactly two executables, no third) and the Requirement 27 user story; UD-22 | Two Demonstration_Executables | 27.1 amended to exactly two RuleTrace executables plus two Demonstration_Executables (UD-33, Recommended-pending). 27.15 tests the executable set. The criteria 27.5 to 27.13 apply to ruletrace and ruletrace-ai unchanged. |
| 28 | Introduction and Scope (Prize Check and Bracket Check out of scope; this specification delivers only the shared foundation) | Scope amendment | Rewritten. The full applications stay out of scope. |
| 29 | 16.15 (no other exit code from ruletrace) | Application exit codes | No conflict. The Demonstration_Executables are separate executables with their own table (31.12), which returns the same code as ruletrace evaluate for the same inputs. |
| 30 | 24.3 (no file access in Core_Crates) | Application binaries read and write files | No conflict. Application_Layer library crates get the same restrictions (31.11). Only the Demonstration_Executables open the files that their arguments name. |
| 31 | 28.11 (public documents only) | Real documents in the Demo_Corpus | Kept, extended by 28.21 and 33.2 to 33.5. |
| 32 | 30.1 and 23.x (a rejected Provider_Response is never stored; no raw response is kept) | A Recorded_Provider_Script needs the unmodified response bytes | Not silently changed. UD-36 (Recommended-pending) records three capture options. 27.3 stays unchanged under the recommended option 1. |
| 33 | 18.x and 27.9 (ruletrace independent of AI) | Demonstration flow uses ruletrace-ai and ruletrace together | No conflict. The flow uses the existing executables only. The acceptance test also asserts the dependency tree of ruletrace (33.15). |

### 5.2 Checks

- No safety criterion is weakened. The approval gate, the ledger writer, the dependency checks, the no-network core, determinism, isolation, provenance, the diagnostic state, and exact numeric criteria are unchanged. New criteria add checks (24.12, 22.22, 31.10, 31.11).
- The Application_Layers add no Rule kind and no operation (31.10, 32.5), unless the user selects Rank_Reference.
- Honest claims: 34.1 to 34.8 make the limits of the demonstration testable by a documentation scan.

### 5.3 Open points that need the user

UD-32 (rank-based qualification: Threshold_Only or Rank_Reference), UD-34 (choice of the real public documents), UD-35 (real or synthetic Competition_Data), UD-33 (confirm the separate Demonstration_Executables), UD-36 (capture of the recorded provider response).

### 5.4 Follow-up for later phases (not done here)

The design (section 8, the crate list, the phase plan, the traceability table) and tasks.md describe the earlier scope and must be revised after the user answers UD-32 to UD-36. The design must also assign the catalogue family of the APP codes and fix the Application_Input_Format.
