# Implementation Plan: ruletrace-compiler

## Overview

The implementation language is Rust (stable, edition 2021 or later, toolchain pinned in `rust-toolchain.toml`), as fixed by the design. The plan follows design §7.5: three demonstration slices and a hardening phase. Each slice ends with a runnable demonstration checkpoint and the usual fmt, clippy, test and graph checkpoint.

| Slice | Phases (top-level task groups) | What it demonstrates |
| --- | --- | --- |
| Demo Slice 0 | Phases 0 to 4 (groups 2 to 6) | `ruletrace` ingests a document, a person authors rules with source spans, a human approves them with `ruletrace review`, and `validate`, `compile`, `evaluate` produce Outcomes, Decision_Traces and a replayable Proof_Report with source text. No AI. The `hand` profile of the demonstration harness reproduces the flow on three platforms with byte-identical goldens. The document is a labelled stand-in text until the user confirms real documents (UD-34). |
| Demo Slice 1 | Phases 5 and 6 (groups 7 and 8) | The same flow with rules proposed by `ruletrace-ai` through the mock provider replaying an authored script, adopted by human approval (two-file journaled commit), every Candidate_Clause classified in the Coverage_Statement, and origin independence (`ai` and `hand` profiles compile to equal bytes). |
| Demo Slice 2 | Phases 7 to 9 (groups 9 to 11) | `prize-check` and `bracket-check` over the same Compiled_Rulebook with results byte-identical to `ruletrace evaluate`; the Responses-protocol provider (Meta Model API first, UD-37) and a recorded script from a real provider; real documents and data confirmed by the user; published-result checks; mutation checks; honest-claims scan; required `demo-offline` CI job. |
| Hardening | Phase 10 (group 12) | Breadth tests, parser no-panic runs, catalogue baseline and release checks. Required before a release tag, not before a slice exit. |

Conventions:

- Phase N is the top-level group N+1 (group 1 holds the decision gates). Each task names its requirement numbers (`_Requirements_`), its design section, and an `Observable:` line, which is the result a reviewer can check.
- Decision gates are ordinary tasks in group 1. A gate task writes a record under `tools/decisions/`, asks the user with the user_input tool, and then stops until the answer arrives. The record carries `status: Pending` or `status: Confirmed`. `rt-xtask check-decisions` (task 2.10) fails if a crate that `tools/decision-gates.v1` ties to a decision is a workspace member while that decision is `Pending`. A task that lists a gate under `Blocked by` must not start before the gate record is `Confirmed`.
- Required tests are not starred. A `*` marks a breadth test that no slice exit needs. Breadth tests that every release needs are in Phase 10 and are not starred there. No test of Requirements 20, 22 and 24 to 27 is removed or weakened. Every one of the 52 design properties appears in at least one task.
- A phase does not exit while its own gate tests fail. The `rt-xtask` checks named in each checkpoint are `check-graph`, `check-tree`, `scan-sources`, `check-catalogue`, `check-justification` and `check-decisions`, plus `check-app-layers`, `check-executables`, `gen-claims` and `scan-claims` once they exist.
- The demonstration is document-agnostic. No task names or assumes a real document. Until the user confirms documents under UD-34, Slices 0 and 1 use a stand-in text that is labelled as a stand-in in the manifest and is not part of the Demo_Corpus.
- Tasks that need a person (live recording, human review decisions on real documents) are gate tasks. They prepare the commands and stop for the user.

## Decisions needed

Each decision has a gate task in group 1. The recommendation is the one the design already follows (`Recommended-applied` or `Recommended-pending` in design §9.2). A gate does not block anything that does not list it.

| Gate task | Decision | Recommendation | Blocks | Needed by |
| --- | --- | --- | --- | --- |
| 1.1 | UD-15 and DD-3: CI runner labels, toolchain version, whether `File::try_lock` is accepted | Labels `ubuntu-24.04`, `macos-15`, `windows-2022`. Pin a full `major.minor.patch` at or above 1.89 so `File::try_lock` is available (otherwise the lock-file fallback of §3.8.3 applies). | Workspace skeleton and every later task | Wave 0 |
| 1.2 | UD-2 and UD-9: serialization format and size limits | RT-JSON, a strict hand-written positioned subset, and the default limits of §4.1.5 | `rt-json` (2.7) and every parser of a serialized artifact | Wave 0 |
| 1.3 | UD-3: Exact_Number representation | R-128 bounded rational (matches the stated preference for rational) with decimal literals | `rt-numeric` and all crates that use numbers (`rt-ir`, `rt-data`, `rt-compiled`, `rt-engine`) | Wave 0 |
| 1.4 | UD-27: ledger format and atomic commit | JSON Lines ledger and a journaled two-file commit | `rt-ledger` parse and print (4.7), `rt-review` (6.1), the commit protocol (7.3) | Wave 0 |
| 1.5 | UD-14: exit-code classification | Follow §6.1, including exit 2 for `RT-VAL-0017` and `RT-VAL-0018` (missing or mismatched Source_Model) | `validate` command and exit-code tests | Wave 0 |
| 1.6 | RN-29: may `validate` write its Validation_Result artifact when blocking Diagnostics exist (Req 17.4 and 16.9 say a failed command creates no file at its output location, but Req 33.17 needs `compile` to exit 3 after `validate`) | Let `validate` write the artifact when only blocking Diagnostics prevent success, amend the exception lists of 17.4 and 16.9, and let `compile` refuse such an artifact with exit 3. Alternative: change 33.17 so that `compile` exits 2 when no artifact exists. | `validate` command (5.4), `compile` command (5.14), mutation M3 (6.9, 11.7) | Wave 0 |
| 1.7 | UD-23: Reviewer identity check | Exact comparison with the identities recorded in Origin and the batch. Documented residual risk R-8 (names are not authenticated). | `rt-review` (6.1) | Wave 0 |
| 1.8 | UD-36: capture of the recorded provider response | Option 1: the Proposal_Store retains the accepted response, and `rt-xtask export-script` writes the script | Proposal_Store format (7.1), `export-script` (10.6) | After Slice 0 |
| 1.9 | UD-26: exit code when a stored proposal carries blocking semantic findings | Exit 0 with the Warning `RT-AI-0305` | `propose` exit codes (8.7, 8.8) | After Slice 0 |
| 1.10 | UD-34: demonstration documents | The user supplies or confirms real documents against the selection criteria of Req 33.3. Until then the stand-in stays. | Real Demo_Corpus (11.1), live recording (10.8), UD-32 | After Slice 0 |
| 1.11 | UD-32: rank-based qualification | Rank_Reference if the confirmed tournament document states qualification as top places, otherwise Threshold_Only. Decide before the request vocabulary is frozen. If UD-34 is still open, record Threshold_Only as the default and re-key the authored script if it changes (RN-27). | Request builder vocabulary (8.5), authored script (8.17), Rank_Reference work (9.3, 9.6, 9.15), live recording (10.8) | Before 8.5 |
| 1.12 | UD-33: packaging of the Application_Layers | Option B: two Demonstration_Executables `prize-check` and `bracket-check`, one package each. Option C (libraries only) is the fallback. | Application packages (9.5 onward) | After Slice 1 |
| 1.13 | UD-35: demonstration data and Published_Results | Real published values where they exist and are licensed, otherwise synthetic and labelled, with one equal and one seeded-difference Published_Result | Demonstration data (11.2) | After Slice 1 |

User actions that are not decisions: the live recording of provider responses (10.8) and the human review decisions on the real documents (11.4). Both are gate tasks that stop for the user.

Not gated: DD-2 (whether a Precedence_Declaration needs its own approval, RN-3) is not needed by the first version. It affects only a steering wording (design §9.3 item 5), which task 8.3 leaves untouched until the user decides.

## Critical path to the first end-to-end run

The first end-to-end run is the `hand` profile acceptance test of task 6.9 (Slice 0 demonstration). The path, in order:

1. 1.1 Decision gate UD-15 and DD-3 (toolchain version, runner labels, File::try_lock)
2. 2.1 Create the Cargo workspace skeleton
3. 2.4 Implement rt-foundation identifier and location types
4. 2.5 Implement the diagnostic catalogue, Code and Diagnostic
5. 2.7 Implement the RT-JSON parser and canonical writer in rt-json
6. 2.9 Implement rt-xtask check-graph and check-tree
7. 2.10 Implement rt-xtask scan-sources, scan-api and check-decisions
8. 3.1 Define the representation-independent rt-numeric API
9. 3.2 Implement the confirmed representation and checked arithmetic
10. 3.3 Implement literal parsing and canonical serialization
11. 3.4 Implement rounding at declared points
12. 4.4 Implement rt-ir types
13. 4.5 Implement the Rule_IR parser and canonical printer
14. 4.6 Implement Rule_Content hash and Draft hash
15. 4.7 Implement rt-ledger parse, print and chain verification
16. 4.8 Implement status derivation, Effective revisions and Reviewer
17. 5.1 Implement rt-validate passes 1 to 8
18. 5.2 Implement conflict detection and precedence validation (pass 9)
19. 5.3 Implement the approval gate and candidate checks (passes 10 and 11)
20. 5.4 Implement the validate command
21. 5.13 Implement the compile and evaluate commands
22. 5.14 Write negative tests for approval gating
23. 5.27 Add the Slice 0 golden fixtures and the isolation and trace-verification fixture tests
24. 6.6 Author the stand-in demonstration for the hand profile
25. 6.7 Implement the Coverage_Statement generator for hand-authored rules
26. 6.8 Write property tests for Coverage_Statement partition and Proof_Report completeness
27. 6.9 Write the demonstration acceptance test for the hand profile

Gates 1.2 to 1.7 are answered at wave 0 in parallel with 1.1 and join this path where they are needed: 1.2 at 2.7, 1.3 at 3.1, 1.4 at 4.7, 1.5 at 5.1, 1.6 at 5.4 and 1.7 at 6.1. Tasks off the path (unit and property tests, fixtures, other crates) run in the same waves. The first end-to-end run is reached at wave 26 of the graph below, and the Slice 0 CI jobs at wave 27.

Phase 5 and 6 work (groups 7 and 8) is not on this path. Phase 7 does not depend on Phases 5 and 6 (design §7.5), so the user may ask to run it before the AI slice. The graph below runs the AI slice first, so that Slice 1 is a demonstration of its own.

## Tasks

- [x] 1. Decision gates
  - [x] 1.1 Decision gate UD-15 and DD-3 (toolchain version, runner labels, `File::try_lock`)
    - Create `tools/decisions/UD-15.md` with `status: Pending`. Record the recommended runner labels, a proposed full `major.minor.patch` toolchain version, and whether `File::try_lock` (stable since Rust 1.89) is accepted. Say what the fallback is if it is not.
    - Ask the user with the user_input tool, then stop. Set `status: Confirmed` only after the answer arrives. The `live-openai` secret part of UD-15 is confirmed later, in task 10.5.
    - Observable: `tools/decisions/UD-15.md` is `Confirmed` and names the toolchain and the labels.
    - Blocked by: none.
    - _Requirements: 21.10, 21.12 (UD-15)_
    - Design §3.6.3, §7.4, §9.2 (UD-15, DD-3), §9.1 (R-D4)

  - [x] 1.2 Decision gate UD-2 and UD-9 (serialization format and size limits)
    - Create `tools/decisions/UD-2.md` with the comparison of design §4.1.1 (RT-JSON, serde_json, TOML, RON, custom text) and the default limits table of §4.1.5. Ask the user to confirm RT-JSON and the limits, or to choose another option, then stop.
    - `rt-json` is registered against UD-2 in task 2.1. If the user picks another format, record it and revise task 2.7 before it starts.
    - Observable: record `Confirmed`, with the chosen format and limits.
    - Blocked by: none.
    - _Requirements: 4.2, 4.4, 19.3, 19.4, 19.9 (UD-2, UD-9)_
    - Design §4.1.1, §4.1.5, §9.2

  - [x] 1.3 Decision gate UD-3 (exact numeric representation)
    - Create `tools/decisions/UD-3.md` with the comparison of design §4.2.1 across bounded decimal D, bounded rational R-128, bounded rational R-Big and hybrid H on exactness, range and scale, rounding, overflow protection, canonical serialization, and dependency and performance cost. Include the §4.2.7 table of criteria that differ and the §4.2.8 schema points.
    - State the recommendation (R-128) and its accepted costs. Do not state it as decided. Ask the user to confirm the documented comparison and select the representation, then stop. Record the choice and the `ExactNumber_Limits` constants.
    - Observable: record `Confirmed` with the bounds. Steering item 3 of design §9.3 is noted as a follow-up for task 8.3.
    - Blocked by: none.
    - _Requirements: 8.1, 8.4, 8.5, 8.7, 8.11, 8.16, 8.19, 8.27 (UD-3, UD-28)_
    - Design §4.2.1 to §4.2.8, §9.2

  - [x] 1.4 Decision gate UD-27 (ledger format and atomic commit)
    - Create `tools/decisions/UD-27.md` with the format comparison of design §3.8.1 and the commit-protocol comparison of §3.8.3. Note that the format is used from Phase 2, the one-file append from Phase 4, and the journaled two-file commit from Phase 5.
    - Ask the user to confirm, then stop. `rt-ledger` and `rt-review` are registered against UD-27 in task 2.1.
    - Observable: record `Confirmed`.
    - Blocked by: none.
    - _Requirements: 26.1, 26.19 (UD-27)_
    - Design §3.8.1, §3.8.3, §9.2

  - [x] 1.5 Decision gate UD-14 (exit-code classification)
    - Create `tools/decisions/UD-14.md` with the §6.1 classification, including exit 2 for `RT-VAL-0017` and `RT-VAL-0018` and the split of RN-10. Ask the user to confirm, then stop.
    - Observable: record `Confirmed`.
    - Blocked by: none.
    - _Requirements: 16.8, 16.10, 16.15 (UD-14)_
    - Design §6.1, §9.2, §9.4 (RN-10)

  - [x] 1.6 Decision gate RN-29 (artifact of `validate` when blocking Diagnostics exist)
    - Create `tools/decisions/RN-29.md` that states the conflict: Req 17.4 and 16.9 say a failed command creates no file at its output location, while Req 33.17 needs `compile` to exit 3 after `validate` reported `APPROVAL_UNRESOLVED`, and Req 9.2 has `compile` return blocking Diagnostics unchanged.
    - Give both options with their cost: (a) `validate` writes the Validation_Result when only blocking Diagnostics prevent success, and the requirement exception lists are amended; (b) change 33.17 so that `compile` exits 2 when no artifact exists. Recommend (a). Ask the user, then stop. If (a) is chosen, record the exact requirement amendments to apply.
    - Observable: record `Confirmed` with the chosen option.
    - Blocked by: none.
    - _Requirements: 17.4, 16.9, 9.2, 33.17 (RN-29)_
    - Design §3.9.1, §8.11.11, §9.4 (RN-29)

  - [x] 1.7 Decision gate UD-23 (Reviewer identity check)
    - Create `tools/decisions/UD-23.md`. Record the recommended exact comparison with the identities in Origin and the batch, the hardening option (compare with all identities in the store), and the residual risk R-8. Ask the user to confirm, then stop.
    - Observable: record `Confirmed`.
    - Blocked by: none.
    - _Requirements: 23.15, 26.17 (UD-23)_
    - Design §3.8.3, §9.2

  - [x] 1.8 Decision gate UD-36 (capture of the recorded provider response)
    - Create `tools/decisions/UD-36.md` with the three capture options of design §8.11.7 and the recommended option 1 (the Proposal_Store retains the accepted response; `rt-xtask export-script` writes the script). Record the wording change RN-27 for Req 30.1 and 23.25. Ask the user, then stop.
    - Observable: record `Confirmed`. Task 7.1 reads the chosen option.
    - Blocked by: Slice 0 complete (6.9).
    - _Requirements: 30.1, 33.8, 23.25 (UD-36)_
    - Design §3.7.5, §8.11.7, §9.4 (RN-27)

  - [x] 1.9 Decision gate UD-26 (exit code with blocking semantic outcomes)
    - Create `tools/decisions/UD-26.md` with the recommendation (exit 0 with the `RT-AI-0305` Warning when a stored proposal carries blocking semantic findings) and the alternative. Ask the user, then stop.
    - Observable: record `Confirmed`.
    - Blocked by: Slice 0 complete (6.9).
    - _Requirements: 23.35, 23.36, 30.2, 27.8 (UD-26)_
    - Design §6.1, §9.2

  - [x] 1.10 Decision gate UD-34 (demonstration documents)
    - Create `tools/decisions/UD-34.md` with the selection criteria of Req 33.3 and design §8.11.1. Do not name or assume any document. Ask the user to supply the documents (or state that the stand-in stays for now), the retrieval text, the licence or permission, and whether transmission to the provider is permitted. If a document is PDF-only, ask for the manual conversion record of Req 33.5. Stop.
    - Observable: record `Confirmed` with the user's references, or `Deferred` with the stand-in named. Only `Confirmed` unblocks tasks 10.8 and 11.1.
    - Blocked by: Slice 0 complete (6.9).
    - _Requirements: 33.1, 33.2, 33.3, 33.4, 33.5, 28.21 (UD-34)_
    - Design §8.11.1, §8.11.3, §9.2

  - [x] 1.11 Decision gate UD-32 (rank-based qualification: Threshold_Only or Rank_Reference)
    - Create `tools/decisions/UD-32.md` with both options of design §8.9 and their effect on the request `vocabulary`, the Rule_IR (one added operation under Rank_Reference, Req 14.12), the Bracket_Check report (Req 32.3, 32.4) and the Request_Identifier (RN-27). Recommend by the rule in the decision table above. Ask the user, then stop.
    - Observable: record `Confirmed` with the selected option. The request builder (8.5) reads the option.
    - Blocked by: 1.10.
    - _Requirements: 14.12, 14.13, 32.3, 32.4 (UD-32)_
    - Design §8.9, §9.2, §9.4 (RN-27)

  - [x] 1.12 Decision gate UD-33 (packaging of the Application_Layers)
    - Create `tools/decisions/UD-33.md` with options A, B and C of design §8.2. Recommend B (two small Demonstration_Executables, one package each). Record that choosing C turns both packages into library-only packages with `examples/` targets and reverts the executable-set test of Req 27.15 to two names. Ask the user, then stop. `prize-check` and `bracket-check` are registered against UD-33 in task 2.1.
    - Observable: record `Confirmed`.
    - Blocked by: Slice 1 complete (8.18).
    - _Requirements: 27.1, 27.15, 31.3 (UD-33)_
    - Design §8.2, §9.2

  - [x] 1.13 Decision gate UD-35 (demonstration data and Published_Results)
    - Create `tools/decisions/UD-35.md` with the recommendation (real published values where licensed, otherwise synthetic and labelled, with one equal and one seeded-difference Published_Result) and the `nature` manifest field of design §8.11.3. Ask the user which data is real and which is synthetic, then stop.
    - Observable: record `Confirmed`.
    - Blocked by: Slice 1 complete (8.18).
    - _Requirements: 33.13 (UD-35)_
    - Design §8.11.10, §9.2


- [ ] 2. Phase 0: Foundation (workspace, CI skeleton, hash, foundation types, diagnostics, RT-JSON, xtask checks)
  - [x] 2.1 Create the Cargo workspace skeleton
    - Create root `Cargo.toml` with `members = ["crates/*"]` and `rust-toolchain.toml` with the version confirmed in 1.1. Add `rustfmt.toml`, a workspace clippy configuration (deny `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` in library crates; `disallowed-macros` for AI crates), and `.gitattributes` with `* -text`.
    - Create crate skeletons for the crates that Phase 0 needs: `rt-hash`, `rt-foundation`, `rt-json`, `rt-xtask`, `rt-tests-core`. Each has `#![forbid(unsafe_code)]`, edition 2021, and the class recorded in `tools/allowed-edges.v1`.
    - Create the `tools/` files `allowed-edges.v1`, `scan-allow.v1`, `dependencies.v1`, `contracts.v1`, `decision-gates.v1` and the directories `tools/decisions/`, `catalogue/`, `tests/fixtures/`, `docs/`. Write `decision-gates.v1` once for every gate that ties a crate to a decision: UD-2 (`rt-json`), UD-3 (`rt-numeric`), UD-27 (`rt-ledger`, `rt-review`), UD-33 (`prize-check`, `bracket-check`) and UD-36 (`rt-proposals`). A missing record counts as Pending. Later-phase crates are created only when their phase starts, and no later task edits `decision-gates.v1`.
    - Observable: `cargo build --workspace` and `cargo test --workspace` succeed on an empty test set.
    - Blocked by: 1.1.
    - _Requirements: 21.1, 21.2, 19.7, 24.5_
    - Design §3.1, §3.3

  - [x] 2.2 Create the CI workflow skeleton on three platforms
    - Create `.github/workflows/ci.yml` with the jobs `fmt`, `clippy`, `test`, `golden`, `toolchain` on `ubuntu-24.04`, `macos-15` and `windows-2022` (labels from 1.1). Add `graph`, `deny` and `core-only` on Linux as stubs that later tasks fill in. The change fails if any platform is missing, skipped or cancelled. Failed-check output is kept as an artifact.
    - Add `deny.toml` with `advisories`, `bans` and `licences` sections. The network and AI crate bans are added in task 10.2.
    - Observable: the workflow file parses, and each job has the three-platform matrix or the Linux runner named in design §7.4. A red platform blocks merging.
    - Blocked by: 2.1.
    - _Requirements: 21.3, 21.4, 21.7, 21.10, 21.12_
    - Design §7.4

  - [x] 2.3 Implement `rt-hash`
    - Implement `Sha256Hex` and `sha256(bytes)` as a thin wrapper over `sha2` (default features off). Add the `sha2` row to `tools/dependencies.v1` with purpose, alternatives and maintenance status.
    - Test with the NIST SHA-256 vectors: the empty string, `abc`, the 448-bit message and the one-million-`a` vector.
    - Observable: the vectors pass on the three platforms.
    - Blocked by: 2.1.
    - _Requirements: 1.3, 21.5, 21.6_
    - Design §3.2

  - [x] 2.4 Implement `rt-foundation` identifier and location types
    - Implement `DocumentId`, `InputId`, `RuleId` (`[a-z][a-z0-9_.-]{0,127}`, bytewise UTF-8 order), `RevisionNumber` (1 to 4,294,967,295), `RuleRef`, `ByteOffset`, `SourceSpan`, `InputLocation`, `CliLocation`, `NoLocationReason` and `Location`. A `Location::Source` can only be built from a Source_Model or a verified Rule.
    - Implement `Severity` with `is_blocking`, the limits constants of design §4.1.5, and the single `escape_controls` function that escapes every Unicode `Cc` character except the final line feed.
    - Observable: unit tests reject invalid identifiers and out-of-range revisions, and no `Location::Source` can be constructed without a span.
    - Blocked by: 2.1.
    - _Requirements: 7.14, 7.15, 19.5, 19.10, 25.1_
    - Design §3.4.1, §4.1.5, §6.2.1

  - [x] 2.5 Implement the diagnostic catalogue, `Code` and `Diagnostic`
    - Write `catalogue/diagnostics.v1.json` with every row of design §4.3 (`code`, `mnemonic`, `severity`, `meaning`, `exit`, `executable`), including the APP and `RANK_REFERENCE_CYCLE` rows. Each failure mode has exactly one code, and each mnemonic maps to one identifier.
    - Implement `rt-xtask gen-catalogue`, which writes the checked-in generated `Code` enum into `rt-foundation`, and `Code::id`, `mnemonic`, `severity`, `exit_code`.
    - Implement `Diagnostic::new(code, message, primary)` so that severity comes only from the code, plus `with_related`, `with_rules`, accessors, and the total `Ord` of Req 7.10 and 7.22 (bytewise rule ids).
    - Observable: a severity cannot be overridden by any public constructor, and catalogue round-trip tests pass.
    - Blocked by: 2.4.
    - _Requirements: 7.1, 7.10, 7.16, 7.17, 7.18, 7.22, 7.23_
    - Design §3.4.1, §4.3

  - [x] 2.6 Implement `DiagnosticState` and the staged result types
    - Implement `DiagnosticState` as a private ordered `Vec<Diagnostic>` with `empty`, `push`, `extend`, `sorted`, `as_slice`, `has_errors`, `no_errors`, `outcome` and `has_unresolved_approval`. No constructor, field or method takes `has_errors` or `no_errors` as input, and nothing stores them.
    - Implement `Staged<T>`, `Rejected` and `StageResult<T>`. A Warning never changes the derived flags, the outcome or compilability.
    - Observable: the crate exposes no item that takes or returns the two flags (checked by 2.10).
    - Blocked by: 2.5.
    - _Requirements: 22.1, 22.2, 22.3, 22.4, 22.5, 22.8, 22.14, 22.18, 22.19, 7.19_
    - Design §3.4.2

  - [x] 2.7 Implement the RT-JSON parser and canonical writer in `rt-json`
    - Implement the strict positioned parser: UTF-8 only, no BOM, no comments, no duplicate keys, depth counter checked before building values, size limit checked before parsing, integers only as non-negative values up to 9,007,199,254,740,991, no unescaped controls, no unpaired surrogates. Every value carries byte, line and column.
    - Implement the compact writer and the pretty writer (two-space indent, trailing LF) with fixed key order and string escaping. Writers are iterative where depth can be large.
    - Implement the shared flag-key decoder check: a key named `has_errors`, `no_errors`, `validation_status`, `validation_result` or `passed` gives exactly one `RT-DGN-0001` (one flag) or `RT-DGN-0002` (both), and a lifecycle key (`approval_status`, `is_approved`, `is_rejected`, `is_proposed`, `approval_record`) gives `RT-DGN-0003`, whether the value agrees or not. The Diagnostic names every offending field and no value is returned.
    - Observable: documents at the size and depth limit parse, and one above the limit gives the limit Diagnostic with its position.
    - Blocked by: 1.2, 2.3, 2.5.
    - _Requirements: 4.2, 4.3, 4.4, 4.10, 19.3, 19.4, 19.6, 19.9, 22.7, 22.9, 22.10, 22.12_
    - Design §4.1.1, §3.4.2, §6.2.1

  - [x] 2.8 Write unit and differential tests for RT-JSON
    - Unit tests for RFC 8259 examples, each restriction of the dialect, positions of the first error, and limits at and one above the bound. Differential test against `serde_json` as a dev-only oracle on generated valid and invalid documents (design risk R-D3). Record `serde_json` as dev-only in `tools/dependencies.v1`.
    - Observable: no disagreement on generated documents. Moved here as breadth; the no-panic runs of task 12.1 stay required for release.
    - _Requirements: 4.2, 19.4, 19.9, 20.1_
    - Design §9.1 (R-D3)

  - [x] 2.9 Implement `rt-xtask check-graph` and `check-tree`
    - Parse `cargo metadata --format-version 1 --all-features` with `rt-json`. Implement Tarjan cycle detection, set difference against `tools/allowed-edges.v1`, the class rule (core never depends on aux), the `forbid-name` rules, the stage-order rule and the `forbid-external` list of network and AI crates.
    - Implement `check-tree`, which resolves the full tree (normal, build, dev, all features) of `rt-cli` and of each core crate and fails naming the path to any network or AI crate. Write `tools/allowed-edges.v1` once for every crate of design §3.1 and §3.3, including the forbidden edges for the AI crates and the application packages (the check ignores crates that are not yet workspace members), so that later tasks that create a crate do not edit it. A missing or malformed list is its own failure.
    - Add negative tests on synthetic metadata: a cycle, a forbidden edge, an AI crate depending on `rt-review`, a core crate depending on `ureq`, a missing edge list. Each must fail naming crate and edge. These tests are required.
    - Observable: `cargo run -p rt-xtask -- check-graph check-tree` passes on the workspace and each negative fixture fails with the expected message.
    - Blocked by: 2.7.
    - _Requirements: 21.8, 21.13, 24.5, 24.9, 24.10, 20.17, 27.5_
    - Design §3.3

  - [x] 2.10 Implement `rt-xtask scan-sources`, `scan-api` and `check-decisions`
    - `scan-sources`: a token scan (no regex crate) of core crates for `static mut`, `thread_local!`, `lazy_static`, `OnceLock`, `OnceCell`, `std::env`, `std::fs`, `std::net`, `SystemTime`, `Instant`, `rand`, `f32`, `f64`, `unsafe`, `Command`, `libloading`, `HashMap` and `HashSet`, plus prize or bracket terms in the engine. It checks `forbid(unsafe_code)` per crate and honours `tools/scan-allow.v1` entries that carry a reason.
    - `scan-api`: compare public stage signatures with `tools/contracts.v1`, and fail if any public item takes or returns `has_errors` or `no_errors`.
    - `check-decisions`: read `tools/decision-gates.v1` and `tools/decisions/*.md`, and fail if a gated crate is a workspace member while its decision is `Pending`.
    - Negative tests: snippets with `static mut`, `thread_local!`, `std::env::var`, `f64`, `HashMap` in a core crate, and a gated crate with a pending decision. Each fails naming crate and item. These tests are required.
    - Observable: the three commands pass on the workspace, and the negative fixtures fail as specified.
    - Blocked by: 2.9.
    - _Requirements: 10.7, 10.10, 15.1, 19.7, 20.17, 24.2, 24.6, 24.7, 8.2_
    - Design §3.3, §7.3

  - [x] 2.11 Implement `rt-xtask check-catalogue` and `check-justification`
    - `check-catalogue`: every `Code` variant appears in `catalogue/diagnostics.v1.json`, the generated enum equals a fresh generation, and once a baseline exists, a removed, renumbered or severity-changed code fails.
    - `check-justification`: every external crate in `Cargo.lock` that is absent from `tools/dependencies.v1`, or lacks purpose, alternatives or maintenance status, fails.
    - Add the `graph` job steps of design §7.4 to `ci.yml`.
    - Observable: both commands pass, and a hand-edited generated enum fails the freshness check.
    - Blocked by: 2.5, 2.10.
    - _Requirements: 7.11, 7.12, 21.5, 21.6_
    - Design §3.3

  - [x] 2.12 Write property test for Diagnostic_State flags
    - **Property 4: Diagnostic_State flags are derived and consistent**
    - Use `gen_diagnostics` (every Severity and Location variant, empty and Warning-only lists), at least 256 cases, tagged `Feature: ruletrace-compiler, Property 4: Diagnostic_State flags are derived and consistent`.
    - Observable: the test passes, and a mutation that stores a flag makes it fail.
    - **Validates: Requirements 22.1, 22.2, 22.3, 22.4, 22.5, 22.14, 22.19, 22.20, 25.20, 6.26**

  - [x] 2.13 Write property test for Diagnostic ordering
    - **Property 7: Diagnostic canonical order is total and permutation independent**
    - Required because byte-identical output depends on one canonical order.
    - Observable: any permutation of a generated list sorts to identical bytes.
    - **Validates: Requirements 7.10, 7.22, 6.12, 17.12**

  - [x] 2.14 Checkpoint - Phase 0 exit
    - Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, and `rt-xtask check-graph check-tree scan-sources scan-api check-catalogue check-justification check-decisions`.
    - Exit criteria: Properties 4 and 7 pass; graph and scan negative fixtures pass; the catalogue generator and its freshness check pass; CI is green on three platforms.
    - Ensure all tests pass, ask the user if questions arise.

- [x] 3. Phase 1: Exact numeric representation (`rt-numeric`)
  - [x] 3.1 Define the representation-independent `rt-numeric` API
    - Define the opaque `ExactNumber`, `NumericError` (`Overflow`, `DivisionByZero`, `InvalidLiteral`, `InvalidPlaces`), `RoundingMode` (`half_even`, `half_away_from_zero`, `toward_zero`), and a `limits` module whose constants come from the record of 1.3. Declare `add`, `sub`, `mul`, `div`, `cmp`, `neg`, `sum`, `min`, `max`, `count`, `round`, `parse_literal`, `to_canonical`, `parse_canonical`.
    - Floats are banned, and the crate is std-only. `rt-numeric` is already registered against UD-3 (task 2.1).
    - Observable: the crate compiles with `check-decisions` green, and no public item mentions a float.
    - Blocked by: 1.3, 2.10.
    - _Requirements: 8.1, 8.2, 8.3, 8.12, 8.17_
    - Design §4.2.2, §4.2.8

  - [x] 3.2 Implement the confirmed representation and checked arithmetic
    - Implement only the variant recorded in `tools/decisions/UD-3.md`. Every operation is checked and returns `Result`. Overflow is an error, never a wrap or saturation. Keep invariants: normalized, one zero, structural equality equals value equality.
    - If R-128: sign-and-magnitude `u128` numerator and denominator at most 2^127 - 1, a private 256-bit wide helper, binary gcd, cross-cancelling multiply, and the algorithms of design §4.2.3. If D, R-Big or H: follow the sub-option in the record and add its dependency justification first.
    - Observable: table tests at and one beyond the overflow bound, and divide by zero, pass.
    - Blocked by: 3.1.
    - _Requirements: 8.1, 8.5, 8.8, 8.9, 8.11, 8.12, 8.17, 8.18, 8.21, 8.22, 8.23_
    - Design §4.2.2, §4.2.3, §4.2.7

  - [x] 3.3 Implement literal parsing and canonical serialization
    - Literals: an optionally negative decimal as a string, at most 38 digits and 28 fractional digits, no exponent, no leading plus, no `NaN`, `Infinity` or fraction. `-0` and `0.00` give zero. Limits are identical under every option.
    - Canonical output and the artifact parser follow the confirmed variant (design §4.2.4): a terminating decimal else `n/d` through the bounded digit-vector codec, or one decimal string for D. Apply RN-8 only if the user chose it.
    - Observable: accept and reject tables pass for `1.5`, `2`, `0.3`, `1/3`, `-2/3`, and the 126-digit expansion when the variant allows it.
    - Blocked by: 3.2.
    - _Requirements: 3.6, 4.4, 8.7, 8.16, 8.24, 8.25, 8.27_
    - Design §4.2.4

  - [x] 3.4 Implement rounding at declared points
    - Implement `round(places 0..28, mode)` for `half_even`, `half_away_from_zero` and `toward_zero`. Rounding happens only at a `round` node or a `rounding` field. Out-of-range places and rounding overflow are reported.
    - Observable: the tables of Req 8.28 pass (2.5 to 0 places gives 2, 3, 2; -2.5 gives -2, -3, -2; 3.5 gives 4, 4, 3; 0.125 to 2 places gives 0.12, 0.13, 0.12). These tests are required.
    - Blocked by: 3.2.
    - _Requirements: 8.3, 8.4, 8.14, 8.28_
    - Design §4.2.3, §4.2.5

  - [x] 3.5 Build the differential reference model and numeric test harness
    - In `rt-tests-core`, implement an independent exact-rational reference model with `num-bigint` as a dev-only dependency (record it in `tools/dependencies.v1`). Cover arithmetic, comparison, rounding, decimal and fraction printing and parsing, and aggregation, with every bound taken from the confirmed `limits`.
    - Create the test-only `rt-numeric-candidate-d` crate only if the UD-3 record says both candidates must run until a later confirmation.
    - Observable: the reference model agrees with `rt-numeric` on a fixed seed corpus.
    - Blocked by: 3.3, 3.4.
    - _Requirements: 20.22, 8.11_
    - Design §7.3, §9.4 (RN-20)

  - [x] 3.6 Write property test for exact arithmetic laws and differential equivalence
    - **Property 17: Exact arithmetic laws and differential equivalence**
    - Generate values at, near and beyond the bound, 256 cases per operation. Check the laws, equality by value, one zero form, and `Overflow` exactly when the exact normalized result is outside the limits. Required.
    - Observable: no disagreement with the reference model.
    - **Validates: Requirements 8.1, 8.8, 8.9, 8.11, 8.12, 8.17, 8.18, 8.22, 8.23, 20.22**

  - [x] 3.7 Write property test for numeric serialization and literal limits
    - **Property 18: Numeric serialization round trip and literal limits**
    - Required.
    - Observable: serialize-then-parse equals the original for every generated value, and every out-of-limit literal is rejected.
    - **Validates: Requirements 8.16, 8.24, 8.25, 8.27, 3.6, 4.4, 8.7**

  - [x] 3.8 Write property test for rounding at declared points only
    - **Property 19: Rounding at declared points only**
    - Required.
    - Observable: no operation other than `round` changes the exact value of a result.
    - **Validates: Requirements 8.3, 8.4, 8.14, 8.28**

  - [x] 3.9 Checkpoint - Phase 1 exit
    - Run the full gate set. Confirm `rt-numeric` has no dependency, no float type and no `unsafe`.
    - Exit criteria: Properties 17 to 19 pass at the bounds; the Req 8.28 tables pass; the UD-3 record is `Confirmed`.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 4. Phase 2: Source, Rule_IR and ledger types (`rt-srcmodel`, `rt-source`, `rt-ir`, `rt-ledger`, `ingest`)
  - [x] 4.1 Implement `rt-srcmodel` types and verification helpers
    - Implement the Source_Model and Candidate_Clause types, the canonical Source_Model serialization (full text embedded), the loader that checks SHA-256 and byte length (`RT-PRV-0005`), and the pure span and hash verification helpers. The crate does not depend on the Ingestor or Extractor.
    - Observable: a Source_Model with one altered byte fails to load with `RT-PRV-0005`, and the helper resolves a span to the exact bytes.
    - Blocked by: 2.7.
    - _Requirements: 1.1, 1.3, 1.7, 13.15, 24.12_
    - Design §3.1, §3.5

  - [x] 4.2 Implement `rt-source` ingestion
    - Implement `ingest(bytes, id, format_hint, limit)` with the check order: size limit (`RT-LIM-0001` or `RT-ING-0003`), format and magic bytes (`RT-ING-0002`), UTF-8 (`RT-ING-0001`, first invalid offset), then structure. Implement the Markdown and plain-text mapping of design §3.5 (headings, paragraphs, list items, fenced code, pipe tables, setext and HTML as paragraphs), byte-offset spans, BOM handling, LF, CRLF and CR endings, and 1-based line and column in Unicode scalar values.
    - Observable: ingesting the same bytes twice gives identical Source_Model bytes, and each failure gives its one code.
    - Blocked by: 4.1.
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.7, 1.9, 1.10_
    - Design §3.5

  - [x] 4.3 Implement clause extraction and the candidates file
    - Implement `extract(model, ExtractionMode::first_version)`: one Candidate_Clause per list item, and per heading or paragraph that starts with a numbering label (`\d+(\.\d+)*[.)]?`, `\([a-z]{1,2}\)`, `\([ivxlc]+\)`, `[A-Z][.)]`) followed by whitespace; table rows only when the first cell has a label. Text is the exact span bytes, ordered by start offset. Zero clauses gives the `RT-ING-0005` Warning. Implement the candidates file print and parse.
    - Observable: a document with three labelled clauses gives three Candidate_Clauses in ascending offset order with exact text.
    - Blocked by: 4.2.
    - _Requirements: 2.1, 2.2, 2.4, 2.6, 2.8_
    - Design §3.5 (Extraction)

  - [x] 4.4 Implement `rt-ir` types
    - Implement `Rulebook`, `Rule`, `RuleRevision`, `Origin`, the five rule kinds, the expression AST of design §4.1.4 with declared types, `Span`, markers (`ambiguous`, `incomplete`), `precedence`, types and `documents`. Exact numbers use `rt-numeric` only. Unrecognized well-formed kinds and nodes are carried as opaque subtrees for the Validator. `ApprovalStatus` is derived only: no stored field, no deserialization.
    - Observable: the crate has no way to construct an approval status from data (checked by `scan-api`).
    - Blocked by: 3.9, 4.1.
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.14, 14.1, 14.3, 25.1_
    - Design §3.4.4, §4.1.2 to §4.1.5

  - [x] 4.5 Implement the Rule_IR parser and canonical printer
    - Implement `parse_rulebook(bytes, input, size, depth)`: strict RT-JSON, `ir_version` `"1"` (`RT-IR-0003` lists supported versions, `RT-IR-0006` missing or repeated), flag and lifecycle keys rejected before the Unsupported check, other unknown names as `RT-IR-0002` with line and column, a bare JSON number where an Exact_Number is expected as `RT-IR-0004`. Return every parse Diagnostic in canonical order and no Rulebook on error.
    - Implement compact and pretty printers with the key order of design §4.1.2, rules sorted by (id bytes, revision), and byte-identical repeated output. Enforce the rule-count, revision-count and span-count limits (`RT-LIM-0004`).
    - Observable: parse then print of the worked example of design §4.1.7 reproduces its bytes.
    - Blocked by: 4.4.
    - _Requirements: 3.8, 3.10, 3.11, 3.15, 3.17, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9, 4.10, 4.11_
    - Design §4.1.1, §4.1.2

  - [x] 4.6 Implement Rule_Content hash and Draft hash
    - Compute the Rule_Content_Hash over the canonical compact bytes of `ir_version`, `id`, `revision`, `kind`, `target`, `type`, body, `markers`, and `spans` with `document`, `sha256`, `start` and `end`. Exclude `origin`, approval data, span `line` and `column`, and derived values. Compute the Draft hash with the same rules without `revision`.
    - Observable: the worked example of design §4.1.7 gives Rule_Content_Hash `d1b4158c4852a5cd3d15a8d1226143461defc7c238eea3f84fef425a5d94fe34` for rule `elig.basic`, and document hash `eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a`. This is the first golden test.
    - Blocked by: 4.5.
    - _Requirements: 25.3, 25.4, 25.5, 25.6, 25.8, 3.13_
    - Design §3.8.2, §4.1.7

  - [x] 4.7 Implement `rt-ledger` parse, print and chain verification
    - Define `LedgerRecord`, the approval binding (rule, revision, content hash, IR_Version, sorted document id and hash pairs) and `ApprovalLedger`. Implement JSON Lines (header, then one canonical compact record per line) with `parse_ledger` verification in order: syntax and fields (`RT-APR-0101`), sequence with no gap (`0103`), hash chain (`0102`), reviewer validity (`0105`), contradictions (`0104`). Stop at the first failing record. Any failure is invalid input and no status is derived. A header-only ledger is valid and empty. Flag and lifecycle keys give the RT-DGN codes. `time` is checked for shape only.
    - The crate exposes no function that appends or writes a record. Its line is already in `tools/allowed-edges.v1` (task 2.9). Make `scan-api` fail if a `pub fn` in `rt-ledger` creates a record.
    - Observable: a ledger with one changed byte fails with the first failing record's code and position.
    - Blocked by: 1.4, 4.6.
    - _Requirements: 26.1, 26.2, 26.3, 26.4, 26.6, 26.18, 26.19, 26.20, 26.21, 26.23, 22.17, 24.5_
    - Design §3.1 (rationale), §3.8.1

  - [x] 4.8 Implement status derivation, Effective revisions and `Reviewer`
    - In `rt-ledger`, implement `derive_statuses`, `effective_revisions` and `unresolved` as pure functions over a Rulebook and a verified ledger (Rejected if a rejection names the key, else Approved only if rule, revision, recomputed content hash, IR_Version and every binding pair match, else Proposed). No fallback to a lower revision. Implement `Reviewer::new` (1 to 200 scalar values) and `check_not_ai` against an AI identity set.
    - Observable: changing one span offset, one document hash or the IR_Version of an approved revision makes it Proposed again.
    - Blocked by: 4.7.
    - _Requirements: 5.1, 5.3, 5.16, 25.11, 26.10, 26.11, 26.17_
    - Design §3.4.4, §3.8.2

  - [x] 4.9 Create `rt-cli` skeleton and the `ingest` command
    - Create `rt-cli` (binary `ruletrace`) with the hand-written argument parser returning `Cli_Location` Diagnostics, the `Fs` trait with `StdFs`, atomic output (write temp, sync, rename), exit codes 0, 2 and 70 and the earliest-failing-stage order of design §6.1. Implement `ruletrace ingest --document --document-id --format --out [--candidates] [--max-bytes]`. Output exists only on success, and existing output files stay byte-identical on failure.
    - Observable: `ruletrace ingest` on the stand-in text writes a Source_Model and candidates file, exit 0. A missing input exits 2 with a Diagnostic and no output file.
    - Blocked by: 4.3, 4.8.
    - _Requirements: 17.1, 17.2, 17.3, 17.4, 17.13, 16.1, 1.8_
    - Design §3.9.1, §6.1, §6.2.4

  - [x] 4.10 Write property test for source span reproduction
    - **Property 1: Source span reproduction and ordering**
    - Use `gen_document` with LF, CRLF and CR endings, tabs and non-ASCII text. Required because it protects provenance.
    - Observable: every clause text equals the bytes of its span.
    - **Validates: Requirements 1.1, 1.2, 1.3, 1.4**

  - [x] 4.11 Write property test for Rulebook serialization round trip
    - **Property 3: Rulebook serialization round trip**
    - Breadth: the worked-example round trip of 4.5 covers Slice 0.
    - Observable: parse-print-parse equality for every generated Rulebook.
    - **Validates: Requirements 3.10, 3.11, 4.5, 4.6, 25.18**

  - [x] 4.12 Write property test for contradictory flag input (Rulebook and ledger)
    - **Property 5: Contradictory flag input is rejected**
    - Insert each flag and lifecycle key at any position with any value into a generated Rulebook and a generated ledger. Assert exactly one Error with `RT-DGN-0001`, `0002` or `0003` naming every offending field, no value returned, and no later stage run. Later tasks (7.8, 9.12) extend the same generator to the other input kinds. Required.
    - Observable: exit code 2 for every case through the CLI.
    - **Validates: Requirements 4.10, 22.6, 22.7, 22.9, 22.10, 22.11, 22.12, 22.13, 22.16, 22.17, 22.18, 20.25**

  - [x] 4.13 Write property test for Rule_Content_Hash sensitivity
    - **Property 12: Rule_Content_Hash sensitivity**
    - Change exactly one part: a span offset, a document hash, an operand, a rounding declaration, a marker, the revision, the IR_Version. Required because it protects the approval gate.
    - Observable: every single change alters the hash.
    - **Validates: Requirements 20.15, 25.4, 25.5, 25.6, 25.8, 25.22, 26.9, 5.6, 5.15**

  - [x] 4.14 Write property test for Rule_Content_Hash invariance
    - **Property 13: Rule_Content_Hash invariance**
    - Origin, approval data, line and column, and key order must not change the hash. Task 5.18 extends it to compiled-artifact bytes. Required.
    - Observable: the hash is stable under every non-content change.
    - **Validates: Requirements 3.13, 9.5, 18.5, 25.3, 25.9, 16.18**

  - [x] 4.15 Write property test for approval status derivation
    - **Property 9: Approval status is a pure function of Rulebook and ledger**
    - Required because it protects the approval gate.
    - Observable: the same Rulebook and ledger always give the same statuses, and no other input changes them.
    - **Validates: Requirements 5.1, 5.3, 5.16, 25.11, 26.10, 26.11**

  - [x] 4.16 Write property test for ledger round trip and tamper evidence
    - **Property 14: Ledger round trip and tamper evidence**
    - Required because it protects the approval gate.
    - Observable: any one-byte change, removal, reordering or insertion in a generated ledger is detected with the specified code.
    - **Validates: Requirements 26.2, 26.3, 26.4, 26.20, 26.21, 26.23**

  - [x] 4.17 Add the ingest golden fixture and the golden-file harness
    - Create `tests/fixtures/slice-basic/source/` from the worked example (`rules.md`, 171 bytes) with `expected/source-model.json`. Add a golden-file comparison harness in `rt-tests-core` that compares bytes and names file and platform on failure. Fixtures use only public documents with recorded source and permission, and a test fails if a fixture contains a credential-looking string. Run `ruletrace ingest` through `std::process::Command` with a scrubbed environment.
    - Observable: the `golden` CI job is green on the three platforms.
    - Blocked by: 4.9.
    - _Requirements: 20.4, 21.11, 21.12, 28.11, 1.7_
    - Design §7.3

  - [x] 4.18 Checkpoint - Phase 2 exit
    - Run the full gate set. Confirm `rt-ledger` has no append function and that `check-decisions` passes.
    - Exit criteria: Properties 1, 5 (Rulebook and ledger), 9, 12, 13, 14 pass; the worked-example hash is reproduced; the ingest golden is byte stable on three platforms.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Phase 3: Validate, compile, evaluate and explain (`rt-validate`, `rt-compiled`, `rt-compile`, `rt-data`, `rt-trace`, `rt-engine`, `rt-explain`)
  - [x] 5.1 Implement `rt-validate` passes 1 to 8
    - Implement `ValidationResult` (private fields, accessors) and `validate_content(declarations, draft, sources)`. Passes: structure, sources, names, types, numeric declarations, boundary, markers, graph. Collect all findings and never short-circuit. A name-resolution failure gives the internal `Error` type so one defect yields one Diagnostic. Origin is not an argument of any check. Rejected and superseded revisions get structural checks only. A missing or mismatched Source_Model is invalid input (`RT-VAL-0017`, `0018`, exit 2 per UD-14).
    - Observable: a Rulebook with an unknown name, a type mismatch and a span outside its document gives exactly one Diagnostic per defect, with the specified codes and locations.
    - Blocked by: 1.5, 4.8.
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.10, 6.11, 6.24, 6.25, 14.6, 14.7, 18.5_
    - Design §3.6.1, §4.1.5

  - [x] 5.2 Implement conflict detection and precedence validation (pass 9)
    - Implement `DuplicateTarget`, `DuplicateRankKey`, `ContradictoryDefault` and `InputShadowing` among Effective revisions, naming every rule in ascending id order and selecting no winner. A precedence declaration listing every rule of a conflict in a total order resolves the first three classes with a Warning (`RT-VAL-0010`) and never resolves `InputShadowing`. A cycle or a non-Effective listed rule gives `RT-VAL-0012`. Record `checked_conflict_classes` in ascending order.
    - Observable: two rules with the same target give one conflict Diagnostic naming both, and no winner.
    - Blocked by: 5.1.
    - _Requirements: 6.7, 6.8, 6.16, 6.17, 6.18, 6.19, 6.20, 6.21, 6.23, 14.10_
    - Design §3.6.1

  - [x] 5.3 Implement the approval gate and candidate checks (passes 10 and 11)
    - `RT-APR-0001` per unresolved revision (primary location is the rule's first span, Severity Error), `RT-APR-0002` when no Effective revision exists, the stale-record Warning for any ledger record matching no current revision, the rejected-revision Warning, and `APPROVAL_REJECTED_REVISION_MODIFIED`. Rejected revisions and Pending_Proposals never block. A non-matching record never counts as an approval. Pass 11: a Warning for each unreferenced Candidate_Clause.
    - Observable: an unresolved revision gives `RT-APR-0001` and a failed validation result, and a rejected revision gives only a Warning.
    - Blocked by: 5.2.
    - _Requirements: 5.4, 5.5, 5.7, 5.14, 6.13, 6.14, 2.5, 25.13, 25.19, 26.12, 26.13, 26.28, 26.29_
    - Design §3.6.1 (Approval gating)

  - [x] 5.4 Implement the `validate` command
    - Implement `ruletrace validate --rulebook --ledger --source-model... [--candidates] --out [--output-format human|json] [--max-bytes] [--max-depth]`. Exit codes follow the UD-14 record: 2 for invalid input, 3 for blocking validation, 0 otherwise. Diagnostics go to stderr in canonical order. The artifact rule follows the RN-29 record: under option (a), write the Validation_Result also when only blocking Diagnostics prevent success, and record in the artifact that `compile` must refuse it.
    - Observable: on the slice fixture `validate` exits 0 and writes the artifact. On an unresolved revision it exits 3 and writes or omits the artifact exactly as the RN-29 record says.
    - Blocked by: 1.6, 5.3, 4.9.
    - _Requirements: 16.2, 16.8, 16.10, 17.1, 17.4, 17.5, 9.11_
    - Design §3.9.1, §6.1, §9.4 (RN-29)

  - [x] 5.5 Implement `rt-compiled` types and canonical serialization
    - Define `CompiledRulebook` with names resolved to indices, a node arena per rule, resolved types, spans with document hashes, evaluation order, precedence table, input schema and document hashes. It owns all of its data with no interior mutability and no shared state. Implement canonical compact serialization with the key order of design §4.1.6 and a parser that rejects flag, lifecycle and origin keys with the RT-DGN codes. No Origin, approval data or validity flag is stored.
    - Observable: serialize-parse-serialize gives identical bytes.
    - Blocked by: 4.6, 3.9.
    - _Requirements: 9.3, 9.4, 9.5, 9.7, 22.6_
    - Design §4.1.6

  - [x] 5.6 Implement `rt-data` (Competition_Data, Questions, Published_Result)
    - Implement strict parsers with the limits of design §4.1.5 (`RT-LIM-0004`), Exact_Number strings, `null` as missing, and distinct present zero, `false` and empty string. Participants and collections are keyed in `BTreeMap`s. Implement the canonical serializer for the three file kinds, the questions file format, and default-question generation (one `value` question per participant and value Target, then one `rank` question per ranking Target). The `tabular` module is added in Phase 7.
    - Observable: a missing value, an explicit zero and an empty string parse to three distinct values.
    - Blocked by: 4.6, 3.9.
    - _Requirements: 10.1, 10.3, 11.1, 11.5, 16.1, 8.7_
    - Design §3.6.3, §3.9.1

  - [x] 5.7 Implement `rt-trace` types, three-valued logic and replay
    - Implement `Tri`, `Outcome` (`Decided`, `Indeterminate`), `DecisionTrace` with canonical serialization (entries `read`, `step`, `decision`, `not_applied`, `missing`, diagnostics), and `replay`, which re-derives each step through `rt-numeric` and compares Outcomes with exact equality. Traces hold locations and hashes only, never source text.
    - Observable: replay of a hand-built trace equals the recorded results, and an altered operand fails replay.
    - Blocked by: 3.9, 2.6.
    - _Requirements: 13.1, 13.2, 13.3, 13.4, 13.6, 13.11, 10.11, 11.4_
    - Design §3.6.3 (Decision_Trace)

  - [x] 5.8 Implement `rt-compile`
    - Implement `compile(&ValidationResult)`: re-derive statuses and compare the blocking Diagnostics with the supplied state (`RT-APR-0005` on any difference); return the blocking Diagnostics unchanged and in order if the state has errors; `RT-APR-0002` when no Effective revision exists; lower exactly the Effective revisions; Kahn's algorithm with a `BTreeSet<RuleId>` ready set (`RT-CMP-0001` on a cycle); record IR_Version, Rulebook content hash, compiler version and referenced document hashes. Compilation is atomic with no partial value on any path, and no Origin or ledger data affects the output.
    - Observable: compiling a Validation_Result with an unresolved revision returns its Diagnostics and no Compiled_Rulebook.
    - Blocked by: 5.5, 5.3.
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 9.8, 9.9, 9.10, 9.11, 9.12, 9.13, 9.14_
    - Design §3.6.2

  - [x] 5.9 Implement the `rt-engine` evaluator core
    - Implement `evaluate(rulebook, data, questions, published)` as a pure function. Implement `Eval = Known | Unknown | Fault`, Kleene AND, OR and NOT (no short-circuit, operands evaluated left to right), missing-input handling with declared defaults and recorded default spans, faults (`RT-EXE-0002`, `0003`, `0004`, `0005`), aggregation with the empty-collection rules (count and sum of empty are 0, min and max of empty are Unknown), `round` nodes and `weighted_score`. Only `BTreeMap` and sorted `Vec` are used. The engine reads no file, clock, environment or source text and has no application-specific term. Re-export `Outcome`, `DecisionTrace` and `Evaluation` from `rt-trace`.
    - Observable: a weighted score with exact decimal weights gives the exact sum, and a missing input gives Indeterminate with the input named.
    - Blocked by: 5.5, 5.6, 5.7.
    - _Requirements: 8.5, 8.6, 8.15, 8.29, 8.30, 8.31, 10.1, 10.2, 10.6, 10.7, 10.8, 10.10, 11.1, 11.2, 11.3, 11.4, 11.5, 11.6, 15.1_
    - Design §3.6.3

  - [x] 5.10 Implement ranking, precedence, execution conflicts and published comparison
    - Ranking: explicit direction per criterion, tie-breakers in declared order applied only to still-tied participants, `RT-EXE-0006` Indeterminate for the whole set when a needed value is missing, Tie_Group members listed by ascending id for serialization only, `Rank_Position = 1 + participants strictly ahead`. Precedence uses only the first-listed rule's result and records the others as considered and not applied. Without precedence and with unequal Known results, the subject is Indeterminate with one `RT-DGN-0004`. Empty min or max needed for an Outcome gives `RT-DGN-0005`. Published comparison by exact equality with no tolerance, differences in participant then field order. Add the two difference kinds of RN-26 (a published tie the computed result does not have, and a computed Tie_Group the published file splits) if the requirements amendment is accepted; otherwise leave them to task 9.2.
    - Observable: a ranking with a tie shows one Tie_Group with the compared values in the trace, and a published value that differs by one unit is reported.
    - Blocked by: 5.9.
    - _Requirements: 12.1, 12.2, 12.3, 12.5, 12.8, 12.9, 12.10, 12.11, 12.12, 13.12, 13.20, 8.26, 10.4, 10.5_
    - Design §3.6.3 (Ranking, Precedence and conflicts, Published comparison)

  - [x] 5.11 Implement the Explainer in `rt-explain`
    - Implement `verify_trace` and `render_trace` over pre-loaded, hash-verified Source_Models only. For each span, the first matching row of Req 13.16 wins and yields one Diagnostic (`RT-PRV-0006`, `0001`, `0002`, `0003`, `0004`, `0005`). A trace is `Verified` only if it has at least one span and every span resolves; zero spans gives the `RT-PRV-0007` Warning and not verified. Render source text only for resolved spans, equal to the model bytes, with every control character escaped by `escape_controls`. Never invent text. `rt-explain` must not depend on `rt-engine`.
    - Observable: a trace with one altered span offset is not verified and carries the specified code, and no text is rendered for it.
    - Blocked by: 4.1, 5.7.
    - _Requirements: 13.7, 13.8, 13.9, 13.13, 13.14, 13.15, 13.16, 13.17, 13.19, 13.22, 13.23, 19.5, 19.10, 15.9_
    - Design §3.5 (Explainer)

  - [x] 5.12 Implement the shared `results` module and the Proof_Report builder
    - In `rt-explain::results`, implement the results-file serializer (`outcomes`, `traces`, and a separate `verification` section) that `ruletrace evaluate` and both applications will use. In `rt-explain::report`, implement the Proof_Report builder with the section order of Req 33.14 (Question and Outcome; each applied Rule with identifier and Revision_Number; the Approval_Record of each Rule; each Source_Span with exact source text; each input value read; each calculation step with operation, operands and exact result; each tie-breaking criterion with compared values; each missing input; each considered Rule that did not apply; the verification status; the published-result comparison). It is built only from the Decision_Trace, the ledger and the Explainer output, contains no AI text, and has a human and a machine form. The header statements of Req 34 are added by task 11.9.
    - Observable: the Proof_Report for a slice fixture is byte-identical across two runs and its replay recomputes each step.
    - Blocked by: 5.11, 4.8.
    - _Requirements: 33.14, 13.11, 13.19, 13.21, 31.13_
    - Design §8.5, §8.11.9, §3.9.1

  - [x] 5.13 Implement the `compile` and `evaluate` commands
    - `ruletrace compile --validation --out` writes the artifact only on success, and refuses a Validation_Result that carries blocking Diagnostics with exit 3 (RN-29). `ruletrace evaluate --compiled --data [--questions] [--published] [--source-model...] [--ledger --rulebook --render --proof-report --proof-report-json] --out [--output-format]` writes `outcomes`, `traces` and a separate `verification` section. Exit 4 for any Indeterminate Outcome (including `RT-DGN-0004` and `RT-DGN-0005`), exit 2 when a provenance Error row applies (still writing complete outcomes and traces), and only 0, 2, 3, 4 and 70 may occur. `--render` and `--proof-report` never change the results file.
    - Observable: the slice fixture evaluates with exit 0, the missing-value fixture with exit 4, and the same run with `--proof-report` writes an identical results file.
    - Blocked by: 5.4, 5.8, 5.10, 5.12.
    - _Requirements: 16.3, 16.4, 16.5, 16.6, 16.7, 16.9, 16.20, 16.21, 16.22, 16.23, 17.1, 17.4, 13.18, 13.19_
    - Design §3.9.1, §6.1

  - [x] 5.14 Write negative tests for approval gating
    - Assert exact code, Severity and primary location for: unresolved revision, no executable rule, stale record after changing content, a span offset, a document hash and the IR_Version, rejected revision (never blocks), rejected revision modified, a Proposed revision below a Rejected one (does not block), a ledger contradiction, and a Pending_Proposal (never blocks). Required.
    - Observable: each case exits with the specified code through the CLI.
    - Blocked by: 5.13.
    - _Requirements: 5.4, 5.5, 5.6, 5.7, 5.14, 25.11, 25.13, 25.19, 26.12, 26.13, 20.3_

  - [x] 5.15 Write property test for unresolved approvals blocking compilation
    - **Property 10: Unresolved approvals block compilation**
    - Validation yields one `RT-APR-0001` per unresolved revision and a failed result, and compilation returns no artifact, `RT-APR-0002` when no Effective revision exists. Required.
    - Observable: no generated Rulebook with an unresolved revision produces a Compiled_Rulebook.
    - **Validates: Requirements 5.4, 5.5, 5.7, 5.14, 9.13, 25.13, 25.21, 26.12, 26.13, 26.28, 26.29**

  - [x] 5.16 Write property test for compiled set equals the Effective set
    - **Property 11: Compiled set equals the Effective set and compilation is atomic**
    - Required.
    - Observable: the compiled rule set equals the Effective set for every generated Rulebook and ledger, and no failing path yields a value.
    - **Validates: Requirements 5.2, 9.1, 9.2, 9.10, 9.11, 9.12, 25.17, 25.18, 7.6, 7.7**

  - [x] 5.17 Write property test for stage determinism
    - **Property 2: Stage determinism**
    - Cover all seven stage functions (ingest, extract, parse, validate, compile, evaluate, verify), independent of call order, process and other rulebooks. Required because it protects determinism.
    - Observable: repeated calls give identical bytes.
    - **Validates: Requirements 1.7, 4.11, 6.12, 9.4, 10.2, 10.8, 24.6**

  - [x] 5.18 Extend the hash invariance test to compiled-artifact bytes
    - Extend Property 13 (task 4.14): Origin and ledger differences that leave Rule_Content equal produce byte-identical compiled artifacts. Required.
    - Observable: a manually authored and an AI-origin Rule with equal content compile to equal bytes.
    - **Property 13: Rule_Content_Hash invariance**
    - **Validates: Requirements 9.5, 16.18, 18.5**

  - [x] 5.19 Write property test for locations never fabricated
    - **Property 8: Locations are never fabricated and are preserved**
    - Required because it protects provenance.
    - Observable: every Source location in a Diagnostic, Compiled_Rulebook or trace equals a span of the Rulebook.
    - **Validates: Requirements 7.3, 7.14, 7.15, 7.21, 9.3, 13.2**

  - [x] 5.20 Write property test for missing values never becoming defaults
    - **Property 20: Missing values never become defaults**
    - Required: the demonstration contains a missing-value case.
    - Observable: a missing input never evaluates to zero, false or empty without a declared default.
    - **Validates: Requirements 11.1, 11.2, 11.3, 11.5, 11.6, 13.5**

  - [x] 5.21 Write property test for ranking completeness and positions
    - **Property 23: Ranking completeness and positions**
    - Required: the demonstration ranks participants.
    - Observable: every participant gets one position or Tie_Group membership, and positions equal one plus the participants strictly ahead.
    - **Validates: Requirements 12.1, 12.2, 12.3, 12.8, 12.9, 20.2**

  - [x] 5.22 Write property test for input order independence
    - **Property 24: Input order independence**
    - Required because it protects determinism.
    - Observable: permuting participants, fields and collection elements leaves results byte-identical.
    - **Validates: Requirements 10.4, 10.3, 10.1**

  - [x] 5.23 Write property test for trace completeness and replay
    - **Property 25: Trace completeness and replay**
    - Required because it protects provenance.
    - Observable: every trace names its rules, spans, inputs read and steps, and replay reproduces the Outcome.
    - **Validates: Requirements 13.1, 13.2, 13.3, 13.4, 13.6, 13.11, 13.21, 10.11, 12.7**

  - [x] 5.24 Write property test for provenance verification
    - **Property 26: Provenance verification**
    - Required.
    - Observable: each failing row of Req 13.16 gives exactly its code, and a verified trace has resolved text equal to the model bytes.
    - **Validates: Requirements 13.7, 13.8, 13.9, 13.13, 13.14, 13.15, 13.16, 13.17, 13.19, 13.22, 19.5, 19.10**

  - [x] 5.25 Write property test for isolation of compiled rulebooks
    - **Property 40: Isolation of compiled rulebooks**
    - Evaluate two generated rulebooks in the same process in both orders and interleaved per record, and assert each result equals its solitary evaluation. Required.
    - Observable: results are byte-identical to the solitary runs.
    - **Validates: Requirements 9.7, 10.8, 20.6**

  - [x] 5.26 Write property test for exit code follows the earliest failing stage
    - **Property 30: Exit code follows the earliest failing stage**
    - Cover the four pipeline commands now. Task 8.8 extends it to `ruletrace-ai` and task 9.7 to the demonstration executables. Required.
    - Observable: for generated multi-fault inputs the exit code is the one of the earliest failing stage, and Warnings never change it.
    - **Validates: Requirements 16.13, 16.14, 16.15, 7.19, 22.19**

  - [x] 5.27 Add the Slice 0 golden fixtures and the isolation and trace-verification fixture tests
    - Create five fixtures from the §4.1.7 example under `tests/fixtures/`: `slice-basic` (eligibility, weighted score with exact decimal weights, ranking with tie-breaker), `slice-missing` (Indeterminate, exit 4, field and span asserted), `slice-tie` (one Tie_Group), `slice-unsupported` (one Unsupported Diagnostic, exit 3) and `slice-unresolved` (`RT-APR-0001`, exit 3). Each has a Rulebook, an Approval_Ledger, data and expected files (`source-model.json`, `validation.json`, `compiled.json`, `results.json`, `stderr.txt`, `exit-code`). Compare bytes in the `golden` job on all three platforms.
    - Add the isolation test (rulebook A then B, B then A, interleaved, equal to solitary runs) and the trace-verification test (the Explainer over every trace of every fixture with models preloaded). Both required.
    - Observable: all five fixtures are green with identical bytes on Linux, macOS and Windows.
    - Blocked by: 5.13, 5.14.
    - _Requirements: 16.1, 16.2, 16.3, 16.4, 16.5, 16.6, 16.7, 20.4, 20.5, 20.6, 21.11, 21.12, 13.7, 13.15_
    - Design §7.3

  - [x] 5.28 Checkpoint - Phase 3 exit
    - Run the full gate set and the `core-only` job. The core must be usable without AI: ingest, validate, compile and evaluate work from a ledger and a Rulebook written by hand. Exit codes 0, 2, 3, 4 occur as specified.
    - Exit criteria: Properties 2, 8, 10, 11, 20, 23, 24, 25, 26, 30 and 40 pass; the five slice fixtures are green on three platforms with identical bytes.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 6. Phase 4: Review of existing revisions and the demonstration harness (`rt-review`, `review`, `rt-demo`; ends Demo Slice 0)
  - [x] 6.1 Implement `rt-review` for revisions already in the Rulebook
    - Implement the pure `plan_review(rulebook, ledger, decision, sources) -> Result<CommitPlan, Rejected>` for `approve` and `reject` of a revision that exists in the Rulebook: resolve the target (`REVIEW_TARGET_NOT_FOUND`), `REVIEW_STALE_HASH`, invalid transition, reviewer checks (`REVIEW_REVIEWER_INVALID`, `REVIEW_SELF_APPROVAL`, rule of UD-23), reason checks, and a dry-run validation of the post-approval state that refuses with `REVIEW_BLOCKING_DIAGNOSTIC` for any blocking item naming the rule other than its own unresolved approval. The plan holds the exact new ledger bytes. The ledger-append function exists only in this crate and is reached only from `rt-cli`.
    - Confirm that `check-graph` forbids every `rt-ai-*` crate from depending on `rt-review` (rule written in 2.9). Adoption of Pending_Proposals is added in task 7.2.
    - Observable: planning an approval of a revision with a matching hash returns the ledger bytes with one more record, and a stale hash returns one `REVIEW_STALE_HASH` and no bytes.
    - Blocked by: 1.4, 1.7, 5.3.
    - _Requirements: 5.8, 5.9, 5.10, 5.12, 5.17, 23.14, 23.15, 25.1, 26.16, 26.17, 26.18, 26.24, 26.25, 26.26_
    - Design §3.8.2, §3.8.3 (step 4)

  - [x] 6.2 Implement `review list`, `approve` and `reject` with the one-file commit
    - In `rt-cli`, implement `ruletrace review list`, `approve` and `reject` with `--rule --revision --content-hash --reviewer --time [--create-ledger]` (and `--reason` for reject). They read no environment variable, no AI configuration and no clock. A ledger change is one atomic rename under an exclusive lock (`File::try_lock` or the lock-file fallback with `RT-APR-0212`, per 1.1). `review list` shows each revision awaiting a decision with its Rule_Content_Hash and source text through the Explainer, and writes nothing. Refusals return exactly one REVIEW Diagnostic with the specified exit code (3) and leave every file byte-identical.
    - Observable: `ruletrace review approve` on a hand-authored revision appends one ledger record and `validate` then passes. A wrong hash exits 3 with the files unchanged.
    - Blocked by: 6.1, 5.13.
    - _Requirements: 5.8, 5.9, 5.10, 5.11, 5.12, 17.1, 17.11, 16.19, 23.12, 27.12_
    - Design §3.9.1, §2.4, §3.8.3

  - [x] 6.3 Write property test for review refusals leaving files unchanged (single-file case)
    - **Property 15: Review refusals leave files unchanged**
    - Cover stale hash, invalid transition, invalid or AI-equal reviewer, blocking Diagnostic, missing target, bad reason and missing Source_Model. Task 7.5 extends it to adoption. Required.
    - Observable: every refusal leaves ledger and Rulebook byte-identical.
    - **Validates: Requirements 5.9, 5.10, 5.12, 5.17, 23.14, 23.15, 26.16, 26.17, 26.18, 26.24, 26.25, 26.26**

  - [x] 6.4 Write the no-other-channel approval test
    - Assert that no code path outside `ruletrace review` appends to the ledger or changes a status: scan for ledger-append callers, assert `rt-ledger` has no append function, and assert through `check-graph` that no AI crate depends on `rt-review`. Assert that no flag, config, environment variable, Rulebook field or Proposal_Store field sets Approved. Required.
    - Observable: adding a call to the append function from any other crate fails the test naming crate and line.
    - Blocked by: 6.2.
    - _Requirements: 5.12, 26.18, 26.26, 27.4, 18.13_

  - [x] 6.5 Create `rt-demo`: manifest, runbook runner and golden comparison
    - Create the test-only crate `rt-demo` (class T). Implement the manifest parser and checks (design §8.11.3: every field present, hash equals file hash, licence recorded, `transmission_permitted` recorded; failure names the document id and the field), the `ruletrace-runbook` block parser, the subprocess runner (environment cleared, logical `argv[0]` mapped to the built binary through `RT_BIN_DIR`, relative arguments only), and the golden comparison with `RT_DEMO_BLESS=1` bless mode that refuses to run when `CI` is set. A failure reports the step name, the golden file and the first differing byte offset. The runbook parser fails if the Markdown has an unmarked fenced command line.
    - Observable: a manifest with a missing licence fails with the document id and field name, and a one-byte golden difference reports the offset.
    - Blocked by: 4.17, 2.10.
    - _Requirements: 33.2, 33.12, 33.15, 20.26_
    - Design §8.11.2, §8.11.3, §8.11.12

  - [x] 6.6 Author the stand-in demonstration for the `hand` profile
    - Under `demo/<name>/`, commit a short stand-in text (public domain or written for the fixture), labelled as a stand-in in the manifest, with `"stand_in": true`, never part of the Demo_Corpus and never named as a real document. Add `rulebook/seed.json` (declarations and the document hash, no rules), `rulebook/hand-authored.json` (revisions with real source spans covering an eligibility predicate, a weighted score with exact decimal weights, and a ranking with a tie-breaker), small synthetic data labelled `synthetic` with an eligible and an ineligible participant, one Missing_Value on an eligibility input, one Tie_Group, and canonical data, questions and a published result. Add `RUNBOOK.md` with the machine-readable block for steps s01, s03 to s07 of design §8.11.4 (ingest; `review approve` once per revision with `--create-ledger` on the first, a caller-supplied reviewer and time; validate; compile; evaluate; evaluate with `--proof-report`), and bless the goldens.
    - Observable: running the steps by hand gives the golden files, and evaluate exits 4 because of the one Indeterminate case, as the runbook states.
    - Blocked by: 5.27, 6.2, 6.5.
    - _Requirements: 33.7, 33.11, 33.12, 33.13, 16.1_
    - Design §8.11.2, §8.11.4, §8.11.10, §7.5 (Slice 0)

  - [x] 6.7 Implement the Coverage_Statement generator for hand-authored rules
    - Implement `rt_demo::coverage::classify`, a pure function over Candidate_Clauses, Rulebook, ledger, store batches, ValidationResult and `coverage-notes.json`, with the first-matching-row classes of design §8.11.8 (`rule_approved`, `proposal_rejected`, `unsupported`, `ambiguity`, `not_a_rule`, else `CoverageError::Unclassified` naming the clause). Write `coverage-statement.json` and `.md`. Add `coverage-notes.json` for the stand-in clauses that are not rules, labelled authored. The store inputs are empty in this slice.
    - Observable: every Candidate_Clause of the stand-in appears once, in ascending offset order, and removing a notes entry fails naming the clause.
    - Blocked by: 6.6.
    - _Requirements: 33.6_
    - Design §8.11.8

  - [x] 6.8 Write property tests for Coverage_Statement partition and Proof_Report completeness
    - **Property 51: The Coverage_Statement classifies every Candidate_Clause exactly once**
    - **Property 52: The Proof_Report is complete, ordered and replayable**
    - Property 51: for generated clause sets and Rulebooks, each clause gets exactly one class in ascending offset order, the class agrees with the validation run, and an unclassified clause fails the generator. Property 52: every section of Req 33.14 is present in order, built only from trace, ledger and Explainer output, and the replay recomputes each step. Required.
    - Observable: both properties pass, and removing a clause or a calculation step fails them.
    - Blocked by: 6.7, 5.12.
    - **Validates: Requirements 33.6, 33.14, 13.11**

  - [x] 6.9 Write the demonstration acceptance test for the `hand` profile
    - Implement the acceptance test of design §8.11.12 for the `hand` profile: manifest checks, runbook execution step by step with exit codes, byte comparison of every produced artifact with its golden file, assertions that every Decision_Trace is Verified, the Proof_Report replays, the data contains the participants of Req 33.13 (the synthetic stand-in satisfies them), the Coverage_Statement matches, and the `ruletrace` dependency tree has no network, provider or AI crate (`check-tree` library function).
    - Add the three mutations M1 (one source byte), M2 (one data value) and M3 (one approved Rule_Content literal) of design §8.11.11 as an early smoke check (`RT-PRV-0002` and not verified; only dependent Outcomes change; `RT-APR-0001`, exit 3, no Compiled_Rulebook). M3 follows the RN-29 record. Print a readable summary (a step table with exit codes, the Outcome table, and the first Proof_Report section) when run with `-- --nocapture`.
    - Observable: `cargo test -p rt-demo --test acceptance hand -- --nocapture` passes and prints the summary.
    - Blocked by: 6.8, 6.4, 5.27.
    - _Requirements: 33.6, 33.7, 33.9, 33.11, 33.12, 33.14, 33.15, 33.17, 20.26_
    - Design §8.11.11, §8.11.12, §7.5 (Slice 0 exit)

  - [x] 6.10 Add the `core-only` and `demo-offline` (hand profile) CI jobs
    - Fill in the `core-only` job: build and test the Core_Crates and `ruletrace` with every auxiliary crate excluded, inspect the resolved tree with `check-tree`, and assert byte-identical output against the full build. Add the `demo-offline` job on the three platforms: build the executables (`cargo build --workspace --bins`), set `RT_BIN_DIR`, run the `hand` profile with no network (on Linux under `unshare --net`) and no credential. Both are required jobs.
    - Observable: the `demo-offline` job is green on Linux, macOS and Windows with byte-identical goldens.
    - Blocked by: 6.9.
    - _Requirements: 20.27, 21.3, 21.13, 33.16, 33.15_
    - Design §7.4

  - [x] 6.11 Checkpoint - Phase 4 and Demo Slice 0 exit
    - Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, and every existing `rt-xtask` check.
    - Exit criteria: the `hand` profile acceptance test passes on three platforms with identical goldens; every Decision_Trace of the demonstration is Verified; the Proof_Report replays; the three mutations pass as a smoke check; `check-tree` shows no network or AI crate in the `ruletrace` tree; Properties 15 (single-file), 51 and 52 pass.
    - Ensure all tests pass, ask the user if questions arise.

  - [x] 6.12 Demonstration checkpoint - Demo Slice 0
    - Run `cargo build --workspace --bins` and `cargo test -p rt-demo --test acceptance_hand -- --nocapture` (the `hand` profile test is in the target `acceptance_hand`; the target `acceptance` holds the `ai` profile tests, so a filter `hand` there runs 0 tests) and show the user the printed summary: a plain statement that the document is a stand-in and that no AI was used, the stand-in document text, the human approvals recorded by `ruletrace review` (reviewer, time, content hash), the Outcomes of every `evaluate` step (including the Indeterminate one with its reason, the Tie_Group, the diagnostic codes and the verification status), the complete-data happy path (s06c, s07b, exit 0) and the first Proof_Report section with the source text and the replayed calculation.
    - Observable: the user sees one hand-authored rulebook go from source text to a proved decision. Then stop for the user's review before the Slice 1 gates 1.8 to 1.10.
    - Ensure all tests pass, ask the user if questions arise.
    - Status note: not complete, awaiting the user's review. The command above was corrected (the earlier `--test acceptance hand` ran 0 tests and printed nothing). The summary printed by `acceptance_hand` now carries the stand-in and no-AI statement (backed by assertions: no `ruletrace-ai` step, no provider script in the manifest, every rule origin `manual`), the document, the approvals, all five `evaluate` steps and the first proof section. The transcript of the run is saved in the report of the change, see `traceability.md` (Demo follow-up).

- [x] 7. Phase 5: Proposals and adoption (`rt-proposals`, adoption in `rt-review`, `FaultFs`; Demo Slice 1 begins)
  - [x] 7.1 Implement `rt-proposals` (Proposal_Store data model)
    - Define the batch file model (`format`, `version`, `request_id`, `response_sha256`, provider, model, adapter version, AI identities, `created_at`, proposals with `proposal_id = SHA-256(request_id ":" index)`, kind, proposer, evidence, rule draft with Draft hash, Semantic_Outcome Diagnostics), its parser and printer, and the Pending_Proposal derivation (a `rule_suggestion` whose id is not referenced by an Origin in the Rulebook). Store the accepted response as the `response` member (`completion` and exact `payload`) as the UD-36 record says. Reject flag and lifecycle keys with the RT-DGN codes. No AI logic and no network dependency.
    - Observable: a batch round-trips byte-identically, and a batch with a `has_errors` key is rejected with `RT-DGN-0001`.
    - Blocked by: 1.8, 4.6, 6.11.
    - _Requirements: 23.11, 23.12, 23.13, 30.5, 22.16, 33.8_
    - Design §3.7.5, §3.8.2, §8.11.7

  - [x] 7.2 Implement adoption planning in `rt-review`
    - Extend `plan_review` with the Proposal_Store input: the prospective revision (`1 + max`, `RT-APR-0210` on exhaustion), the prospective Rule_Content_Hash that the Reviewer must name, and the plan holding the exact new bytes of both the ledger and the Rulebook (the adopted revision with its Origin). A rejection adopts the revision as Rejected so no number gap appears. Reviewer checks compare with the identities recorded in Origin and the batch.
    - Observable: adopting a Pending_Proposal gives revision 1, a rejection of the next gives revision 2 with no gap.
    - Blocked by: 7.1, 6.1.
    - _Requirements: 5.8, 5.9, 23.12, 23.14, 23.15, 23.34, 25.1, 26.16, 26.17_
    - Design §3.8.2, §3.8.3 (step 4)

  - [x] 7.3 Implement the journaled two-file commit, `FaultFs` and recovery
    - In `rt-cli`, implement the protocol of design §3.8.3: exclusive lock, journal recovery, load and verify, plan, write `.new` files and sync, hard-link or copy `.old` backups, write the journal, rename ledger first then Rulebook, remove journal and backups. An in-process failure after the journal rolls back and exits 70 (`RT-APR-0208`); a crash is recovered by the next review command. Read-only commands add the `RT-APR-0211` Warning when a journal exists and never write.
    - Implement `FaultFs` (test only) that fails each numbered operation in turn and simulates a crash after each step.
    - Observable: with a fault injected after the journal write, the next `review` command finishes or rolls back and both files are consistent.
    - Blocked by: 7.2.
    - _Requirements: 26.19, 26.30, 26.31, 25.1, 23.13_
    - Design §3.8.3, §6.2.4

  - [x] 7.4 Extend `review list`, `approve` and `reject` to adoption
    - `review list` shows pending proposals with the prospective revision, prospective Rule_Content_Hash and source text through the Explainer, and writes nothing. `review approve` and `reject` on a Pending_Proposal use the two-file commit of 7.3. All refusals still exit 3 and leave every file byte-identical.
    - Observable: `review approve` of a proposal changes the Rulebook and the ledger together, and `validate` then reports the rule as approved.
    - Blocked by: 7.3.
    - _Requirements: 5.8, 5.9, 5.10, 5.11, 5.12, 17.11, 23.12, 23.16, 27.12_
    - Design §3.9.1, §2.4

  - [x] 7.5 Extend the review-refusal property test to adoption
    - **Property 15: Review refusals leave files unchanged**
    - Add the adoption refusal cases (stale prospective hash, revision exhaustion, proposer equal to Reviewer) to the generator of 6.3. Required.
    - Observable: every adoption refusal leaves ledger, Rulebook and store byte-identical.
    - **Validates: Requirements 5.9, 5.10, 5.12, 5.17, 23.14, 23.15, 23.34, 26.16, 26.17, 26.24, 26.25, 26.26**

  - [x] 7.6 Write property test for all-or-nothing adoption with fault injection
    - **Property 16: Adoption is all or nothing**
    - Use `FaultFs` to fail each operation of the commit protocol and assert that either both files carry the new state or both are byte-identical to the earlier state, with revision numbers 1..n and no gap. Required.
    - Observable: no injected fault leaves a mixed state.
    - **Validates: Requirements 25.1, 26.19, 26.30, 26.31, 23.13**

  - [x] 7.7 Write recovery and concurrency tests
    - Simulate a crash after each step and assert roll-forward or cleanup on the next command. Two concurrent review commands serialize on the lock and the second gets `REVIEW_STALE_HASH` rather than a mixed state. Required because they protect the approval gate.
    - Observable: both tests pass on the three platforms.
    - _Requirements: 26.19, 26.31_
    - Design §3.8.3 (Crash recovery, Concurrency)

  - [x] 7.8 Extend the contradictory-flag property test to the other input kinds
    - **Property 5: Contradictory flag input is rejected**
    - Extend the generator of 4.12 to the validation artifact, compiled artifact, Proposal_Store entry, review decision, fixtures and CLI input, each with exit code 2. The Provider_Response case (`RT-AI-0204`, exit 6) is added in 8.11 and the application input kinds in 9.12. Required.
    - Observable: every input kind rejects each flag and lifecycle key with the specified code.
    - **Validates: Requirements 22.6, 22.7, 22.9, 22.10, 22.11, 22.12, 22.13, 22.16, 22.17, 22.18, 20.25**

  - [x] 7.9 Checkpoint - Phase 5 exit
    - Run the full gate set. Confirm the `ruletrace` tree has no AI or network crate.
    - Exit criteria: Properties 15 and 16 pass with fault injection; recovery and concurrency tests pass; the no-other-channel test still passes; UD-36 is `Confirmed`.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 8. Phase 6: AI pipeline with the mock provider (`rt-ai-adapter`, `rt-ai-mock`, `rt-ai-tool`, `propose`; ends Demo Slice 1)
  - [x] 8.1 Implement `rt-ai-adapter`
    - Implement the `AiAdapter` trait (`attempt`, `clock`), `MonotonicClock`, `ProviderRequest`, `ProviderResponse`, `Completion`, `ProviderFailure`, `AttemptReport`, and the pure retry decision `next_step(policy, history, elapsed)` with the 60 s deadline, 20 s attempt timeout, two retries at 2 s and 8 s, `Retry-After` as integer seconds, no jitter, and retry only when `remaining >= wait + attempt_timeout`. The crate depends only on `rt-foundation`, `rt-hash` and `rt-json`, as its line in `tools/allowed-edges.v1` says.
    - Observable: a table of attempt histories gives the specified next step with no clock read.
    - Blocked by: 7.9.
    - _Requirements: 29.1, 29.2, 29.3, 29.4, 29.5, 29.6, 29.13, 29.15, 29.18, 18.9_
    - Design §3.4.5, §3.7.6

  - [x] 8.2 Implement `rt-ai-mock` with step scripts and replay mode
    - Implement the Mock_Provider with the `ruletrace-mock-script` format (steps with `elapsed_ms` and a `response` or `failure` result), a `SimulatedClock` that advances only on `wait`, and `ScriptExhausted`. Add the replay mode (`provider = "mock-replay"`) over the Recorded_Provider_Script format of design §8.11.7: `attempt` computes the Request_Identifier, returns the entry's payload with zero elapsed time, and returns `ProviderFailure::Other` with the Request_Identifier in `AttemptReport.detail` if no entry matches. No network, clock, randomness or environment.
    - Observable: a request whose identifier is not in the script yields `RT-AI-0105` with the identifier in the detail, and a matching one yields the stored payload.
    - Blocked by: 8.1.
    - _Requirements: 23.25, 29.14, 20.8, 33.8_
    - Design §3.7.10, §8.11.7

  - [x] 8.3 Apply the steering updates of design §9.3 items 1 to 4 and 9
    - Prepare the patch for items 1 to 4 (incomplete-run exception for `ruletrace-ai`, I/O wording, numeric decision from 1.3, revision number assigned at adoption) and item 9 (the Proposal_Store retains the accepted response, per UD-36) for `.kiro/steering/`. Show the patch to the user with the user_input tool and apply it only after the user confirms. Leave item 5 untouched (DD-2 is open). Items 6 to 8 and 10 are applied in tasks 9.19 and 11.12.
    - Observable: the steering files contain the confirmed wording, or the task records that the user declined.
    - Blocked by: 1.9, 8.1.
    - _Requirements: 23.35, 23.36, 30.1, 30.2, 27.8_
    - Design §9.3

  - [x] 8.4 Implement `rt-ai-tool` configuration, consent, credential and redaction
    - Parse the TOML AI_Configuration with `toml_edit` (parse-only, with line and column spans) and record the dependency justification. Run the checks in the exact order of Req 28 criterion 17: inline-secret and key-like field rejection (`RT-AI-0003`), limit ranges (`0005`), endpoint https except loopback (`0004`), enabled state (`0001`), provider (`0006`), consent equal to the documents of the run (`0008`), then credential (`0007`) read last from the named environment variable only. Support the `mock-replay` provider kind with no endpoint and no credential.
    - Implement `Credential` (redacted `Debug`, no `Display`), the single `Out` type applying `Redactor` (also the `Bearer <value>` form), the Transmission_Log append before every attempt (no text, no credential, `RT-AI-0010` on failure), and the permission Warning.
    - Observable: a config with an inline secret exits 2 with `RT-AI-0003` and sends nothing, and a canary credential appears in no output.
    - Deviations for user decision: (1) the `script` field of provider `mock-replay` is not listed in Req 28.2 and needs a Req 28.2 amendment by the user. (2) A non-http(s) scheme such as `ftp://localhost` is rejected as `RT-AI-0004`, which is stricter than Req 28.6.
    - Blocked by: 8.2.
    - _Requirements: 28.1, 28.2, 28.3, 28.4, 28.5, 28.6, 28.7, 28.8, 28.9, 28.10, 28.15, 28.17, 28.18, 23.19, 23.22_
    - Design §3.7.8, §3.7.9, §3.9.2

  - [x] 8.5 Implement request building, Request_Identifier and `--dry-run`
    - Sort candidates by (start, document id bytes, end), split into consecutive chunks (default 20, range 1 to 100, at most 256 KiB), reject a clause over 8,192 bytes with `RT-AI-0304` before any transmission, and build canonical request bytes with the `vocabulary` for the UD-32 option. `request_id = SHA-256(canonical bytes)`, computed once. Only clause texts are sent as document text. Add `ruletrace-ai propose --dry-run`, which prints each Request_Identifier and sends nothing.
    - Observable: `--dry-run` prints the identifier for each request, and the identifier changes when the vocabulary or one clause changes.
    - Blocked by: 1.11, 8.4.
    - _Requirements: 23.2, 23.3, 23.32, 29.7, 29.9, 29.16, 28.2_
    - Design §3.7.2, §8.9, §8.11.7
    - Note (8.5 review): exact duplicate Candidate_Clauses (same document id, start and end) are rejected with `RT-AI-0304` naming the duplicate, before any transmission; they are never deduplicated silently.
    - Note (8.5 review): Rulebook set members are not sent. Req 23.2 asks for the type declarations, and set members are values, not types.
    - Note (8.5 review): the CLI wiring of `ruletrace-ai propose --dry-run` belongs to task 8.8. Only `dryrun::dry_run` exists now.
    - Note (8.5 review): the unsupported names in `vocab.rs` are hard-coded and `tools/known-unsupported.v1` (design.md, Reserved names) does not exist. No task in this file creates it. When it or an equivalent validator capability list exists, a test must tie the request vocabulary to those names. Track it under the task that creates the file; until a task names it, this note stands and task 8.6 (validator side of `unrecognized`) is the nearest owner.

  - [x] 8.6 Implement response parsing, strict schema, provenance and semantic validation
    - Parse with `rt-json` (depth 32, duplicate keys rejected). The single Diagnostic follows the order: too large, malformed, incomplete or refusal, forbidden field (`RT-AI-0204` for any listed name, agreeing or contradictory), schema violation. Implement the five proposal kinds with closed field sets and the `unrecognized` expression and kind variants of §3.7.3 so that an unsupported clause reaches `validate_content` and yields `RT-VAL-0003`.
    - Verify every span of every proposal, including advisory kinds, against the Source_Models in the order of Req 23.7 (`RT-PRV-0101` to `0104`) with Input_Locations in the response, repairing nothing. Run `validate_content` for each `rule_suggestion`. Semantic findings are stored and never change the exit code (UD-26). No provider-supplied flag is stored. Refuse to store a payload that contains the credential value (`RT-AI-0206`).
    - Observable: a response with a quote that differs from the Source_Model bytes is rejected as a whole batch with the specified code, and a well-formed unsupported clause is kept as a non-executable proposal.
    - Blocked by: 8.5.
    - _Requirements: 23.5, 23.6, 23.7, 23.8, 23.9, 23.10, 30.1, 30.2, 30.3, 30.4, 30.5, 30.7, 30.9, 30.10, 30.11, 22.16_
    - Design §3.7.3, §3.7.4, §8.11.7, §8.11.8

  - [x] 8.7 Implement the Proposal_Store writer, call loop and Run_Summary
    - Implement the idempotency check, the temp-file write with `sync_all`, create-if-absent by hard link that fails closed (`RT-AI-0303`, exit 70, store unchanged, temp file removed, no rename fallback), `RT-AI-0301` when a batch exists or a concurrent run wins, and `RT-AI-0303` on any write failure with no partial batch file. Implement the call loop using `next_step` and the adapter, per-attempt Warnings, final codes (`RT-AI-0101` to `0108`), sequential requests with no request after the first failure, the Run_Summary lists (`processed`, `stored`, `unprocessed`) and `RT-AI-0306` when an earlier batch was kept.
    - Observable: a three-request run whose second request fails keeps the first batch, lists the remaining clauses as unprocessed and never claims completeness.
    - Carried from 8.6 review: emit `RT-AI-0305` (`AcceptedBatch::blocking_warning()`) only after a successful store write, with a test that a failed write emits none. The store writer accepts only `AcceptedBatch`, never a bare `Batch` or `Proposal`. The Run_Summary writer and the Transmission_Log writer each get their own canary test (`sk-canary-Zq7x93LmN` absent from every written byte and every Diagnostic).
    - Blocked by: 1.9, 8.6.
    - _Requirements: 23.20, 23.35, 23.36, 23.37, 23.38, 29.7, 29.8, 29.10, 29.11, 29.12, 29.17, 29.19_
    - Design §3.7.5, §3.7.6, §3.7.7

  - [x] 8.8 Implement the `ruletrace-ai propose` command
    - Implement the argument parser, exit codes 0, 2, 3, 5, 6 and 70 with the earliest-failing-stage order of Req 16.17, and stdout and stderr through `Out`. The tool never opens a ledger file and never writes the Rulebook. `rt-ai-tool` has no edge to `rt-review`, `rt-compile`, `rt-engine` or `rt-compiled`, which `check-graph` enforces. Extend Property 30 to `ruletrace-ai`.
    - Observable: `ruletrace-ai propose --config ai/mock-replay.toml ... --consent D` writes a batch to the store and a Transmission_Log entry and exits 0.
    - Carried from 8.5: wire `--dry-run` to `dryrun::dry_run` (print each Request_Identifier, send nothing, write no log), with a CLI-level test that runs the parsed command and asserts the identifiers on standard output and no transmission.
    - Carried from 8.6 review: `Out` and `Redactor::redact` match only the whole credential, so a truncated or cut copy of it passes unredacted. The `Out`, Run_Summary and Transmission_Log canary tests must use a credential longer than 60 characters, a dotted canary and a bracketed canary, and must assert that no run of 8 characters of the credential appears in any output.
    - Carried from 8.7 review: 8.8 supplies `created_at` and `log_time` from `main` (not library code) and validates their format, because the store writes `created_at` verbatim. `Out` still uses whole-value `redact` (already carried above). The store-path diagnostics (`RT-AI-0303`, the stale-file hint, `RT-AI-0307`) are not fragment-redacted and must pass through `Out` or `redact_fragments` before they are shown.
    - Blocked by: 8.7.
    - _Requirements: 16.16, 16.17, 16.24, 17.7, 18.1, 18.13, 23.1, 27.1, 27.2, 27.3, 27.4, 27.8_
    - Design §3.9.2, §6.1

  - [x] 8.9 Write property test for request and proposal identifiers
    - **Property 31: Request and proposal identifiers**
    - Required: the recorded script is keyed by the Request_Identifier.
    - Observable: equal requests give equal identifiers and every change changes them.
    - **Validates: Requirements 23.2, 23.3, 23.32, 29.7, 29.9, 29.16**

  - [x] 8.10 Write property test for the retry decision function
    - **Property 32: Retry decision function**
    - Required: a pure function over a simulated timer.
    - Observable: retries only on transient failures, on the 2 s and 8 s schedule, never beyond the deadline.
    - **Validates: Requirements 29.1, 29.2, 29.3, 29.4, 29.5, 29.6, 29.13, 29.15, 29.18, 23.17**

  - [x] 8.11 Write property test for whole-batch rejection and strict acceptance
    - **Property 33: Whole-batch rejection and strict acceptance**
    - Each single mutation of a valid response is rejected with its one specified code and nothing is stored. Include the Provider_Response flag case of Property 5 (`RT-AI-0204`, exit 6). Required because it protects provenance.
    - Observable: every mutation of a valid response is rejected with one code, and the store is unchanged.
    - **Validates: Requirements 23.5, 23.6, 23.7, 23.8, 23.10, 30.1, 30.2, 30.3, 30.4, 30.5, 30.10, 30.11**

  - [x] 8.12 Write property test for the same validation of manual and AI-origin content
    - **Property 34: Same validation for manual and AI-origin content**
    - Include instructions inside a response or a Source_Document: no ledger, status or Diagnostic code changes. Required because it protects the approval gate.
    - Observable: the validator gives identical Diagnostics for equal content whatever its Origin.
    - **Validates: Requirements 18.3, 18.5, 18.6, 18.14, 18.16, 23.9, 30.8, 20.20**

  - [x] 8.13 Write property test for credential redaction
    - **Property 37: Credential redaction**
    - Embed a canary credential in generated provider text for every failure mode. Required for credential safety.
    - Observable: the canary appears in no stdout, stderr, log, store or file.
    - **Validates: Requirements 23.22, 23.23, 28.9, 28.15, 28.20**
    - Note (8.13 coverage is partial): the property covers the Redactor, `Out` streams, Diagnostics of every `RT-AI` code and the Transmission_Log line. Proposal_Store entries, provider-failure texts and files are not yet covered. Re-run and extend the property when tasks 8.6, 8.7 and 8.8 land.

  - [x] 8.14 Write property test for configuration checks
    - **Property 38: Configuration checks are ordered and total**
    - Required: configuration is the consent boundary.
    - Observable: for generated configurations exactly one earliest check fails and nothing is transmitted.
    - **Validates: Requirements 28.1, 28.3, 28.4, 28.5, 28.6, 28.7, 28.17, 28.18, 23.19**

  - [x] 8.15 Write gate-level mock-provider failure tests
    - One required test per failure class: transport failure with retry then success (one stored batch), retry exhaustion, a non-transient failure (no retry), a malformed response, a schema violation, a bad span, a quote mismatch, a store write failure, and a well-formed unsupported clause stored as a non-executable proposal. Assert exit codes, the Run_Summary and that nothing is stored on a protocol failure. The full cross product, simulated-timer matrix and idempotence under concurrency are in Phase 10 (task 12.10).
    - Observable: each case gives its specified code and exit code.
    - Blocked by: 8.8.
    - _Requirements: 20.7, 20.8, 20.9, 23.17, 23.33, 23.38, 29.14_

  - [x] 8.16 Add the `slice-ai` fixture and approval-gating tests
    - End to end with the Mock_Provider: propose, `review approve`, validate, compile, evaluate. The compiled output is byte-identical to the manually authored equivalent (Req 16.18). Add the self-approval attempt (Reviewer equal to the proposer, provider, model or an AI identity) and the prompt-injection fixture (clause text instructing approval). Neither may change any ledger, status or Diagnostic code. Required.
    - Observable: the fixture is green on three platforms, and both attack fixtures leave the ledger unchanged.
    - Blocked by: 8.8, 8.12.
    - Status note: every behaviour sentence is covered by `rt-tests-ai` (`slice_ai`, `slice_ai_attacks`). Green on three platforms: `test` jobs of CI run 43743980937 (commit b818ca3). See `traceability.md` (Checkpoint evidence, run 43743980937).
    - _Requirements: 16.18, 18.6, 18.14, 18.16, 23.14, 23.15, 26.17, 20.20_
    - Design §7.3

  - [x] 8.17 Author the provider script and add the `ai` profile with the full Coverage_Statement
    - Under `demo/<name>/`, add `provider/authored-script.json` keyed by the Request_Identifiers that `ruletrace-ai propose --dry-run` prints for the stand-in, every entry labelled `authored` (never described as provider output), `ai/mock-replay.toml`, and the `ai` profile steps of the runbook (s02 `propose`; s02b `review list` before any decision (no ledger yet; read-only, Req 26.6), then s03a, s03c to s03e one `approve` or `reject` per proposal with `--create-ledger` on the first, s03f a last `review list`; the same s04 to s07 and the complete-data steps s06c and s07b). Include one unsupported clause (so it reaches the Coverage_Statement as `unsupported`) and one rejected proposal with a reason. Extend `classify` to read the store batches and the ledger rejection records. Bless the goldens, including the proposal store and the Transmission_Log.
    - Observable: the `ai` profile acceptance test passes, and the Coverage_Statement classifies every clause or names the unclassified clause.
    - Blocked by: 8.16, 8.21, 6.7, 1.11.
    - Carried from 8.16 review: the runbook approves proposals in dependency order; a rule reading a sibling's declaration is refused with RT-APR-0205 until that sibling is adopted.
    - Status note: implemented in `demo/stand-in-ai` and verified locally on Linux (`cargo test -p rt-demo --test acceptance`, controlled mutants killed). Decision applied (user): `review list` runs before the first approval as step s02b; the workaround listing after the first approval (s03b) is removed and the id s03b is retired. The goldens of s02b are new, the other goldens are unchanged. CI run 43743980937 was green on three platforms for the profile before this change. The task stays in progress: the CI run on three platforms has not run on the changed profile and the review is pending. Not claimed complete. See `traceability.md` (Demo follow-up).
    - _Requirements: 33.6, 33.7, 33.8, 33.9, 33.11, 33.12, 23.25_
    - Design §8.11.4, §8.11.7, §8.11.8, §7.5 (Slice 1)

  - [x] 8.18 Write the AI-independence and origin-independence tests
    - **Property 41: AI tool and CLI are independent**
    - For every combination of presence of `ruletrace-ai`, an AI_Configuration file and the credential variable, `ruletrace` stdout, stderr, files and exit code are byte identical for the same inputs, and equal between a full build and a core-only build (all AI crates and `rt-tests-ai` excluded). Run `check-tree` for `rt-cli` and every core crate. The matrix over three booleans is exhaustive (8 cells), which replaces generated cases (decision of the orchestrator, recorded in the 8.18 review). Origin independence (Req 16.18): the `hand` and `ai` demo documents differ, so a hand-authored rulebook over the `ai` document is compiled and its bytes are asserted equal to the `ai` profile's. Required.
    - Observable: the `core-only` job still passes with the AI crates present in the workspace, and the origin-independence assertion passes.
    - Blocked by: 8.17, 6.10.
    - **Validates: Requirements 17.7, 17.8, 18.1, 20.16, 27.9, 16.18**
    - _Also: 21.8, 21.13, 27.5, 27.6_
    - Status note: the matrix is exhaustive (user approved the 8 cells in place of 256 generated cases). Strengthened in the demo follow-up: the planted files are in the before/after snapshot and must be unchanged; produced files are compared across cells; a canary string (whole and 8-byte fragments) is scanned in stdout, stderr, every file and path of the cell, and, for the `ruletrace-ai` runs of the `ai` profile, in stdout, stderr, the proposal store, the Transmission_Log, the ledger and the run summary; origin independence is asserted field by field (document bytes and hash, identifiers, revisions, content, spans, ledger records, compiled bytes, Outcomes over both data files). The core-only CI job checks that its build holds no `ruletrace-ai`. CI run 43743980937 was green in `core-only` and `demo-offline` for the earlier version of these tests. The task stays in progress until a CI run of the changed tests and the review. Not claimed complete. See `traceability.md` (Task 8.18, Demo follow-up).

  - [x] 8.19 Checkpoint - Phase 6 exit and Demo Slice 1 exit
    - Run the full gate set including `core-only`. Confirm the steering decision of 8.3 is recorded.
    - Exit criteria: Properties 31, 32, 33, 34, 37, 38 and 41 pass; the `slice-ai` fixture and the `ai` profile are green on three platforms; a script entry is keyed by the identifier that `--dry-run` prints and the accepted response bytes sit in the stored batch; every clause is classified or named; steering items 1 to 4 are merged or declined.
    - Ensure all tests pass, ask the user if questions arise.

  - [x] 8.20 Demonstration checkpoint - Demo Slice 1
    - Run `cargo test -p rt-demo --test acceptance ai -- --nocapture` and show the user: the proposals (each with its source span and Request_Identifier), the human `review approve` and `review reject` steps with the rejection reason, the Coverage_Statement (every clause classified, including the unsupported one), and the equality of the `ai` and `hand` compiled artifacts. State that the script entries are authored and that the document is still the stand-in.
    - Observable: the user sees AI-shaped proposals pass through human approval to a proved decision with nothing hidden. Then stop for the user's review before gates 1.12 and 1.13.
    - Ensure all tests pass, ask the user if questions arise.
    - Status note: not complete, awaiting the user's review. The summary printed by `acceptance` now carries the stand-in and authored-script statements, the document, each proposal with its source span and Request_Identifier, every review step with the rejection reason, the decisions in the ledger, all five `evaluate` steps, the Coverage_Statement and the equality of the `ai` and hand-authored compiled artifacts (bytes and sha256). The command stays `cargo test -p rt-demo --test acceptance ai -- --nocapture`, which now also matches the two canary tests.

  - [x] 8.21 Implement `ruletrace-ai propose` revision options (`--revise-rule`, `--revise-revision`, `--feedback-file`)
    - Accept the three options together or not at all. `--revise-rule` names a Rule identifier of the supplied Rulebook, `--revise-revision` names one of its Revision_Numbers, and `--feedback-file` names a Reviewer feedback text. Include the named Rule_Revision's content and the feedback text in the Clause_Context as additional context, pass the resulting Proposal through steps 1 to 8 of Req 23.1, and store it as a new Rule_Revision with Origin AI_Proposal. The feedback text is untrusted and is sent only under the same consent as the clauses. Reading the Rulebook stays declarations-only for approval: the tool opens no ledger. Today the parser refuses these options with `RT-CLI-0004`. Leave that refusal in place until this task lands, then replace it.
    - Observable: a run with the three options sends a request whose context holds the named revision and the feedback, stores a Proposal with Origin AI_Proposal, and leaves the Rulebook and the Approval_Ledger unchanged. A run with one or two of the options, an unknown Rule identifier or an unknown revision is refused with a Diagnostic that names the option, and sends nothing.
    - Blocked by: 8.8.
    - Must land before 8.17.
    - _Requirements: 23.16, 23.1, 23.2, 23.9, 27.4, 28.7, 28.8, 28.14_
    - Design §3.7.2, §3.9.2

  - [x] 8.22 Fix fabricated CLI argument positions in review diagnostics
    - A review refusal that names an argument, for example `--reviewer`, builds a `CliLocation::Argument` with `position: 0`, so the rendered Location reads `at argument 0 (--reviewer)` even when the argument sits elsewhere on the command line. That position is invented, which Req 7.14 forbids. Either make `CliLocation::Argument.position` optional, so a Diagnostic that knows only the name carries no position, or have the CLI pass the real argv index to the review crates. Fix the four sites: `rt-ledger/src/reviewer.rs:63`, `rt-review/src/plan.rs:103`, `rt-review/src/adopt.rs:34` and `rt-cli/src/review.rs:386`. Keep the rendering and the canonical order of Req 7.10 and 7.13 stable for locations that have a position.
    - Write the regression test first. It runs `ruletrace review approve` with an invalid `--reviewer` and asserts that the rendered Location does not contain `at argument 0 (--reviewer)`, and that it carries either no position or the real argv index. Show that it fails on the current code with exactly that text, then fix and show it passes.
    - Observable: no review Diagnostic renders an argument position that was not on the command line, and the regression test fails before the fix and passes after.
    - Blocked by: 8.8.
    - _Requirements: 7.1, 7.13, 20.13, 7.14_
    - Design §3.8, §3.9.1

  - [x] 8.23 Report an oversize Proposal_Store batch in the review path as RT-LIM-0001 with the true size
    - Req 19.8 applies Req 19.3 to Proposal_Store files. `ruletrace-ai` already reports an oversize batch file as `RT-LIM-0001` with the size from file metadata (`rt-ai-tool/src/store_check.rs`). The review path does not: `rt-cli/src/review.rs` (about lines 119 to 131) reads the file with a limit of `MAX_PROPOSAL_BATCH_BYTES` plus one and hands the bytes to `rt-proposals::parse_batch`, which reports a size fault of the bytes it is given as `RT-AI-0302` with the size of the bounded read. Take the size from the file metadata before reading, refuse a file above the limit with `RT-LIM-0001` in the message format of the other size refusals (`input file '<path>' is <size> bytes, larger than the effective limit of <limit> bytes`), and read no content of it. Make the size mapping of `rt-proposals::parse_batch` consistent with that, so that a caller that passes bytes above the limit also gets `RT-LIM-0001` and not `RT-AI-0302`, with the length of the bytes it was given. Keep a file of exactly the limit accepted (Req 19.9).
    - Write the regression test first. It runs `ruletrace review list` over a store whose batch file is the limit plus one byte, and over one that is much larger, and asserts the exact code `RT-LIM-0001`, the exact size in the message and exit code 2. Add a test of exactly the limit, which must still be parsed, and a `parse_batch` test with bytes above the limit. Show that the new tests fail on the current code with `RT-AI-0302`, then fix and show that they pass. A mutant that takes the size from the bounded read must be killed by the much larger file.
    - Observable: the review commands and `ruletrace-ai` report an oversize batch file identically, with the true size, and a batch of exactly the limit is still accepted.
    - Blocked by: 8.22.
    - _Requirements: 19.3, 19.8, 19.9, 7.1_
    - Design §3.7.5, §3.8

- [ ] 9. Phase 7: Application layers (`rt-data::tabular`, `prize-check`, `bracket-check`; Demo Slice 2 begins)
  - [ ] 9.1 Implement `rt-data::tabular` (CSV parsing and schema-driven mapping)
    - Implement the CSV parser with line and column positions, the typed mapping to Competition_Data and Published_Result driven by the input schema of the Compiled_Rulebook (field names and types come from the schema passed in, never from the crate), the canonical serialization of the three file kinds, and the Application_Input_Format of design §8.3. An empty CSV cell, an absent JSON member and a JSON null are Missing_Values; an explicit zero, false or text stays that value. Return one Diagnostic per violation with `APP_INPUT_MALFORMED`, `APP_INPUT_FIELD_MISSING`, `APP_INPUT_DUPLICATE_PARTICIPANT` or `APP_INPUT_VALUE_INVALID` and an Input_Location (file, line, column, field name). A numeric literal with an exponent or read by a float conversion is `APP_INPUT_VALUE_INVALID`.
    - Observable: a CSV with a missing column, a duplicate participant and a bad number gives three Diagnostics with exact codes and positions, and calls no engine.
    - Blocked by: 5.6, 6.11.
    - _Requirements: 31.8, 31.9, 22.22, 7.23, 8.27, 11.5_
    - Design §8.3, §8.4, §3.1

  - [ ] 9.2 Implement published-rank annotation and the two extra difference kinds in the engine
    - Make the engine report, for a Published_Result with positions and ties, each difference of the kinds in design §3.6.3: a value that differs, a published tie that the computed result does not have, and a computed Tie_Group that the published file splits. Each difference carries the published value, the computed value and the reference to the Decision_Trace entry. No tolerance and no rounding of its own. If the user has not amended Req 13.12 (RN-26), record the amendment text in `docs/requirements-amendments.md` for the user and implement the design behaviour.
    - Observable: a published ordering that gives different positions to participants of one computed Tie_Group yields a difference for each such participant, and the equal case yields none.
    - Blocked by: 5.10.
    - _Requirements: 13.12, 13.20, 31.7, 32.6_
    - Design §3.6.3 (Published comparison), §9.4 (RN-26)

  - [ ] 9.3 Implement the UD-32 qualification option
    - If the record of 1.11 is Rank_Reference: add the one `rank_reference` expression operation to `rt-ir`, the Validator (type Exact_Number, `RANK_REFERENCE_CYCLE` with Severity Error and the primary location of the first Source_Span of the rule that closes the cycle), the Compiler and the Engine, with the rules of Req 14.12 (a to d) and the Tie_Group behaviour (every member gets the same Outcome). If the record is Threshold_Only: make the Validator return one `Unsupported` Diagnostic naming the capability `rank reference` for every occurrence (Req 14.13), and add no operation.
    - Observable: under Rank_Reference, a Tie_Group straddling a threshold gives every member the same Outcome and the trace shows the Rank_Position read. Under Threshold_Only, a rank-based clause gives `RT-VAL-0003` and the Coverage_Statement classifies it `unsupported`.
    - Blocked by: 1.11, 5.10, 5.2.
    - _Requirements: 14.11, 14.12, 14.13, 14.14, 32.3, 32.4, 7.23_
    - Design §8.9

  - [ ] 9.4 Implement the report model and writers in `rt-explain::report`
    - Implement the report document model, the human writer and the machine writer (field and element order fixed, no timestamp, no random value, no absolute path), and the field set of each report kind (design §8.5 to §8.7): entries in the order of the Outcomes the engine returned, Tie_Group members in ascending participant id for serialization only and marked as carrying no ranking meaning, Prize_Check fields (eligibility, exact weighted score, Rank_Position or Tie_Group, each Indeterminate with the missing input, published-result differences) and Bracket_Check fields (Standings, tie-breaking criteria with compared values, Qualification_Predicate Outcomes with rule id, revision, span and inputs read). No pairing, round, bracket position, seed, slot, advancement, elimination, prize, award or amount field. A report with no Source_Model states `provenance not verified` and carries `PROV_NOT_VERIFIED_NO_SOURCES`.
    - Observable: the writers produce byte-identical output on repeated runs, and the documented field set is exported as a constant that tests compare against.
    - Blocked by: 5.12.
    - _Requirements: 31.6, 31.13, 31.14, 32.1, 32.2, 32.5_
    - Design §8.5, §8.6, §8.7

  - [ ] 9.5 Implement the `prize-check` package library
    - Create the package `prize-check` (library target) with `mapping.rs`, `questions.rs` and `report.rs`. It maps the input file through `rt-data::tabular`, builds one Question per requested Target and participant, calls the engine once for the whole Question list, and lays the report out through `rt-explain::report`. It reads every parameter from the Compiled_Rulebook, contains no arithmetic, ordering, ranking or literal parameter, and performs no file, network, clock, random or environment access. A Target that is not declared or has an unreportable kind gives `APP_TARGET_NOT_DECLARED` or `APP_TARGET_KIND_MISMATCH` with a Cli_Location and calls no engine. Its dependencies are limited to `rt-foundation`, `rt-numeric`, `rt-data`, `rt-compiled`, `rt-engine`, `rt-explain` and `rt-srcmodel`, as its line in `tools/allowed-edges.v1` says.
    - Observable: a library test maps a CSV, evaluates, and returns a report whose entries equal the engine's Outcomes in order.
    - Blocked by: 1.12, 9.1, 9.4.
    - _Requirements: 31.1, 31.2, 31.4, 31.6, 31.7, 31.8, 31.9, 31.11, 31.14, 24.12, 15.5, 15.6_
    - Design §8.2, §8.4, §8.6

  - [ ] 9.6 Implement the `bracket-check` package library
    - Create the package `bracket-check` (library target, same file layout and allowed edges as 9.5). It reports Standings per ranking Target, each Tie_Group as one entry with its members, each Indeterminate position with the missing input, the tie-breaking criteria applied with the compared values, and each requested Qualification_Predicate and participant as true, false or Indeterminate with rule id, revision, span and inputs read. A published-standings file is mapped to a Published_Result and each difference the engine returns is reported. Under Rank_Reference a Tie_Group at the threshold reports every member with the same Outcome and states that the count of true Outcomes can exceed the numeric threshold N. The library applies no tie-breaking and no seeding of its own.
    - Observable: a library test shows a Tie_Group as one entry, an Indeterminate position with the missing input named, and a qualification Outcome per participant.
    - Blocked by: 1.12, 9.1, 9.3, 9.4.
    - _Requirements: 32.1, 32.2, 32.3, 32.4, 32.5, 32.6, 31.1, 31.2, 31.8, 24.12_
    - Design §8.7, §8.9

  - [ ] 9.7 Implement the two Demonstration_Executables
    - In each package add `src/main.rs` (argument parsing) and `src/io.rs` (read the named files, write the named report files by temp file and rename). Only `io.rs` and `main.rs` may open files, read arguments, exit or write standard streams. Options: `--compiled`, `--input`, the Targets, `[--published]`, `[--source-model...]`, `--report-human`, `--report-json`, and `--emit-data`, `--emit-questions`, `--emit-published`, `--emit-results` (the canonical data and results, for the byte-identity comparison). Exit codes: 0 when the report is complete and no Outcome is Indeterminate, 4 when at least one is, 2 for invalid input (including a Source_Model that is missing or whose hash differs), 70 for an internal error, equal to the code of `ruletrace evaluate` for the same inputs, and no partial report on failure. Read no environment variable, no AI_Configuration and no credential, open no network connection and start no subprocess. Extend Property 30 to both executables.
    - Observable: `prize-check` on the stand-in CSV writes the report and exits 4 because of the Indeterminate case, with the same exit code as `ruletrace evaluate`.
    - Blocked by: 9.5, 9.6.
    - _Requirements: 31.3, 31.11, 31.12, 31.13, 27.15_
    - Design §8.2, §8.4

  - [ ] 9.8 Implement `rt-xtask check-app-layers` and `check-executables`
    - `check-app-layers`: a source scan of the application packages that fails with crate, file and line on an operator or call that adds, subtracts, multiplies, divides, rounds, compares or aggregates Exact_Number values, a sorting, ordering, minimum, maximum or ranking function over participants, scores or Outcomes, a tie-breaking rule, a literal weight, threshold or rounding mode, or a field name of a specific competition. It also fails on any file, network, clock, random, environment or subprocess use outside `io.rs` and `main.rs`, and on any edge outside the application allowed list, directly or transitively (including `rt-cli`, `rt-review`, `rt-ledger`, `rt-validate`, `rt-compile`, `rt-source`, `rt-proposals`, every `rt-ai-*`, the other application, `rt-demo`, `rt-xtask`). `check-executables`: the set of executables the workspace builds equals `ruletrace`, `ruletrace-ai`, `prize-check` and `bracket-check` and names any other (`rt-xtask` is a development binary that is not delivered, RN-25). Add one negative fixture per violation class; each must fail with the expected file and line. Required.
    - Observable: both commands pass on the workspace, and each negative fixture fails as specified.
    - Blocked by: 9.7.
    - _Requirements: 31.10, 31.11, 24.12, 27.15, 15.8, 15.10, 20.17_
    - Design §3.3, §8.10

  - [ ] 9.9 Write property test for mapping preserving missing values
    - **Property 44: Mapping preserves missing values and reports each violation once**
    - Required: a missing value must never become zero, false or empty.
    - Observable: for generated tables each empty cell, absent member and null maps to a Missing_Value, and each violation gives one Diagnostic with exact code and Location.
    - **Validates: Requirements 31.8, 31.9, 31.15, 11.5**

  - [ ] 9.10 Write property test for application byte identity
    - **Property 42: Application layers are byte-identical to `ruletrace evaluate`**
    - For at least 256 generated Competition_Data sets, the results written by each application equal the bytes `ruletrace evaluate` writes for the same Compiled_Rulebook, canonical data, Questions, Published_Result and Source_Models, with the same exit code. Required.
    - Observable: the byte comparison passes for every generated set and for each Fixture.
    - **Validates: Requirements 31.5, 31.12, 15.7**

  - [ ] 9.11 Write property test for parameters read only from the Compiled_Rulebook
    - **Property 43: Applications read every business parameter from the Compiled_Rulebook**
    - The metamorphic test of Req 31.4: change exactly one weight, one threshold and the order of two tie-breaking criteria in the Rulebook, compile each variant with a test ledger that approves it, run the unchanged application, and assert that the Outcomes equal those of `ruletrace evaluate` for the variant and differ from the unchanged Rulebook where the change is observable. Required (no duplicated business logic).
    - Observable: each variant's application output equals the engine's, and the observable change appears.
    - **Validates: Requirements 31.4, 15.6, 31.10**

  - [ ] 9.12 Extend the contradictory-flag test to the application input kinds
    - **Property 5: Contradictory flag input is rejected**
    - Extend the generator to Application_Input_Format files, Published_Result files, the Recorded_Provider_Script and the Demo_Corpus manifest (`has_errors`, `no_errors`, a validation status or a lifecycle flag, agreeing or not), each exit code 2 with the code of Req 22.22. Required.
    - Observable: each input kind rejects each key with the specified code and no later stage runs.
    - **Validates: Requirements 22.22, 22.12, 22.13, 22.9, 20.25**

  - [ ] 9.13 Write property test for published differences
    - **Property 46: Published differences are exactly the engine's differences**
    - Required: the applications must apply no comparison of their own.
    - Observable: the differences rendered by each application equal the differences the engine returned, with published value, computed value and trace reference, and an equal Published_Result gives none.
    - **Validates: Requirements 31.7, 32.6, 13.12, 13.20**

  - [ ] 9.14 Write property test for report determinism and closed field set
    - **Property 47: Reports are deterministic, closed and carry no independent validity field**
    - Required for the honest-claims and no-hidden-logic checks.
    - Observable: repeated runs give identical bytes; the machine-readable field set equals the documented set and fails on any additional field; no report has a stored validity flag.
    - **Validates: Requirements 31.13, 31.14, 32.5, 22.1**

  - [ ] 9.15 Write property test for rank reference and Tie_Group outcomes
    - **Property 45: Rank reference gives every Tie_Group member one Outcome and never truncates the group**
    - Only if the record of 1.11 is Rank_Reference. Otherwise record in the task that the option is Threshold_Only and add the negative test that every rank-based clause gives one Unsupported Diagnostic naming `rank reference`.
    - Observable: for generated rankings with ties at the threshold every member has the same Outcome, and a rank reference cycle gives `RANK_REFERENCE_CYCLE`.
    - Blocked by: 9.3, 9.6.
    - **Validates: Requirements 14.12, 14.13, 32.3, 32.4**

  - [ ] 9.16 Add application fixtures with golden reports
    - Create Fixtures with golden machine-readable and human reports for each application: eligibility, a weighted score, a ranking with a Tie_Group, an Indeterminate Outcome from a Missing_Value, a published result that equals the computed result, a published result that differs (this includes the `published-differs` fixture), Standings with a Tie_Group, Standings with an Indeterminate position, a Qualification_Predicate over a weighted score, a Tie_Group at the threshold (under Rank_Reference over the Rank_Position), a published standings file that equals the computed Standings, and one that breaks a computed tie. Add the unit tests for each mapping error of Req 31.9 with exact code and Location. Compare bytes in the `golden` job on all three platforms.
    - Observable: all application fixtures are green on three platforms with identical bytes.
    - Blocked by: 9.7, 9.2, 9.10.
    - _Requirements: 31.15, 32.7, 31.9, 31.14, 20.4_
    - Design §7.3, §8.5

  - [ ] 9.17 Write the two-application isolation and process-independence tests
    - Isolation: run `prize-check` and `bracket-check` in the same process over two different Compiled_Rulebooks in any order and interleaved, and assert each result equals its result run alone. Process test: run each executable under at least two values of the clock, the working directory and the locale, with and without `ruletrace`, `ruletrace-ai`, an AI_Configuration file and a credential variable present, and assert byte-identical stdout, stderr, report files and exit code. Required.
    - Observable: both tests pass on the three platforms.
    - Blocked by: 9.7.
    - _Requirements: 31.11, 31.15, 20.6, 27.15_

  - [ ] 9.18 Add step s08 to the stand-in demonstration
    - Extend the runbook of the stand-in demonstration with s08 (`prize-check` and `bracket-check` on the CSV files with the `--emit-*` and `--report-*` options) and the extra step for `standings-missing.csv`. Assert in the acceptance test that the emitted canonical data, questions, published result and results equal the committed canonical files and the output of s06 (byte identity with `ruletrace evaluate`), and that the application reports equal their goldens. Bless the new goldens.
    - Observable: the `hand` and `ai` profiles both include s08 and pass on three platforms.
    - Blocked by: 9.16, 8.17.
    - _Requirements: 33.7, 33.15, 31.5_
    - Design §8.10, §8.11.4

  - [ ] 9.19 Apply the steering updates of design §9.3 items 6 to 8
    - Prepare the patch for items 6, 7 and 8 (two small demonstration executables beside the two RuleTrace executables, the application-layer dependency rule, the I/O wording for `io.rs` and `main.rs`, the quality gates for the acceptance test and the application scan, the isolation test of two applications). Show it to the user with the user_input tool and apply it only after the user confirms.
    - Observable: the steering files carry the confirmed wording, or the task records that the user declined.
    - Blocked by: 9.8.
    - _Requirements: 27.1, 27.15, 24.12, 31.10_
    - Design §9.3

  - [ ] 9.20 Checkpoint - Phase 7 exit
    - Run the full gate set, `check-app-layers` and `check-executables`, and the `demo-offline` job with the extended runbook.
    - Exit criteria: Properties 42, 43, 44, 46, 47 pass (45 if Rank_Reference); the application scan negative fixtures pass; the two-application isolation test and the process test pass; application results are byte-identical to `ruletrace evaluate`.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 10. Phase 8: Provider integration over the Responses protocol (`rt-ai-responses`, Meta Model API first) and recording (`export-script`)
  - Decision UD-37 (`tools/decisions/UD-37.md`, Confirmed) governs this phase: no `rt-ai-meta` and no `rt-ai-modal` crate, the planned crate `rt-ai-openai` is `rt-ai-responses`, Meta and OpenAI are Provider_Profiles (data in `rt-ai-tool`), Modal is deferred, and the first real provider is the Meta Model API with `muse-spark-1.3`. The open decision OD-37-1 (HTTP 504 and Req 29.2) does not block any task here, and Req 29.2 is not changed by any task.
  - Execution order (the ids are stable, the order is not numeric): 10.4, 10.10 and 10.11 first, then 10.12 and 10.14, then 10.1, 10.13 and 10.2, then 10.3, then 10.15 and 10.16, then 10.5, then 10.17 (stop for the user), then 10.8. 10.6 is independent. 10.7 keeps its place and its blocking of 10.8 (see 10.7).
  - Rust code, `Cargo.toml`, `deny.toml`, `tools/allowed-edges.v1` and `ci.yml` edits belong to the tasks below. None was made when UD-37 was recorded.
  - Every new task below follows the red/green rule of the testing policy: the named regression or negative test is shown failing against a controlled mutant or the absent behaviour, then passing.

  - [x] 10.4 Write the flat Proposal schema generator and its limit tests (no longer optional, runs first)
    - Un-deferred by UD-37. Implement the pure, provider-neutral generator (module `proposal_schema` of `rt-ai-tool`, not in `rt-ai-responses`) that writes the canonical flat node-table schema of design §3.7.3 from the request vocabulary, and the lowering of a Proposal_Schema version "2" document to the same Rule_Content as version "1" in the local strict parser, with the table rules (child index greater than the parent index, in range, each node referenced exactly once or the root, none unreferenced) refused with `RT-AI-0202`. Keep version "1" accepted unchanged. Commit the generated schema as a reviewed golden file.
    - Limits, measured by a counter that does not reuse the generator: nesting depth at most 10, at most 5,000 properties, at most 120,000 characters of names and enum values, at most 1,000 enum values, no `$ref` and no `$defs`, `additionalProperties:false` on every object, every property in `required`, no `oneOf` or `allOf`, `anyOf` only below the root.
    - Observable: the limit test passes with the exact bounds; an expression of depth 16 and one of depth 17 lower to Rule_Content equal to the original (fixed cases plus a property test of at least 256 cases); the schema bytes equal the golden file; a table with a cycle, a forward-violating child, a shared node, an unreferenced node or an out-of-range index gives `RT-AI-0202` with its location.
    - Red/green: each of these mutants of the generator or the lowering is killed by a named test: reintroduced `$ref` recursion, depth 11, a property count above 5,000, a missing `additionalProperties:false`, a property missing from `required`, an accepted child index not greater than its parent, an accepted shared node.
    - Blocked by: 8.19.
    - Status note: implemented and run locally (limit counter, golden, depth 16 and 17 round trips with 256-case property tests, table refusals, the seven named mutants). The task stays in progress until a CI run on the three platforms and the independent review. The `root` member of design §3.7.3 is replaced by one index per expression field (see the implementation notes there). Not claimed complete.
    - _Requirements: 23.4, 23.27, 23.39_
    - Design §3.7.3, §9.1 (R-D1)

  - [x] 10.10 Add the UD-37 catalogue entries and regenerate the `Code` enum
    - Add the rows `RT-AI-0011` AI_CAPABILITY_UNSUPPORTED, `RT-AI-0012` AI_MODEL_TRAINING_MODE_REFUSED, `RT-AI-0013` AI_CONFIG_ENDPOINT_HOST_NOT_PERMITTED (Error, exit 2) and `RT-AI-0109` AI_MODEL_DISCOVERY_FAILED, `RT-AI-0110` AI_MODEL_NOT_IN_CATALOGUE (Error, exit 5), all for `ruletrace-ai`, to `catalogue/diagnostics.v1.json` with the meanings of design §3.9.2, run `rt-xtask gen-catalogue`, and update any test that pins the number of codes, with the reason stated. The rows were not added when UD-37 was recorded because the generator rewrites `rt-foundation/src/code_generated.rs`.
    - Observable: `rt-xtask check-catalogue` passes with 152 codes (147 plus 5); each new code has the severity and exit code above; `Diagnostic::new` for each code gives that severity (a test per code); no existing row changed.
    - Red/green: a test that looks up each of the five codes by identifier fails before the rows exist and passes after.
    - Blocked by: 8.19.
    - Status note: the five rows are in the catalogue, `Code` is regenerated (152 codes), `check-catalogue` passes, and `crates/rt-foundation/tests/catalogue_ud37.rs` asserts each code's identifier, mnemonic, severity, exit code and `Diagnostic::new` severity from values written out of UD-37 section 5 (shown failing before the rows, passing after, and failing for a changed severity or exit code). No test pinned the code count. The task stays in progress until the CI run on three platforms and the independent review. Not claimed complete.
    - _Requirements: 7.15 to 7.20, 35.3, 35.6, 35.8, 35.18, 35.19_
    - Design §3.9.2, §5 (catalogue table)

  - [x] 10.11 Add the capability model, provider identity, usage and pure negotiation to `rt-ai-adapter`
    - Implement `Tri`, `Capabilities`, `StructuredOutput`, `Required`, `Unmet`, pure `negotiate()`, `Identity { provider, protocol, adapter_version }`, `Usage` and `ProviderMeta` (non-exhaustive, built through a constructor) as in design §3.4.5, and the optional reasoning effort in `ProviderRequest` (in the canonical bytes only when set). Update `rt-ai-mock` to build `ProviderMeta` through the constructor and to declare its Capabilities. No network, no environment, no provider-specific type. `rt-ai-tool` maps `negotiate()` failures to `RT-AI-0011` in task 10.13.
    - Observable: the truth table of `negotiate` (every combination of required and of yes, no, unknown) returns the exact `Unmet` list in ascending capability-name order, and an unknown required capability is unmet; `ProviderMeta` cannot be built with a struct literal outside the crate (compile-fail test); a request without an effort has the same Request_Identifier as before this task (a pinned hash), and a request with an effort has a different one.
    - Red/green: a mutant that treats unknown as yes is killed by the truth-table test; a mutant that always writes the effort field changes the pinned identifier and fails the test.
    - Blocked by: 8.19.
    - Status note: implemented and run locally, no new dependency. `ProviderId` (lowercase letters, digits, hyphen; 1 to 64 bytes; a `const` literal constructor so a mock names itself without a run-time failure), `Protocol` (`#[non_exhaustive]`), `Identity`, `Usage`, `Tri`, `Capabilities`, `StructuredOutput`, `Required`, `Unmet`, `negotiate`, and `ProviderMeta::new` with `with_` methods. A separate trait `AdapterInfo` (`identity`, `capabilities`) carries what an adapter declares, so that the eleven existing `AiAdapter` implementations are unchanged; the two mock providers implement it. The struct-literal refusal is a `compile_fail,E0639` doctest. `ProviderMeta` keeps the configured `model` beside the new fields. Not claimed complete: the CI run on three platforms and the independent review remain.
    - _Requirements: 35.1, 35.2, 35.11, 29.7, 18.12_
    - Design §3.4.5

  - [x] 10.12 Add the Provider_Profiles (Meta, OpenAI) as documented data in `rt-ai-tool`
    - Implement the profiles of Req 35.5 as constants and data with no network or credential code: provider id, protocol, official host, default Capabilities, accepted reasoning effort values, the `max_output_tokens` constant (at least 16; provisional, set in 10.1 and revised in 10.8), and the documentation URLs with the date each fact was read, in `docs/provider-profiles.md`. The Meta profile is as in design §3.7.3. The OpenAI profile marks `reasoning_effort` and `usage_reporting` unknown and records that its limits are re-checked here: read the current OpenAI structured-output limits from the vendor documentation and record the result and date in `docs/provider-profiles.md` (UD-37 section 3 lists them as not verified).
    - Observable: a test reads each profile and asserts the host, protocol, accepted effort list, capability values and that `none` is not accepted for Meta; a source scan test shows no network, environment or credential access in the profile module; `docs/provider-profiles.md` names a URL and a date for every profile fact.
    - Red/green: mutants that add `none` to the Meta efforts, that set OpenAI `reasoning_effort` to yes, or that change the Meta host are each killed.
    - Blocked by: 10.10, 10.11.
    - _Requirements: 35.5, 35.11_
    - Design §3.7.3
    - Status note: implemented and run locally. `rt-ai-tool` module `profile` holds `META`, `OPENAI` and `MOCK_REPLAY` as constants (host, protocol, capabilities, schema mode, recursive/depth/property limits, store control, model discovery, context and output limits `Unknown`, effort list, provisional `max_output_tokens` 8192, documentation URLs with read dates) and the path constants `/v1/responses` and `/v1/models`. `docs/provider-profiles.md` lists the source and the date of each fact. The OpenAI structured-output limits were re-read on 2026-11-16 (5000 properties, 10 levels, recursion supported); its effort, usage, store-control and model-discovery values stay Unknown. Local mutants M7 (none added to Meta efforts), M8 (OpenAI effort yes), M9 (Meta host changed) are each killed. The `max_output_tokens` value 8192 is a placeholder for 10.1 to set. The task stays in progress until the CI run on three platforms and the independent review. Not claimed complete.

  - [x] 10.14 Implement Proposal_Store format version 2 and the dual-version reader
    - In `rt-proposals` and the store writer of `rt-ai-tool`: write new batches as `"version":2` with the optional members `protocol`, `reported_model`, `response_id` and `usage` (omitted when absent, never null; a reported value above 256 scalar values or with a control character is omitted; the redactor covers every reported value), read versions 1 and 2, never rewrite an existing batch, count either version for idempotency (Req 29.10), and refuse another version, an unknown member or a wrong type with `RT-AI-0302` (and `RT-APR-0213` in `rt-review`). The reader of `rt-review` adoption and of `export-script` is the same parser. Show and review every demonstration golden whose stored batch changes from version 1 to 2, with old and new output and the affected criteria, as the testing policy requires. No golden is changed automatically.
    - Observable: a version 1 batch from the existing goldens is read unchanged with the optional members absent; a new batch has `"version":2`; usage with only `total_tokens` stores only that member; a response without usage stores none and no cost; a batch with a canary credential in a reported value is redacted; an unknown version fails with `RT-AI-0302`; a v1 and a v2 batch with the same Request_Identifier give `RT-AI-0301` and no second write.
    - Red/green: a mutant that writes null for an absent member, one that sums input and output into total, one that lets the reported model replace the configured model, and one that accepts version 3 are each killed.
    - Blocked by: 10.11.
    - Status note: implemented and run locally. `rt-proposals` reads versions 1 and 2 (`Format::V1`, `Format::V2(Extras)`), writes a version 1 batch back byte for byte, and refuses another version, an unknown member and a wrong type with `RT-AI-0302` (`RT-APR-0213` through `rt-review`). `rt-ai-tool` writes `"version":2` with `protocol` and, when the response reported them, `reported_model`, `response_id` and `usage`, each text of at most 256 scalar values without a control character, redacted, and each count only as reported. The two demonstration batch goldens changed from version 1 to 2 (each change is the digit and the 25-byte member `"protocol":"mock-replay",`; see the commit message). The two earlier batches are kept as `tests/fixtures/proposal-store-v1/`. The `export-script` reader does not exist yet; it will use the same `parse_batch`. The provider id of a version 1 configuration is now mapped in memory (`openai-responses` kind to id `openai`); a configuration of schema version 2 is task 10.13. The task stays in progress until the CI run on three platforms (the Windows goldens are unchanged for the batch files, which have no `.windows` variant) and the independent review. Not claimed complete.
    - _Requirements: 35.15, 35.16, 35.17, 29.10, 23.25_
    - Design §3.7.5

  - [x] 10.1 Implement Responses-protocol request building and response mapping
    - Confirm that `tools/decisions/UD-15.md` is `Confirmed` and `tools/decisions/UD-37.md` is `Confirmed`. In the new crate `rt-ai-responses` (renamed from the planned `rt-ai-openai`; it serves the Meta and OpenAI profiles), implement the deterministic request body: `POST {endpoint}/v1/responses` with `model`, `input`, `text.format = {"type":"json_schema","name":..,"schema":..,"strict":true}` taking the schema bytes of 10.4 from the `StructuredOutput`, `"store": false`, no `tools`, `max_output_tokens` from the profile constant, and `reasoning.effort` only when configured. Send no other field. The model, provider and protocol come only from configuration. Set the `max_output_tokens` constant to a documented provisional value of at least 16 that leaves room for reasoning tokens (they count toward it), say in `docs/provider-profiles.md` that it is provisional until the live observation of 10.8, and never estimate a price or a cost.
    - Map the envelope: require `status == "completed"`, ignore reasoning items, and require exactly one output message with exactly one `output_text` (anything else, including a tool call item, is `RT-AI-0201`); map refusal and incomplete to `Completion::Refused` or `Incomplete`; read `usage` and the reported model and response id into `ProviderMeta`; map 401 and 403 to `AuthFailed`, TLS errors to `TlsFailed`, 429 with an integer `Retry-After` of 1 to 9 digits to `RateLimited`, every 5xx and connection errors to `Unavailable`, and 3xx, 400, 402 and other 4xx to `Other`, by HTTP status and not by the error `type` or `code`. Read at most `response_bytes + 1` bytes. The credential is read only in this crate, from the named environment variable, and is redacted from every output.
    - Observable: a canned envelope for each status and each error shape of design §3.7.3 maps to its `Completion` or `ProviderFailure` in a unit test; the exact request body bytes are pinned by a test and contain no field outside the list.
    - Carried from 8.21 review: the provider body is the scanned request bytes plus constants; any text derived from request content must pass the RT-AI-0308 scan.
    - Blocked by: 8.19, 10.4, 10.11.
    - _Requirements: 23.27, 23.28, 23.29, 23.30, 23.31, 28.12, 28.13, 28.14, 35.12, 35.13, 35.14_
    - Design §3.7.3

    - Status note: implemented and run locally in `crates/rt-ai-responses` (body, envelope, status, credential, endpoint, adapter). The schema arrives as `StructuredOutput` bytes from the caller, and `max_output_tokens` arrives through `Settings` from the profile constant, so the crate does not depend on `rt-ai-tool`. The value stays the provisional 8192 and is not measured (decision needed, see the report). The credential type and its environment read live in this crate, and the older copy in `rt-ai-tool` is removed by 10.5. A malformed envelope returns an empty payload with `Completion::Complete` and a fixed reason in `detail`, so the caller reports RT-AI-0201. A body above the limit returns its first `limit + 1` bytes, so the caller reports RT-AI-0203. Refused and Incomplete carry an empty payload, and `parse_response` parses before it checks the completion, so an empty payload gives RT-AI-0201 and not RT-AI-0205: the order of that function must change in 10.5 (open finding). The task stays in progress until the CI run on three platforms and the independent review. Not claimed complete.
    - Follow-up from the wave 3 and wave 4 reviews (2026-11-16). Issue 6 (message-level `status` and `role` of the envelope) is deferred: Req 35.13 names only the envelope `status`, and whether Meta sets `status: "incomplete"` or `role` on a message is not known until the live run (10.8). Mapping on an unverified field could reject a valid answer, so the envelope stays as specified; 10.8 must record a real truncated answer and then decide. Issue 8 (Request_Identifier and the preamble, schema and `max_output_tokens`) is decided: the identifier intentionally covers only the canonical request, because authored mock-replay script keys must stay valid when the prompt text changes. The profile version in `adapter_version` records which prompt produced a batch (Req 35.23). A batch stored under an older profile therefore still short-circuits a new request (Req 29.10); the operator removes the batch to force a new call. Issues 9 (RT-AI-0203 wording) and 10 (endpoint trailing slashes and format characters) are fixed with regression tests.
  - [x] 10.13 Implement AI_Configuration version 2, the version 1 mapping, host restrictions and the model guard
    - In `rt-ai-tool`: parse schema version 2 (`[provider]`, `[limits]`, optional `[capabilities]`, with `reasoning_effort`, `verify_model` and `allow_provider_training` under `[provider]`), keep version 1 loading unchanged through an in-memory mapping to provider id `openai` and protocol `openai-responses` (and `mock-replay` to `mock-replay`), and never create, change, rename or delete a user's configuration file or a neighbour. Validate provider id, protocol, model and credential variable independently; the pair must be (meta, openai-responses), (openai, openai-responses) or (mock-replay, mock-replay), else `RT-AI-0006`. For ids `meta` and `openai` require the endpoint host to equal the official host, allowing only the loopback exception of Req 28.6, else `RT-AI-0013`; there is no field to allow another host. Refuse a model identifier that contains `contributor` in any case with `RT-AI-0012` unless `allow_provider_training = true`. Refuse effort `none` and unknown efforts with `RT-AI-0002` (Meta), and map `negotiate()` failures to `RT-AI-0011` with the location of the field that caused the requirement. Keep the check order of Req 28.17 as amended.
    - Observable: each row of the Req 35.21 host table, pair table, model-guard table (letter cases, the real `muse-spark-1.3-contributor` identifier, `allow_provider_training = true`) and effort table gives its exact code, severity, location and exit code with no network call and no credential read; a version 1 file loads with the same results as before, and the file bytes, its modification time and the directory listing are unchanged after the run; two runs of the same files give byte-identical output.
    - Red/green: mutants that compare the host by prefix or suffix, ignore a user name in the endpoint, drop the case-insensitive match of `contributor`, treat unknown as yes, write the mapped configuration back to disk, or reorder the checks are each killed.
    - Blocked by: 10.10, 10.11, 10.12.
    - _Requirements: 28.2, 28.12, 28.17, 35.3, 35.4, 35.6, 35.7, 35.8, 35.9, 35.10, 35.11, 35.21_
    - Design §3.9.2
    - Status note: implemented and run locally. Schema version 2 (`[provider]`, `[limits]`, `[capabilities]`) and the version 1 in-memory mapping are in `raw.rs`; the host restriction, model guard and `AiConfig::check_capabilities` are in `config.rs` and `endpoint.rs`. Capability negotiation runs inside `load`, after the model guard and before consent and credential. `propose --dry-run` first used `load_config` without negotiation; it now also negotiates capabilities (review wave 2 issue 5), so it fails like the real run. The version 1 test fixtures changed host `api.example.org` to `api.openai.com`, because version 1 `openai-responses` now maps to `openai` and is held to its official host (Req 35.9, 35.6); one assertion changed `schema_version = 2` to `3` as the invalid example. Local mutants M1 to M20 (suffix, prefix, host check off, user name unseen, guard off, case-sensitive guard, none accepted, unknown as yes, config rewritten on load, reordered checks, raised declaration, case-sensitive host, trailing dot, unscanned tables, wrong v1 id, pair unchecked) are each killed. Open questions are listed in the commit message and the hand-over report. The task stays in progress until the CI run on three platforms and the independent review. Not claimed complete.

  - [x] 10.2 Implement the HTTPS transport, monotonic clock and dependency bans
    - In `rt-ai-responses`, use `ureq` with `rustls` and the embedded root store, redirects disabled, proxy auto-detection disabled, per-attempt timeout, a real `MonotonicClock`, and no option to disable certificate or host verification. Record `ureq`, `rustls` and `webpki-roots` in `tools/dependencies.v1`. Rename the crate in `tools/allowed-edges.v1` (the `crate rt-ai-openai` line becomes `crate rt-ai-responses`, the `rt-ai-tool` dependency list, and `forbid-external ... except-under rt-ai-responses`), in the `check-graph` fixture lists of `rt-xtask`, and in the `--exclude` list of the core-only job of `ci.yml`. Add the `deny.toml` `bans` section listing network and AI crates with `wrappers = ["rt-ai-responses"]`. No `rt-ai-meta`, `rt-ai-modal` or `rt-ai-compatible` crate exists, and `check-graph` fails if one is added without an allowed-edges entry.
    - Observable: `cargo deny check` and `check-tree` pass, `check-graph` passes with the renamed crate, and `check-tree` still shows no network crate under `ruletrace`. A `check-graph` negative fixture with a network crate under `rt-ai-tool` directly still fails.
    - Blocked by: 10.1.
    - _Requirements: 28.6, 28.19, 21.5, 21.6, 21.7, 23.29, 23.30_
    - Design §3.2, §3.3, §3.7.3, §9.1 (R-D6)

    - Status note: implemented and run locally. `ureq` (default features off, `rustls` on), `rustls` and the embedded `webpki-roots` are recorded with their closure in `tools/dependencies.v1`. `deny.toml` bans the network and AI crates with `wrappers`, and the licence `CDLA-Permissive-2.0` of `webpki-roots` is added to the allow list (a policy change to confirm). `allowed-edges.v1`, the `check-graph` fixture list and the core-only `--exclude` list use `rt-ai-responses`. `cargo deny check`, `check-graph` and `check-tree` pass, and the core-only build and tests pass with the crate excluded. `cargo check --target x86_64-pc-windows-gnu` was not run to the end: `ring` needs `x86_64-w64-mingw32-gcc`, which this machine lacks. The Windows result comes from CI. Not claimed complete.
  - [x] 10.3 Write loopback stub-server tests with a canary credential
    - Using a `std::net::TcpListener` on `127.0.0.1:0` and hand-written HTTP/1.1 responses, test success, 503, a slow body, 429 with `Retry-After`, 401, an oversize body, malformed JSON, a 302 redirect (not followed, no credential or text sent) and a refusal. Assert `"store": false`, no credential in the body, the credential only in the authorization header, behaviour with `HTTPS_PROXY` set, and that the canary appears in no output. Required for credential safety, and needs no external network. Assert that the request body equals the bytes pinned in 10.1 and that `reasoning.effort` is present only when configured.
    - Observable: all stub tests pass offline on three platforms.
    - Blocked by: 10.2.
    - _Requirements: 23.27, 23.28, 28.9, 28.13, 28.19, 28.20, 21.8, 35.12_
    - Design §3.7.10

    - Status note: implemented in `crates/rt-ai-responses/tests` (`stub_http.rs`, `proxy_env.rs`, `no_verification_switch.rs`) with `std::net::TcpListener` on 127.0.0.1:0 and no new test dependency. Eight mutants (redirects followed, credential in body, `store: true`, `strict` dropped, two messages accepted, 5xx final, response limit off by one, proxy detection on) are each killed by a behavioural test. A test of certificate verification against a server with an unknown certificate authority is not written: it needs a certificate fixture, and the plain-text server case covers only the mapping to `TlsFailed`. Not claimed complete.
  - [-] 10.15 Write the Meta Model API profile stub tests
    - On the loopback stub server, encode the Meta contract of UD-37 (the shapes are the user's record and are first observed in the live run): 401 `invalid_api_key`, 429 with `Retry-After`, 503 `service_overloaded` and `backend_unavailable` with `Retry-After: 60` (assert that the header is treated as absent by Req 29.3 and the scheduled waits apply), 504 `gateway_timeout` (final under Req 29.2 as amended by the user's decision of 2026-11-16 on OD-37-1: one attempt, no waiting time, RT-AI-0105), 400 schema violation, 402 billing, a completed response with a reasoning item before the message, a response with a tool call item, a refusal, an incomplete response, and usage with and without each count. Branch on status: a 401 with a changed `type` still maps to `AuthFailed`. Assert the request path, the Bearer header, and that a host other than the official one is refused before any connection (task 10.13). Also re-read the Meta structured-output limits and record any change in `docs/provider-profiles.md`.
    - Observable: every shape gives its exact `ProviderFailure` or `Completion`, Diagnostic code, exit code and stored-batch state; the canary credential appears in no output; `usage` fields are stored exactly as reported and absent otherwise.
    - Red/green: a mutant that branches on `type`, one that retries a 400, one that treats a 503 `Retry-After` as a wait, and one that fabricates `total_tokens` are each killed.
    - Blocked by: 10.3, 10.12.
    - _Requirements: 35.13, 35.14, 35.15, 29.2, 29.3, 28.9, 20.14_
    - Design §3.7.3

    - Status note: the stub-test part is done in `stub_http.rs`: 401, 403, 429 with and without `Retry-After`, 503 with `Retry-After: 60` (the decision function still waits 2 s), 504 (changed on purpose on 2026-11-16: now `GatewayTimeout`, final), 400, 402, a reasoning item before the message, a tool call item, a refusal, an incomplete response, and usage with and without each count. The mutants of this task that concern the transport (5xx final, derived `total_tokens`) are killed. Not done: the host-refusal-before-connection assertion (10.13 owns the check), the re-read of the Meta limits, and the mutant that branches on `type`, which cannot exist because error bodies are never read. The mutant that retries a 400 is now killed by the end-to-end test of 10.5. Not claimed complete.
    - Status note (2026-11-16, second pass; stays `[-]` until CI and the independent review): user decisions applied. (a) The mutant "branch on the error body `type`" is accepted as structurally impossible, because error bodies are never read: `status::failure_for` takes the status and `Retry-After` values only, and `transport::read` returns for every non-2xx status before it touches the body. The justification and the tests that prove it are in `docs/provider-profiles.md` (section "Error bodies are never read"); no error-body parser was added. (b) New tests in `crates/rt-ai-responses/tests/stub_http.rs`: `a_401_or_403_is_auth_failed_with_one_attempt_for_every_body_shape` (14 body shapes, 401 and 403, `AuthFailed`, one request, `NonTransient` stop, no canary), `a_401_is_not_retried_even_when_a_success_is_waiting`, `a_stalled_error_body_is_not_waited_for` (declared 1 MiB body, 3 s stall, result in under 1.5 s) and `an_oversize_error_body_gives_the_same_failure_as_an_empty_one` (4 MiB body, two response limits). (c) Host refusal: the older `tests_e2e_responses.rs::a_foreign_host_is_rt_ai_0013_before_any_connection` asserts a zero connection count on the stub, which is vacuous for a foreign host (the stub is not that host); it fails today only through the test-time panic guard. The new `crates/rt-ai-tool/src/tests_host_before_connection.rs` (`a_foreign_host_asks_for_no_adapter_in_propose`, `a_foreign_host_asks_for_no_catalogue_in_models`, `the_official_origin_does_ask_for_an_adapter`) counts requests for an adapter or catalogue and proves none is made for six foreign spellings with exact codes. (d) Re-read of the Meta pages: dated section in `docs/provider-profiles.md`; no profile constant is wrong and none was changed. Mutants killed and reverted: AuthFailed mapped to Other, 401 as Unavailable (retried), 403 as Unavailable, whole error body read, error body read and branched on, host check accepting a foreign host. Open: three-platform CI and the independent review. Not claimed complete.
  - [x] 10.16 Implement the `ruletrace-ai models` command and `verify_model`
    - Add `GET {endpoint}/v1/models` to `rt-ai-responses` and the `models --config <file>` command to `rt-ai-tool` as in design §3.9.2: configuration checks up to the credential (not consent, not capability negotiation), one Transmission_Log entry without documents before the send, credential in the authorization header only, no document text and no Candidate_Clause in the request, model identifiers on standard output one per line in ascending byte order through `escape_controls`, nothing written but the Transmission_Log, no model selected, failure `RT-AI-0109` with exit 5. Implement `verify_model`: one query per `propose` run before the first Provider_Request, `RT-AI-0110` when the model is not listed, no query when it is false, and no Provider_Request sent after a failed query. Update the command set of Req 27.2 and 27.11 and the executable tests (the unknown-command test now expects `models` to be accepted and any other command to exit 2).
    - Observable: against the stub server, `models` prints the sorted identifiers and exits 0 with no consent argument, the captured request bytes contain no document text, the Transmission_Log entry has no document identifier, a 401 and a malformed body give `RT-AI-0109` exit 5, `propose` with `verify_model = true` and an unlisted model gives `RT-AI-0110` and writes no batch and sends no Provider_Request, and the configuration file is byte-identical after every run.
    - Red/green: mutants that send a Source_Document, that require consent, that choose a model from the list, that skip the Transmission_Log entry, and that continue after `RT-AI-0110` are each killed.
    - Blocked by: 10.3, 10.13.
    - _Requirements: 35.18, 35.19, 35.20, 27.2, 27.11, 28.8, 28.17_
    - Design §3.9.2
    - Status note: implemented and run locally (commit "ruletrace-ai models and verify_model"). `rt-ai-adapter` has the `ModelCatalog` trait and `CatalogFailure` (the design does not name the trait; it keeps `rt-ai-tool` provider-independent and lets a test double stand in). `rt-ai-responses` sends `GET {endpoint}/v1/models` through the same transport. The answer must be an object with a `data` array whose entries each have a string `id`; unknown members are ignored and any other shape gives no partial list. `models` follows the call policy of Req 29 (retries on transient failures, one Warning per failed attempt), appends one Transmission_Log entry of kind `models` per attempt (provider, protocol, model, endpoint host, no document, no time member because the command has no time argument), prints the identifiers sorted by byte order through `escape_controls`, and reads no consent. `verify_model` queries once after the input load, the plan, the credential scan and the store check, immediately before the first Provider_Request (`RT-AI-0110` at the `model` field, `RT-AI-0109` on failure, no request, no batch). The catalogue `created` values are not parsed or printed: Req 35.18 asks for identifiers only. A provider with no catalogue fails closed with `RT-AI-0109`. Mutants N1 to N12 (capability negotiation in `models`, document text in the request, configuration rewrite, no log entry, continue after `RT-AI-0110`, query when false, unsorted output, case-insensitive match, entry without id skipped, unescaped output, failed discovery ignored) are each killed. Not claimed complete: three-platform CI and the independent review are pending.

  - [-] 10.5 Wire the provider into `ruletrace-ai` and add the `live-openai` job
    - Select `rt-ai-responses` when the configuration names the pair (meta, openai-responses) or (openai, openai-responses), keeping the mock providers for tests. Add the non-required `live-openai` CI job that exercises the OpenAI profile only with a repository secret and is skipped otherwise. The Meta live run is run by the user locally (10.8) and is not a CI job. Ask the user with the user_input tool which secret name to document for the OpenAI job, and record it in the UD-15 record.
    - Observable: CI passes without credentials, and the live job runs when the secret exists. `ruletrace-ai propose` with a Meta version 2 configuration reaches the stub server on loopback end to end (configuration, capability check, host check, request, response, store version 2).
    - Carried from the 8.8 review: until this task links `rt-ai-responses`, `ruletrace-ai` reuses `RT-AI-0006` (AI_PROVIDER_UNKNOWN) for a configured hosted provider that is not linked. Remove that interim arm in this task, so that a linked pair selects the provider and `RT-AI-0006` is left for a pair that no provider implements.
    - Blocked by: 10.3, 10.13, 10.14, 10.16.
    - _Requirements: 21.14, 23.27, 28.12, 35.9, 35.10_
    - Design §7.4
    - Status note: the wiring is implemented and run locally. `BuiltInProviders` links `rt-ai-responses` for the (meta, openai-responses) and (openai, openai-responses) pairs and keeps the replay provider; the interim `RT-AI-0006` arm is removed, so `RT-AI-0006` is left for a pair that no provider implements (tests use an `Unlinked` build). The credential type and environment read are the integration crate's; `rt-ai-tool` keeps the `RT-AI-0007` mapping and the redactor. `parse_response` now decides an empty payload with `Refused` or `Incomplete` as `RT-AI-0205` before it parses (a non-empty malformed payload still outranks a refusal). Two defects found by the end-to-end tests are fixed: the stored batch named the kind (`openai`) as provider, and the configured reasoning effort never reached the request. 28 end-to-end tests in `tests_e2e_responses.rs` run `run_with` with the real adapter against the loopback stub (exact request bytes, 401, 429 with `Retry-After` on a simulated clock, refusal, incomplete, malformed envelope, tool call, oversize at limit plus one, 504 attempts, 400, no fallback model, no redirect, scan, consent, capability, model guard, host, credential, idempotence, dry run). Mutants M1 to M12 are each killed. The `live-openai` CI job is implemented (see the next paragraph; secret name recorded in UD-15 section 6). Open: the Transmission_Log entry of a propose attempt still lacks the `protocol` member that Req 28.8 lists. Adding it changes `demo/stand-in-ai/golden/artifacts/transmission.log.jsonl` (each line gains `"protocol":"mock-replay"` after `provider`), so it waits for the user's approval of that golden change. Not claimed complete.
    - Live job (2026-11-16, user-approved secret name `RULETRACE_OPENAI_API_KEY`, UD-15 section 6): job `live-openai` in `.github/workflows/ci.yml`, ubuntu-24.04, not in the `needs` of `ci-gate`, contents read only. A first `gate` step sets `configured=true` only when the secret and the optional repository variable `RULETRACE_OPENAI_MODEL` are both non-empty and prints which one is missing otherwise; every later step has `if: steps.gate.outputs.configured == 'true'`, so forks and unconfigured repositories skip with a success conclusion. When configured it validates the model text against `^[A-Za-z0-9][A-Za-z0-9._:-]{0,99}$` (a bash regex, after a red test showed that a `grep` check let a value with an embedded line feed through), writes mode-600 configurations under `$RUNNER_TEMP`, and runs only `ruletrace-ai models` (authenticated catalogue request, no document text). A manual `workflow_dispatch` with `propose = true` also runs one propose over the synthetic stand-in document and treats exit codes 0, 2, 3, 5 and 6 as informational. Nothing is uploaded. Tests: `crates/rt-xtask/tests/live_openai_job.rs` (structure rules with mutants, the gate script run under five environment combinations with a canary, the configuration step with hostile and non-ASCII model texts; the secret is step-level `env` on the gate, `models` and propose steps only) and `crates/rt-tests-ai/tests/live_ci_config.rs` (a real `propose --dry-run` over the generated configuration). The bash-based tests are `cfg(unix)` and are not run on Windows: of the 21 original structural-and-script tests only 12 run there (review wave 8, finding 6); the later additions follow the same split. A failing `models` step makes the non-required job red by design (UD-15 section 6). Verified locally without network and without a credential; NOT verified: a real Actions run, and the live run itself. Remaining for 10.5: the push and a CI run that shows `live-openai` skipped with success, the three-platform CI run of the whole task, the independent review, and the `live job runs when the secret exists` part of the observable, which stays unverified until the user creates the secret and the variable. Not claimed complete.
    - Follow-up from the wave 4 review (2026-11-16): the tool's endpoint validation and the adapter now share one loopback definition (`rt_ai_responses::is_loopback_host`) and one unusable-character check, with a table test of 40 spellings. `models` escapes every control character and U+2028/U+2029 before it appends the line feed (Req 35.18). Limit of the network guard: the runtime guard covers the unit tests of `rt-ai-tool` and of `rt-ai-responses`. The integration tests of `rt-ai-responses` link the library without `cfg(test)` and the spawned binary has no hook, so they rely on the source scan in `crates/rt-ai-tool/tests/network_guard.rs`, which does not see a host that a test builds from a variable.

  - [x] 10.17 User review gate: Meta stub tests before the live run
    - Stop here. When 10.3, 10.15, 10.16 and 10.5 pass on all three CI platforms (Linux, macOS, Windows) and the full gate set is green, show the user: the pinned request body, the stub-test results for each Meta error shape, the `models` transcript against the stub, the list of goldens changed by 10.14, `docs/provider-profiles.md`, and the status of OD-37-1 (resolved 2026-11-16, option B). Ask the user with the user_input tool whether to proceed to the live run. Do not run any live step, do not request or read any credential, and do not set `allow_provider_training`. Record the answer in `tools/decisions/UD-37.md`.
    - Observable: a CI run id with all required jobs green on the three platforms, and the user's recorded answer.
    - Blocked by: 10.3, 10.5, 10.15, 10.16.
    - _Requirements: 35.22, 28.21, 29.2_
    - Design §8.11.6
    - Status note (2026-11-16): closed by the user's explicit instruction ("Close Task 10.17 (user review gate), recording the acceptance and its evidence. Preserve the distinction between this user review and the independent code reviews."). The answer and evidence are in `tools/decisions/UD-37.md` section 8: CI runs 43799230103 (commit a2ce236) and 43847096409 (commit 438b700, 22 jobs after one rerun of three cancelled macOS jobs), and the live steps the user ran themselves. The gate text lists the blockers 10.5 and 10.15, which are NOT complete (`[-]`). The user closed the gate by decision while they remain open; this is not a claim that those blockers are satisfied. The independent code reviews (`docs/reviews/*ud37*`, waves 1 to 7) are listed separately there and are not the user review.

  - [-] 10.6 Implement `rt-xtask export-script` and the script checks
    - Implement `rt_proposals::export_script` (pure: batches in, canonical bytes out) and `rt-xtask export-script --store <dir> --provider <kind> [--provider-id <id>] --model <id> --captured "<text>" --out <file>`: entries ordered by `request_id`, `origin: "recorded"`, each `response_sha256` recomputed from the payload and compared with the stored hash, and the credential-looking-string scan of Req 28.15 over the payload refusing to write a matching script. The script format stays version 1 (`kind` is the protocol label, the optional `id` is the provider identifier). Read batches of format versions 1 and 2. Add the script tests: a hash that differs from the recorded hash fails naming file and Request_Identifier, a credential-looking string fails, and a Request_Identifier the script lacks gives `RT-AI-0105` through the mock replay. Equal stores give equal bytes.
    - Observable: exporting the store of the `ai` profile and replaying the exported script reproduces the same batches.
    - Blocked by: 1.8, 7.1, 8.8.
    - _Requirements: 33.8, 28.15, 23.25, 16.16, 35.17_
    - Design §8.11.7
    - Status note (2026-11-16): implemented and run locally. `rt_proposals::export_script` (pure) and `rt-xtask export-script` with `--provider-id`. Entries ordered by `request_id`, origin `recorded`, each hash recomputed and compared (mismatch names file and request), credential-looking strings of Req 28.4 refused in payload and header text without echo, equal stores give equal bytes, an existing output file is never overwritten, nothing else is written. Batches of versions 1 and 2 are read. Refused, beyond the task text: batches that differ in adapter version, model, protocol or provider (never merged or relabelled), and batches of the offline replay (protocol `mock-replay`), which would otherwise be labelled `recorded`. The format stays version 1 and does not carry the reported model, response identifier, usage or document hash. The first live batch is exported as `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json` and replays offline through `demo/stand-in-ai/ai/meta-replay.toml` to the same proposals and payload (`rt-tests-ai/tests/replay_live_recording.rs`). This needed the user's decision of 2026-11-16 to declare `reasoning_effort = yes` for `mock-replay`. The `RT-AI-0105` miss test of the task text is the existing demo acceptance test. Mutants C1 to C11 killed. Prompt-3 recording (2026-11-16): the second live batch (run 2, `responses-1/prompt-3`, response sha256 `7f3371ca...`) is exported from a read-only copy of its store to `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-3.json`, kept as fixture `rt-ai-tool/tests/fixtures/live-run2-prompt-3/batch.json`, and replays through `ai/meta-replay-prompt-3.toml`; both recordings are tested through `ruletrace-ai propose` and keep their own adapter version. Same Request_Identifier, separate files, never merged. Not claimed complete: three-platform CI and the independent review are pending.
    - User decision (option A, proposer label): the `proposer` of a proposal is kept as an untrusted model claim and `review list` renders `(proposer claimed by the model: '<p>', provider '<prov>', model '<m>')` (Req 23.12). Schema, prompt profile, refusal logic, ledger, content hashes, stored batches and recordings are unchanged. Disclosed golden change: `demo/stand-in-ai/golden/steps/s02b.stdout` (five lines) and `s03f.stdout` (one line), wording only.

  - [ ] 10.7 Add the live-flow support to the demonstration harness
    - Add `ai/meta.example.toml` and `ai/openai.example.toml` (credential named by variable only), the optional live commands in the runbook marked as live steps, and the harness rules: the live flow writes its store only to a location outside the golden files, never replaces a golden file or the Recorded_Provider_Script, and the harness refuses to run `ruletrace-ai` for a document whose `transmission_permitted` is not `true`. The consent names each transmitted document.
    - UD-37 does not change this task or its place. The harness guard is still needed before a live run on demonstration documents (Req 28.21, 33.10), so 10.8 stays blocked by it. If the user earlier deferred 10.7, 10.8 needs either this task or an explicit decision to run the live step by hand with the manifest checked manually. That choice is for the user.
    - Observable: a test shows that the live profile refuses a document without permission and writes only outside `golden/`.
    - Blocked by: 10.5, 6.5.
    - _Requirements: 33.10, 28.21, 28.11, 33.2_
    - Design §8.11.6
    - Status note (2026-11-16): implemented and run locally; the independent review and the CI runs on three platforms are pending, so the task stays open. `rt-demo` gets `guard.rs` and the runner calls it before any process starts. **Scope.** A `ruletrace-ai` step is offline only when it is `propose` with exactly one readable `--config` whose provider is `mock-replay` (v2 `[provider] protocol`, or the v1 `provider` key) and that has no `endpoint` and no `credential_env_var`. Every other `ruletrace-ai` step is live: `models`, a step with no `--config`, an unreadable or unknown configuration, a configuration outside the small TOML subset the guard reads, and arguments the guard cannot read the way the tool reads them. Steps of other executables are offline. `run_step` (and so `run_runbook_recorded`) refuses a live step with `DemoError::LiveRefused`. `run_live_step`, the explicit opt-in, needs a `LiveContext` with the checked manifest. It requires at least one `--consent`, and every named document must have `transmission_permitted` exactly `true` in the manifest (a missing entry or a non-boolean value is refused; the error names the document and the field). It then refuses a `--store`, `--out` or configured `transmission_log` that resolves (through `..` and symbolic links) under `golden/` or `provider/` of the demonstration or its work copy, or under any `golden` directory. It passes only the credential variable that the configuration names, and the harness itself reads no environment variable. A runbook step may carry `"live": true` (only for `ruletrace-ai`, no golden outputs): `steps_for` leaves it out, so the default runs are unchanged, and no such step is committed. Added `ai/meta.example.toml` (`muse-spark-1.3`, effort `low`, `MODEL_API_KEY`) and `ai/openai.example.toml` (placeholder model `REPLACE_WITH_YOUR_MODEL_ID`, `OPENAI_API_KEY`, no effort because the OpenAI profile does not verify it), loaded by the real `load_config` in `rt-tests-ai/tests/live_examples.rs`. `RUNBOOK.md` has the section "Live steps and document transmission" with `live` blocks. Tests: `rt-demo/tests/live_guard.rs` (refusal before spawn proved with a fake `ruletrace-ai` that leaves a marker and connects to a loopback listener, plus a positive control that runs the same fake for a permitted offline step). Mutants of the guard killed: permission loop removed, guard after spawn, provider root removed, mock-replay classified live, everything offline, boolean accepting the string "true", `may_transmit` ignoring the flag, golden-component rule removed, symlinks not followed, offline runner not classifying, consent check removed, environment check removed, any protocol offline, network fields ignored, absent entry permitted. One survivor, the `golden` root check, is redundant with the golden-component rule (disclosed). Disclosed: three `Allowed` entries in `rt-ai-tool/tests/network_guard.rs` for the host literals of the new example test. Not done: no live call, no real Kiroween or ICC document, no manifest entry for them (task 11.1), no harness command-line program (the harness is a test library).

  - [ ] 10.8 Live recording of provider responses (user-run gate)
    - Prepare the exact commands for the user. First `ruletrace-ai models --config <local meta config>` (the user runs it with their own credential, which no agent requests, reads, prints or logs). Then, only after 10.17 is answered, `ruletrace-ai propose --config <local meta config> --consent <documents> --store target/demo-live/store ...` for the documents confirmed in 1.10, with the request vocabulary frozen by 1.11, the model `muse-spark-1.3`, an explicit low reasoning effort, no `allow_provider_training`, public competition documents and synthetic data only, and an explicit attempt timeout (OD-37-1 is resolved: a 504 is final, so `max_retries = 0` is no longer needed for it). Optionally the same small benchmark with `muse-ember-1.0`, by a separate configuration. No fallback model. Stop and ask the user to run the commands and to report the result. After the user confirms, inspect the batches, run `rt-xtask export-script`, and commit the candidate script only in a deliberate commit that states the provider identifier, protocol, model identifier and capture date text the user supplies, with the new response hashes. Record the observed `max_output_tokens` need and the observed error shapes in `docs/provider-profiles.md`.
    - Observable: `demo/<name>/provider/recorded-script.json` exists with `origin: "recorded"` entries and hashes, and no golden file changed automatically.
    - Blocked by: 1.10, 1.11, 10.6, 10.7, 10.17.
    - _Requirements: 33.8, 33.10, 28.21, 23.27, 35.22_
    - Design §8.11.6, §8.11.7

  - [ ] 10.9 Checkpoint - Phase 8 exit
    - Run the full gate set and confirm that `check-tree` still passes for `rt-cli` and every core crate, that `check-graph` lists `rt-ai-responses` and no `rt-ai-openai`, `rt-ai-meta` or `rt-ai-modal`, and that `check-catalogue` passes.
    - Exit criteria: stub tests (including the Meta profile tests) are green with no external network; the canary tests pass; the user review gate 10.17 is answered; the live job is runnable but not required; the recorded script is committed with its hashes, or the user has deferred the recording.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 11. Phase 9: Final demonstration (real documents, data, published checks, mutation checks, honest claims)
  - [ ] 11.1 Commit the Demo_Corpus documents and manifest
    - Using only the documents the user confirmed in 1.10, commit each as `demo/<name>/documents/<doc-id>.md`, byte-identical to the confirmed text (and, for a converted file, the conversion record of Req 33.5), with the manifest entry of design §8.11.3: title, publisher, address, retrieval text, SHA-256, licence or permission, `transmission_permitted`, the UD-34 confirmation reference and a coverage note. Do not invent, extend or paraphrase document text. Run the manifest test.
    - Observable: the manifest test passes, and altering one byte of a document fails it with the document id and field.
    - Blocked by: 1.10, 6.5.
    - _Requirements: 33.1, 33.2, 33.3, 33.4, 33.5, 28.21_
    - Design §8.11.1, §8.11.3

  - [ ] 11.2 Commit the demonstration Competition_Data and Published_Results
    - Using the answers of 1.13, commit `data/participants.csv`, `collections.csv`, `published.csv`, `published-seeded.csv` and `standings-missing.csv` with manifest entries stating `real` or `synthetic` for each file and `seeded_difference: true` for the seeded one, and the canonical JSON forms. The data holds one eligible and one ineligible participant, one Missing_Value on an eligibility input, one Tie_Group, one Published_Result equal to the computed result and one with a seeded difference. The assertion of these facts is added in 11.6.
    - Observable: the manifest test passes for the data files and labels every file.
    - Blocked by: 1.13, 11.1.
    - _Requirements: 33.13_
    - Design §8.11.10

  - [ ] 11.3 Author the seed Rulebook and coverage notes for the real documents
    - Write `rulebook/seed.json` (declarations and document hashes only, no rules) for the confirmed documents, `coverage-notes.json` (human-authored reasons for clauses that are not rules, labelled authored), and the full runbook with steps s01 to s08, the second evaluate with the seeded Published_Result, the `standings-missing` step and the live steps marked as live. The runbook's machine-readable block is what the acceptance test executes.
    - Observable: the runbook parses, and every offline command in the Markdown is inside the machine-readable block.
    - Blocked by: 11.2, 10.8.
    - _Requirements: 33.7, 33.12, 33.6_
    - Design §8.11.2, §8.11.4, §8.11.12

  - [ ] 11.4 Human review decisions on the real proposals (user-run gate)
    - Show the user the proposals through `ruletrace review list` and stop. The user runs `ruletrace review approve` or `reject` for each proposal with their own Reviewer identity (not an AI identity) and times, and gives a reason for each rejection. Record the exact commands, reviewer and times in the runbook. The ledger is generated only by these commands and is never written by hand. The runbook states that the offline flow replays these recorded decisions.
    - Observable: the committed ledger verifies with the hash chain, and every executable rule has an approval record bound to its Rule_Content_Hash.
    - Blocked by: 11.3.
    - _Requirements: 33.11, 5.8, 26.17, 18.14_
    - Design §8.11.4

  - [ ] 11.5 Generate and bless the goldens for the real corpus
    - Run the offline flow with `RT_DEMO_BLESS=1` (not under `CI`) to write every golden: Source_Models, Candidate_Clauses, Proposal_Store, Rulebook, ledger, Validation_Result, Compiled_Rulebook, Outcomes, Decision_Traces, Proof_Report, application reports, Coverage_Statement, and step transcripts. Explain each golden in the commit message. Do not alter any document text to make classification succeed.
    - Observable: a second run without bless mode reproduces every golden byte-for-byte.
    - Blocked by: 11.4, 9.18.
    - _Requirements: 33.9, 33.6, 33.14, 20.4_
    - Design §8.11.12

  - [ ] 11.6 Write the full demonstration acceptance test and the reproducibility property
    - **Property 48: The offline demonstration is reproducible in every environment**
    - Extend the acceptance test to every assertion of Req 33.15: every artifact equals its golden with the failing step and first differing byte offset reported; expected exit codes; every Decision_Trace Verified; the Proof_Report replays; the data composition of Req 33.13 (an eligible and an ineligible participant, a Missing_Value giving Indeterminate, a Tie_Group, an equal and a seeded-difference Published_Result); the Coverage_Statement; the manifest and script checks; the `ruletrace` dependency tree has no network, provider or AI crate; application results equal `ruletrace evaluate`. Run it under two clocks, two working directories and two locales, with no network and no credential. Required.
    - Observable: the test passes offline on Linux, macOS and Windows, and a deliberately changed golden reports step name, file and byte offset.
    - Blocked by: 11.5.
    - **Validates: Requirements 33.9, 33.12, 33.15, 33.16, 20.26, 20.27, 21.12**

  - [ ] 11.7 Implement the three mutation checks
    - **Property 49: A data mutation changes only the Outcomes that read the value**
    - **Property 50: A source or content mutation is detected and blocks**
    - Implement M1, M2 and M3 of design §8.11.11 in the acceptance test: one byte of a Source_Document gives `PROV_HASH_MISMATCH`, marks the affected traces not verified (exit 2, complete outcomes still written); one data value changes only the Outcomes whose golden trace has a `read` of that participant and field; one approved Rule_Content literal edited without new approval gives `APPROVAL_UNRESOLVED` from `validate` and exit 3 from `compile` with no Compiled_Rulebook (as the RN-29 record says). Required.
    - Observable: all three mutations pass, and a harness bug that skips a mutation fails the test.
    - Blocked by: 11.6.
    - **Validates: Requirements 33.17, 13.18, 25.6, 26.12, 9.13**

  - [ ] 11.8 Implement the claims lists and `gen-claims` and `scan-claims`
    - Commit `claims/required-statements.v1.json` (an identifier for each statement of Req 34 criteria 2 to 6), `claims/prohibited-phrases.v1.json` and `claims/scan-targets.v1` (the demonstration's own documents: README, runbook, reports, goldens and the boundary document; the committed Source_Documents are excluded, RN-28). Implement `rt-xtask gen-claims` and `scan-claims`, which fail with file, line and statement identifier or phrase when a required statement is missing or a prohibited phrase appears. Add negative fixtures for each. Required.
    - Observable: both commands pass on the repository, and each negative fixture fails as specified.
    - Blocked by: 11.5.
    - _Requirements: 34.1, 34.7_
    - Design §8.12

  - [ ] 11.9 Add the honest-claims statements to the reports and the runbook
    - Add to the header of the Proof_Report and of each application report, and to the Demo_Runbook, the statements of Req 34: only capabilities inside the First_Version_Support_Boundary, human approval of the exact revision through `ruletrace review`, a Proposal is untrusted until approval, the offline demonstration replays recorded Reviewer decisions, validation checks internal consistency and not fidelity to the regulation (with the undetected Conflict classes), what verified means, that a live provider can return different proposals, the number and identifiers of Unsupported and Ambiguity clauses (zero stated when none), and that synthetic data is synthetic. Re-bless the goldens and run `scan-claims`.
    - Observable: `scan-claims` passes and every header carries the statement identifiers.
    - Blocked by: 11.8.
    - _Requirements: 34.1, 34.2, 34.3, 34.4, 34.5, 34.6, 33.13_
    - Design §8.12

  - [ ] 11.10 Write the support-boundary document and the demonstration README
    - Create `docs/support-boundary.md` (supported rule kinds, operators, types, rounding modes, the Qualification_Predicate, the UD-32 option that applies, the limits of the Application_Layers of Req 31 and 32, the unsupported list of design §8, the Conflict classes validation cannot detect) and `demo/README.md` (what is real, what is stand-in or synthetic, how to run the offline flow, and the claims of Req 34). No document is described as a full Prize Check or Bracket Check application. Scan both with `scan-claims`.
    - Observable: both files pass `scan-claims`, and the boundary document names every item of Req 14 criteria 1 to 5, 11 and 12 or 13.
    - Blocked by: 11.9, 9.3.
    - _Requirements: 14.8, 34.1, 34.8, 33.12_
    - Design §8.12, §8.13

  - [ ] 11.11 Make `demo-offline` required and add `demo-live`
    - Mark the `demo-offline` job required on the three platforms with the `ai` profile (and the `hand` profile in `core-only`), running with no network and no credential, and a condition of a release tag. Add the non-required `demo-live` job on Linux that runs the live step with a repository secret, writes outside the goldens and never replaces a golden or the script. Add `rt-xtask gen-claims scan-claims check-app-layers check-executables` to the `graph` job.
    - Observable: the workflow file defines both jobs as in design §7.4, and the required-job list includes `demo-offline`.
    - Blocked by: 11.6, 11.8.
    - _Requirements: 20.26, 20.27, 33.15, 33.16, 21.14, 21.3_
    - Design §7.4

  - [ ] 11.12 Apply the remaining steering updates
    - Prepare the patch for the steering text still open after tasks 8.3 and 9.19: design §9.3 item 10 (demonstration documents state the boundary, the meaning of verified, the need for approval and the number of unsupported clauses, enforced by a scan) and the replay sentence of item 7. Show it to the user with the user_input tool and apply it only after confirmation. Item 5 stays open until DD-2 is decided.
    - Observable: steering files carry the confirmed wording, or the task records that the user declined.
    - Blocked by: 11.9.
    - _Requirements: 34.7, 33.8_
    - Design §9.3

  - [ ] 11.13 Checkpoint - Phase 9 and Demo Slice 2 exit
    - Run the full gate set including `demo-offline` on all three platforms and the `rt-xtask` checks (`check-graph`, `check-tree`, `scan-sources`, `check-app-layers`, `check-executables`, `check-catalogue`, `check-justification`, `check-decisions`, `gen-claims`, `scan-claims`).
    - Exit criteria: Properties 48 to 52 pass; the acceptance test passes all assertions of Req 33.15 and 33.17 offline from the committed files; the Proof_Report headers state the limits and the number of Unsupported clauses; application results are byte-identical to `ruletrace evaluate`; the user has run the live flow once and its script was committed deliberately.
    - Ensure all tests pass, ask the user if questions arise.

  - [ ] 11.14 Demonstration checkpoint - Demo Slice 2
    - Run `cargo build --workspace --bins` and `cargo test -p rt-demo --test acceptance -- --nocapture` and show the user the whole story on the real documents: the manifest (source, licence, hash), the recorded proposals with their provider kind and model, the human decisions, the Coverage_Statement, the Outcomes, one Proof_Report section with source text and replayed calculation, the `prize-check` and `bracket-check` reports and their byte identity with `ruletrace evaluate`, the published-result equal and seeded-difference comparisons, and the three mutation results.
    - Observable: the user can follow one decision from the source clause to the proved outcome without trusting an AI statement. Then stop for the user's review before Phase 10.
    - Ensure all tests pass, ask the user if questions arise.

- [ ] 12. Phase 10: Hardening and release
  - [ ] 12.1 Write the no-panic parser property tests
    - **Property 39: Content is data and limits are exact**
    - For `rt-json`, the Rulebook, ledger, Source_Model, compiled artifact, data, proposal store, provider response, AI configuration, Application_Input_Format, manifest and script parsers, arbitrary bytes and mutated valid inputs return a typed result or Diagnostics and never panic, overflow or hang, including size-limit and nesting-depth boundary cases. Required.
    - Observable: no panic, overflow or hang on the generated corpus.
    - **Validates: Requirements 19.1, 19.2, 19.3, 19.4, 19.6, 19.9, 19.11**

  - [ ] 12.2 Write property test for single-fault parse diagnostics
    - **Property 6: Single-fault parse diagnostics**
    - Observable: one injected fault gives exactly one Diagnostic with its specified code and position.
    - **Validates: Requirements 3.8, 3.15, 3.17, 4.2, 4.3, 4.4, 4.7, 4.8**

  - [ ] 12.3 Write property test for three-valued logic
    - **Property 21: Three-valued logic**
    - Observable: AND, OR and NOT agree with the Kleene tables for all operand combinations, including a Fault operand.
    - **Validates: Requirements 11.4, 8.5, 8.6**

  - [ ] 12.4 Write property test for empty aggregation and add the remaining numeric fixtures
    - **Property 22: Empty aggregation**
    - Add the `empty-aggregate` fixture (min and max of an empty collection give Indeterminate with `RT-DGN-0005`, count and sum give 0) and the `thirds` fixture (1/3 weights, fraction output from the confirmed representation) with goldens on three platforms.
    - Observable: the property passes and both fixtures are green.
    - **Validates: Requirements 8.15, 8.29, 8.30, 8.31**

  - [ ] 12.5 Write property test for conflict detection and precedence
    - **Property 27: Conflict detection and precedence**
    - Observable: every injected conflict class is reported with all rules named, and a valid precedence resolves only the three classes of Req 6.20.
    - **Validates: Requirements 6.7, 6.8, 6.16, 6.17, 6.18, 6.19, 6.20, 6.21, 6.23, 12.10, 14.10**

  - [ ] 12.6 Write property test for execution-time conflict handling
    - **Property 28: Execution-time conflict handling**
    - Uses a test-only Compiled_Rulebook built in test code.
    - Observable: incompatible results without precedence give Indeterminate with one `RT-DGN-0004` and exit code 4.
    - **Validates: Requirements 12.5, 12.11, 12.12, 16.23**

  - [ ] 12.7 Write property test for unsupported and incomplete constructs
    - **Property 29: Unsupported and incomplete constructs are explicit**
    - Observable: every construct outside the boundary gives one Unsupported Diagnostic naming its capability, and nothing compiles.
    - **Validates: Requirements 6.4, 6.5, 6.6, 14.6, 14.7, 10.9**

  - [ ] 12.8 Write property test for idempotent and concurrent batch storage
    - **Property 35: Idempotent and concurrent batch storage**
    - Observable: repeated and concurrent runs with one Request_Identifier leave exactly one batch.
    - **Validates: Requirements 29.10, 29.11, 29.17, 29.19, 23.20**

  - [ ] 12.9 Write property test for Run_Summary partitioning
    - **Property 36: Run_Summary partitions the requested clauses**
    - Observable: processed, stored and unprocessed lists partition the requested clauses for every failing position.
    - **Validates: Requirements 23.35, 23.36, 23.37, 23.38, 16.24, 27.8**

  - [ ] 12.10 Write the full mock-provider failure matrix and simulated-timer tests
    - Cover each Provider_Failure kind, each failure of Req 23.5 to 23.8 and 23.33, forbidden-field and instruction responses, Semantic_Outcomes with Ambiguity, Unsupported and Error stored as non-executable, and three-request runs failing the second request with `RT-PRV-0101`, `RT-AI-0106`, `RT-AI-0202` and `RT-AI-0303`. Simulated-timer tests: `Retry-After` that fits and does not fit, exhaustion, deadline, a late response never stored, success after one and two retries with one stored batch, and repeated and concurrent runs.
    - Observable: every cell of the matrix gives its specified code, store state, Run_Summary and exit code.
    - _Requirements: 20.7, 20.8, 20.9, 23.17, 23.33, 23.38, 29.14_

  - [ ] 12.11 Write a negative test for every provokable catalogue code
    - One test per `RT-VAL` and other catalogue code that can be provoked, asserting code, Severity, primary Location and exit code, with every Location variant and the four conflict classes. A test fails if a provokable code has no negative test.
    - Observable: the coverage check lists no untested provokable code.
    - _Requirements: 6.1, 6.2, 6.3, 20.3, 20.12, 20.13_

  - [ ] 12.12 Commit the catalogue baseline and enable the baseline check
    - Write the first-release `catalogue/baseline.v1.json` and enable `check-catalogue` to fail on a removed, renumbered or severity-changed code.
    - Observable: removing one catalogue row makes `check-catalogue` fail naming the code.
    - Blocked by: 12.11.
    - _Requirements: 7.11, 7.12, 7.16, 7.17_
    - Design §4.3

  - [ ] 12.13 Add the support-boundary consistency test and the security-limit document
    - Create `docs/security-limits.md` (R-8 reviewer authentication, no cross-filesystem guarantee, residual ledger limits of Req 26.22). Add a test that every rule kind, node name, rounding mode and unsupported construct known to the validator appears in `docs/support-boundary.md`, and that both documents exist.
    - Observable: adding a node name to the validator without documenting it fails the test.
    - Blocked by: 11.10.
    - _Requirements: 14.8, 14.9, 6.22, 26.22, 20.1_
    - Design §8, §9.1

  - [ ] 12.14 Final full-matrix gate verification
    - Verify the required jobs in `ci.yml`: `fmt`, `clippy`, `test`, `golden`, `toolchain`, `demo-offline` on three platforms and `graph`, `deny`, `core-only` on Linux. Verify `tools/allowed-edges.v1` covers every crate of design §3.1 and that every `tools/decisions/*.md` referenced by `tools/decision-gates.v1` is `Confirmed`. List any decision that is still `Pending` or `Deferred` in the final output.
    - Observable: a printed list of the required jobs and of the open decisions.
    - Blocked by: 12.12, 12.13, 11.11.
    - _Requirements: 21.3, 21.4, 21.9, 21.10, 21.12, 24.9, 20.26_
    - Design §7.4

  - [ ] 12.15 Final checkpoint - all required jobs green
    - Run the full gate set locally. Exit criteria: all required jobs green; baseline committed; boundary documents present; the release checklist of Req 20.26, 14.8 and 34.8 satisfied. Tagging a release is not a coding task and is left to the user.
    - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional breadth tests (2.8, 4.11) and can be skipped for a faster path. Breadth tests that every release needs are not starred and sit in Phase 10 (12.1 to 12.13). Each of the 52 design properties appears in at least one task.
- Task 10.4 was one of the optional breadth tests until UD-37 and is now required (the flat Proposal schema is what the provider receives).
- Required and never optional, because they protect the named invariant: determinism (Properties 2, 13, 24, 48), isolation (Property 40, tasks 5.27, 9.17), provenance (Properties 1, 8, 25, 26, 33, 50), approval gating (Properties 9, 10, 11, 12, 14, 15, 16, 34 and the no-other-channel test 6.4), diagnostic state (Properties 4, 5), no network in core (Property 41, `check-tree`, source scans), exact numeric (Properties 17, 18, 19 and the Req 8.28 tables), application byte identity (Property 42), no duplicated business logic (Property 43 and `check-app-layers`), demonstration reproducibility (Property 48, task 11.6) and the three mutation checks (11.7, smoke in 6.9).
- Moved to Phase 10 from the gate sets of design §7.5: Properties 6, 21, 22, 27, 28, 29, 35, 36, the full mock failure matrix, the per-code negative tests and the `empty-aggregate` and `thirds` fixtures. Breadth moved out of the slices: RT-JSON differential tests (2.8) and Property 3 (4.11) are optional.
- Slice gates differ from design §7.5 in two places, both only in placement: UD-23 and UD-27 gate `rt-review`, which now starts in Phase 4 (so they are asked at wave 0), and the contradictory-flag property test (Property 5) is built up over tasks 4.12, 7.8 and 9.12 as new input kinds appear.
- Phase 7 depends only on Phase 4 (design §7.5) and could run before Phases 5 and 6. The plan runs the AI slice first so that Slice 1 is a demonstration of its own, and the gates 1.12 and 1.13 are asked after Slice 1.
- A gate is not Confirmed until the user answers. A task that needs a gate lists it under `Blocked by`. The Pending records are checked mechanically by `rt-xtask check-decisions` (task 2.10).
- `tools/allowed-edges.v1` and `tools/decision-gates.v1` are written once (tasks 2.9 and 2.1). Other shared `Cargo.toml` and `tools/` files are touched by tasks in different waves. Property tests live in separate files under `crates/rt-tests-core/tests/`, so tests in the same wave do not collide.
- Checkpoint tasks, demonstration checkpoints and top-level parent tasks are not part of the dependency graph.

## Task Dependency Graph

```json
{
  "waves": [
    {"id": 0, "tasks": ["1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "1.7"]},
    {"id": 1, "tasks": ["2.1"]},
    {"id": 2, "tasks": ["2.2", "2.3", "2.4"]},
    {"id": 3, "tasks": ["2.5"]},
    {"id": 4, "tasks": ["2.6", "2.7", "2.13"]},
    {"id": 5, "tasks": ["2.8", "2.9", "2.12", "4.1"]},
    {"id": 6, "tasks": ["2.10", "4.2"]},
    {"id": 7, "tasks": ["2.11", "3.1", "4.3"]},
    {"id": 8, "tasks": ["3.2", "4.10"]},
    {"id": 9, "tasks": ["3.3"]},
    {"id": 10, "tasks": ["3.4"]},
    {"id": 11, "tasks": ["3.5", "4.4", "5.6", "5.7"]},
    {"id": 12, "tasks": ["3.6", "3.7", "3.8", "4.5", "5.11"]},
    {"id": 13, "tasks": ["4.6", "4.11", "5.24"]},
    {"id": 14, "tasks": ["4.7", "4.13", "4.14", "5.5"]},
    {"id": 15, "tasks": ["4.8", "4.16", "5.9"]},
    {"id": 16, "tasks": ["4.9", "4.12", "4.15", "5.1", "5.10", "5.12", "5.20", "5.22", "5.23"]},
    {"id": 17, "tasks": ["4.17", "5.2", "5.21"]},
    {"id": 18, "tasks": ["5.3", "6.5"]},
    {"id": 19, "tasks": ["5.4", "5.8", "6.1"]},
    {"id": 20, "tasks": ["5.13", "5.15", "5.16", "5.18", "5.25"]},
    {"id": 21, "tasks": ["5.14", "5.17", "5.19", "5.26", "6.2"]},
    {"id": 22, "tasks": ["5.27", "6.3", "6.4"]},
    {"id": 23, "tasks": ["6.6"]},
    {"id": 24, "tasks": ["6.7"]},
    {"id": 25, "tasks": ["6.8"]},
    {"id": 26, "tasks": ["6.9"]},
    {"id": 27, "tasks": ["6.10"]},
    {"id": 28, "tasks": ["1.8", "1.9", "1.10"]},
    {"id": 29, "tasks": ["1.11", "7.1"]},
    {"id": 30, "tasks": ["7.2", "7.8"]},
    {"id": 31, "tasks": ["7.3"]},
    {"id": 32, "tasks": ["7.4", "7.6", "7.7"]},
    {"id": 33, "tasks": ["7.5"]},
    {"id": 34, "tasks": ["8.1"]},
    {"id": 35, "tasks": ["8.2", "8.3", "8.10"]},
    {"id": 36, "tasks": ["8.4"]},
    {"id": 37, "tasks": ["8.5", "8.13", "8.14"]},
    {"id": 38, "tasks": ["8.6", "8.9"]},
    {"id": 39, "tasks": ["8.7", "8.11", "8.12"]},
    {"id": 40, "tasks": ["8.8"]},
    {"id": 41, "tasks": ["8.15", "8.16"]},
    {"id": 42, "tasks": ["8.17"]},
    {"id": 43, "tasks": ["8.18"]},
    {"id": 44, "tasks": ["1.12", "1.13", "9.1", "9.2", "9.4", "10.4", "10.6", "10.10", "10.11", "11.1"]},
    {"id": 45, "tasks": ["9.3", "9.5", "9.9", "10.1", "10.12", "10.14", "11.2"]},
    {"id": 46, "tasks": ["9.6", "10.2", "10.13"]},
    {"id": 47, "tasks": ["9.7", "9.15", "10.3"]},
    {"id": 48, "tasks": ["9.8", "9.10", "9.11", "9.12", "9.13", "9.14", "9.17", "10.15", "10.16"]},
    {"id": 49, "tasks": ["9.16", "9.19", "10.5"]},
    {"id": 50, "tasks": ["10.7", "10.17"]},
    {"id": 51, "tasks": ["10.8"]},
    {"id": 52, "tasks": ["9.18", "11.3"]},
    {"id": 53, "tasks": ["11.4"]},
    {"id": 54, "tasks": ["11.5"]},
    {"id": 55, "tasks": ["11.6", "11.8"]},
    {"id": 56, "tasks": ["11.7"]},
    {"id": 57, "tasks": ["11.9", "12.1", "12.2", "12.3", "12.4", "12.5", "12.6", "12.7", "12.8", "12.9", "12.10", "12.11"]},
    {"id": 58, "tasks": ["11.10", "11.11", "11.12", "12.12"]},
    {"id": 59, "tasks": ["12.13"]},
    {"id": 60, "tasks": ["12.14"]}
  ]
}
```
