# RN-52 ownership audit: shared Workflow_Module code

Date: 2026-11-18. Required by UD-40 section 2 before task 9.21 moves code. Read-only analysis of `crates/prize-check/src` and `crates/bracket-check/src` at commit `5670e59`. Line numbers are those of `prize-check`; `bracket-check` lines are equal where the files are identical.

Rules applied (UD-40): no generic common crate; `rt-data` owns preparation, parsing and data mapping; `rt-explain` owns result interpretation, explanation and reports; no move that needs a dependency edge or a Req 24.12 amendment without the user's decision; no path that lets a crate other than `rt-engine`'s callers reach an evaluation function; the Explainer must not depend on the engine (design §3.1: "The Explainer needs trace types and must not depend on the engine"; `tools/allowed-edges.v1` lists no such edge). Correction after review 9.21 F3: Req 15.9 states the reverse ban (the engine must not depend on the Explainer); the conclusion is unchanged.

Current direct edges (`tools/allowed-edges.v1`):

| Crate | Direct internal dependencies |
| --- | --- |
| rt-data | rt-foundation, rt-json, rt-numeric |
| rt-explain | rt-foundation, rt-srcmodel, rt-trace, rt-json, rt-numeric |
| rt-compiled | rt-foundation, rt-numeric, rt-hash, rt-json, rt-ir |
| rt-engine | rt-foundation, rt-numeric, rt-compiled, rt-data, rt-trace |
| prize-check, bracket-check | rt-foundation, rt-numeric, rt-data, rt-compiled, rt-engine, rt-explain, rt-srcmodel |

## 1. Duplicated items

Status: **I** byte-identical, **T** identical modulo type names, **P** partly shared.

| # | Item (prize-check file:line) | St. | Inputs → outputs | Responsibility | Uses |
| --- | --- | --- | --- | --- | --- |
| 1 | `NamedFile` (mapping.rs:19) | I | struct `{bytes: &[u8], name: &InputId}` | one input file and its Diagnostic name | rt-foundation |
| 2 | `FileInputs` (mapping.rs:28) | I | struct of participants, optional collections and published `NamedFile`, `&MappingOptions` | the input files of a run | rt-data |
| 3 | `Mapped` (mapping.rs:41) | I | `{CompetitionData, Option<MappedPublished>}` | result of mapping | rt-data |
| 4 | `internal` (mapping.rs:46) | I | `String → Diagnostic` | InternalError Diagnostic | rt-foundation |
| 5 | `field_type` (mapping.rs:54) | I | `(&CompiledRulebook, CType) → Option<FieldType>` | compiled type → mapping field type | rt-compiled, rt-data |
| 6 | `input_schema` (mapping.rs:74) | I | `&CompiledRulebook → Result<InputSchema, Box<Diagnostic>>` | input and collection schema of a Compiled_Rulebook | rt-compiled, rt-data |
| 7 | `kind_of` (mapping.rs:100) | I | `&CompiledBody → TargetKind` | rule body → Target kind | rt-compiled, rt-data |
| 8 | `target_kinds_by_name` (mapping.rs:113) | I | `&CompiledRulebook → BTreeMap<String, TargetKind>` | kind per Target name | rt-compiled, rt-data |
| 9 | `target_kinds` (mapping.rs:132) | I | `&CompiledRulebook → Result<TargetKinds, Box<Diagnostic>>` | item 8 as `rt-data` wants it | rt-compiled, rt-data |
| 10 | `parse` (mapping.rs:146) | I | `NamedFile → Result<Table, DiagnosticState>` | CSV parse with the default limit | rt-data |
| 11 | `map_files` (mapping.rs:156) | I | `(&InputSchema, &TargetKinds, &BTreeSet<TargetName>, &FileInputs, &mut DiagnosticState) → Option<Mapped>` | map the three files, collecting every fault | rt-data |
| 12 | `ResultClass`, `ResultClass::of` (classify.rs) | I | `(&[Outcome], &DiagnosticState, &[Verification]) → ResultClass` | wraps `rt_explain::EvaluationClass` and adds Internal | rt-explain, rt-trace |
| 13 | `class_of_run` (classify.rs) | I | `(outcomes, diagnostics, verifications, report_built: bool) → ResultClass` | Internal when the report was not built | rt-explain |
| 14 | role consts (prepare.rs:16) | I | `&str` | names of the input-hash roles | — |
| 15 | `Rejection` (prepare.rs:22) | I | enum `{InvalidInput(DiagnosticState), Internal(Box<Diagnostic>)}` | why Prepare produced no Questions | rt-foundation |
| 16 | `Prepared` and accessors (prepare.rs:57) | I | data, Questions, published, unused rows, input hashes; byte accessors | the inputs of one engine call | rt-data, rt-explain (hash type) |
| 17 | `hash_of`, `input_hashes` (prepare.rs:116) | I | `(role, NamedFile) → (role, rt_explain Sha256Hex)` | input hashes for the report header | rt-compiled (`sha256_of`), rt-explain |
| 18 | `prepare` (prepare.rs:152) | T | `(&CompiledRulebook, &Request, &FileInputs) → Result<Prepared, Rejection>` | orchestrates 6, 9, 11, 17 and the app's own request check | all of the above plus app code |
| 19 | `*Inputs` (evaluate.rs:20) | T | struct | run inputs | — |
| 20 | `Evaluated` (evaluate.rs:222) | I | holds `Prepared`, `rt_engine::Evaluation`, verifications, report result | a run that reached the engine | rt-engine |
| 21 | `evaluate` (evaluate.rs:274) | T | `&Inputs → Result<Evaluated, Rejection>` | Prepare, **`rt_engine::evaluate`**, verify, present | rt-engine |
| 22 | `PresentInputs` (report.rs:28) | I | borrows `&CompiledRulebook`, Outcomes, traces, verifications, Diagnostics, `Option<&rt_engine::Comparison>`, unused rows, hashes, statements, synthetic flag | everything Present reads | rt-compiled, rt-engine, rt-explain |
| 23 | `PresentError` (report.rs:56) | I | enum wrapping `rt_explain::report::ReportError` | why no report was built | rt-explain |
| 24 | `published_text` (report.rs:124) | I | `&rt_data::PublishedValue → String` | canonical text of a published value | rt-data, rt-numeric |
| 25 | `computed_text` (report.rs:131) | I | `&rt_engine::Difference → Result<Option<String>, PresentError>` | canonical text of a computed value | rt-engine |
| 26 | `published_section` (report.rs:142) | I | `(Option<&rt_engine::Comparison>, &[UnusedPublished]) → Result<PublishedSection, PresentError>` | the Published section of a report | rt-engine, rt-data, rt-explain |
| 27 | `run_diagnostics` (report.rs:177) | I | `&PresentInputs → Vec<Diagnostic>` | evaluation and verification Diagnostics, de-duplicated | rt-explain |
| 28 | `present` (report.rs:198) | P | `&PresentInputs → Result<ReportDoc, PresentError>` | shared: count and outcome-versus-trace checks, `TraceRef`, Published and Diagnostics sections, header; app-specific: kind-to-section dispatch, `ReportApplication`, the standings flag | rt-explain, rt-compiled |
| 29 | questions.rs (`Request`, `check_group`, `check_request`, `build_questions`) | — | | **app-specific** (options, Targets reported, Question order) | stays |

## 2. Owner per group

### Group A: move to `rt-data`, no new edge

Items 1, 2, 3, 10, 11 (and the `internal` helper they use, item 4, as a private function there). They use only `rt-data` and `rt-foundation` types. Proposed home: `rt_data::tabular` (`NamedFile`, `FileInputs`, `MappedFiles`, `map_files`). Responsibility: data mapping. Direction: apps → rt-data (exists). No cycle (rt-data depends on no app, no engine, no explain). No evaluation path (rt-data cannot reach `rt-engine`; `rt-engine → rt-data` exists, so the reverse is impossible by construction).

### Group B: move to `rt-explain`, no new edge

- Items 12, 13: `ResultClass` and `class_of_run` next to `classify_evaluation` (`rt_explain::classify`), with `rt_trace::Outcome` in the signature. Responsibility: result interpretation.
- Items 23, 27 and the shared part of item 28: a generic report assembler in `rt_explain::report` that takes plain values only: the Outcomes, traces and verifications (rt-trace and rt-explain types), the Diagnostic list, a `&BTreeMap<String, TargetKind>`-like map **of an rt-explain-owned enum** of reportable section kinds, the rulebook hash as `&str`, the input hashes, the statements, the synthetic flag, the application, an already-built `PublishedSection`, and a callback or per-kind table that maps each trace to its app section. It performs the count check, the outcome-versus-trace check, `TraceRef`, the Diagnostics section, the header and `ReportDoc::new`. The apps keep the kind-to-section choice (item 28's dispatch) and build `TargetKind` from the Compiled_Rulebook themselves.
- Direction: apps → rt-explain (exists). No cycle. No engine path: rt-explain gains no edge, so it still cannot reach `rt_engine::evaluate` (Req 15.9).

### Group C: needs a decision (fits an owner only with a new edge)

Items 5 to 9 (`field_type`, `input_schema`, `kind_of`, `target_kinds_by_name`, `target_kinds`): preparation, so the semantic owner is `rt-data`, but they read `rt_compiled::{CompiledRulebook, CType, CompiledBody}`.

| Option | Edge | Cycle? | Evaluation bypass? | Other cost |
| --- | --- | --- | --- | --- |
| C1 | `rt-data → rt-compiled` | No: rt-compiled depends on foundation, numeric, hash, json, ir only | No: rt-compiled contains no evaluation | changes the rt-data line of `allowed-edges.v1` and the existing assertion of `rt_data_depends_on_no_engine_or_compiler_crate` (`crates/rt-data/src/tabular/tests_map.rs`), which needs the user's explicit approval because it changes an existing test expectation; rt-data then reaches rt-ir transitively |
| C2 | none: keep items 5 to 9 in both apps | — | — | about 75 identical lines stay duplicated, guarded by the drift test |
| C3 | none: `rt-data` exposes constructors that take plain schema data, and each app keeps a thinner adapter (about 30 lines) from `CompiledRulebook` | — | — | duplication shrinks but does not vanish |

Req 24.12 is not affected by C1 (it governs the Application_Layer edges, which do not change). Recommendation: **C1**, because the schema of a Compiled_Rulebook as mapping input is a data-mapping responsibility and the edge points from a lower-stage types crate to another types crate. It needs the user's approval of the edge and of the changed assertion.

### Group D: needs a decision (engine-typed result adapters)

Items 24 to 26 (`published_text`, `computed_text`, `published_section`) and the `Option<&rt_engine::Comparison>` member of item 22: result interpretation, so the semantic owner is `rt-explain`, but they read `rt_engine::{Comparison, Difference, Computed}`.

| Option | Change | Cycle? | Evaluation bypass? | Assessment |
| --- | --- | --- | --- | --- |
| D1 | `rt-explain → rt-engine` | No cycle today, but `forbid-name rt-engine -> rt-explain` shows the intended layering | **Yes**: rt-explain could call `rt_engine::evaluate`; Req 15.9 forbids the Explainer to depend on the engine | **Rejected** |
| D2 | move `Comparison`, `Difference`, `Computed`, `DifferenceKind` from `rt-engine` into `rt-trace` (result types), re-exported by `rt-engine` unchanged | needs `rt-trace → rt-data` (they hold `ParticipantId`, `TargetName`, `PublishedValue`); no cycle (rt-data depends on no trace crate) | No | a public-type move in two core crates; behaviour unchanged; needs the user's approval of the edge |
| D3 | keep the adapter in both apps; it converts `Comparison` into the `DifferenceFacts` that rt-explain already owns, and the generic part (building `PublishedSection` from facts) moves to rt-explain | none | No | about 35 identical lines stay duplicated |

Recommendation: **D3 now** (edge-free, removes the larger part), D2 only if the user wants zero duplication.

### Group E: fits neither existing crate

Items 14 to 21 (`Rejection`, `Prepared`, `hash_of`, `input_hashes`, `prepare`, `*Inputs`, `Evaluated`, `evaluate`): the orchestration of one Workflow_Module run. `evaluate` calls `rt_engine::evaluate`; `rt-data` cannot (cycle) and `rt-explain` must not (Req 15.9). `Prepared` holds an rt-explain hash type and is built from rt-compiled hashing, so neither crate can hold it without a new edge either.

Alternatives for the user:

| Option | Assessment |
| --- | --- |
| E1 | Keep in both apps, thinner after groups A, B (and C, D): each app's `prepare` and `evaluate` become short calls of the shared functions plus its own request check. Remaining duplication: the struct definitions and about 40 lines of orchestration, guarded by the drift test. |
| E2 | Make the orchestration generic over the request type in one place. Every candidate place is either an app (forbidden edge between the apps), a core crate that may not call the engine, or a new crate. **No crate is created by assumption** (UD-40); E2 needs the user's decision on a new crate and a Req 24.12 amendment. |

Recommendation: **E1**.

## 3. Proposed task 9.21 scope

- **Part 1 (no decision needed):** groups A and B, and the edge-free part of D (D3). Consumers keep their public names by re-export, so the integration tests compile unchanged. Focused tests of the shared functions are added in `rt-data` and `rt-explain`; the existing app tests stay as consumer tests. The drift guard stays until both are in place, then is narrowed to whatever stays duplicated.
- **Part 2 (after the user's decision):** C1 (or C2/C3), D2 (or D3), E1 (or E2).

## 4. Checks for every move

- `rt-xtask check-graph check-tree scan-api` pass with **unchanged** `tools/allowed-edges.v1` in Part 1.
- No existing assertion, golden or fixture changes in Part 1.
- The source scans of `prize_check_no_io.rs` and `bracket_check_no_io.rs` still find exactly one `rt_engine::evaluate` call per app.
- rt-explain still has no edge to rt-engine (`forbid` rules and Req 15.9).

## 5. Final ownership (UD-41)

- **Group C (C1) moved to `rt-data`.** `input_schema`, `target_kinds` and `target_kinds_by_name` are public in `rt_data::tabular` (`crates/rt-data/src/tabular/compiled.rs`). `field_type` and `kind_of` are private there. The edge `rt-data -> rt-compiled` is in `tools/allowed-edges.v1`, together with `forbid-transitive rt-data -> rt-engine rt-compile rt-validate rt-explain rt-review rt-cli rt-ai-*`. The module reads declarations only; it has no evaluation path. The assertion `rt_data_depends_on_no_engine_or_compiler_crate` now allows `{rt-compiled, rt-foundation, rt-json, rt-numeric}` (approved by UD-41). Tests: `crates/rt-data/src/tabular/tests_compiled.rs`. The internal-error branch for a Ranking-typed input stays as defence in depth. It cannot be reached with a valid Compiled_Rulebook, because `rt-compiled` refuses such a declaration (`check.rs`), and the test shows the refusal.
- **Group D (D3) stays in the apps.** The comparison adapters are in each app's `report.rs` (145 lines in prize-check, 144 in bracket-check, adapters only). The engine owns the comparison.
- **Group E (E1) stays in the apps.** One `rt_engine::evaluate` call per app. Remaining orchestration: `prepare.rs` 184 lines, `evaluate.rs` 122 lines, `questions.rs` 150 and 145 lines, each app. `mapping.rs` (17 lines, `internal` and the re-exports) and `classify.rs` (13 lines) are still byte-identical.
- **Drift guard.** `crates/rt-tests-core/tests/workflow_module_copies.rs` is unchanged: `mapping.rs` and `classify.rs` still exist and are identical, so both stay in `COPIED`. The `cfg(test)` check is kept. `prepare.rs`, `report.rs`, `questions.rs` and `evaluate.rs` differ between the apps and are not guarded.
