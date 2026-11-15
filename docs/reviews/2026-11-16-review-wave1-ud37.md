# Wave 1 of UD-37: flat Proposal schema, catalogue codes, capability types, Proposal_Store version 2

Commits `ac42ecf` (10.4), `67b16a9` (10.10), `1b0e5f9` (10.11) and `4c28650` (10.14), reviewed against UD-37, Requirements 35 and 23.39, the design sections, and the testing-integrity and architecture steering. All builds and tests ran in a separate worktree at `4c28650` with its own target directory. The worktree was removed afterwards, and no file in the main tree changed except this one.

The wave does what UD-37 asks. The schema is measured and within Meta's documented limits. The catalogue rows, `negotiate()`, the request identifier and the version 2 store reader and writer all check out, and eight mutants gave seven kills and one boundary survivor. One real defect crosses tasks 10.4 and 10.14: a flat response with an expression about 61 levels deep or more is accepted and stored, and the stored file can then not be read back. `check_store`, `ruletrace review` and every later `ruletrace-ai propose` run treat an unparseable batch as an invalid store, so one such response blocks the whole Proposal_Store.

Watch for: the stored-batch depth defect, **confirmed** by a probe (details below). The flat path also opens deep decoder recursion to model-controlled input, **possible**, and the schema sits exactly at the depth limit of 10 with no margin.

**Verdict**: NEEDS_CHANGES

## High-level view

The schema generator produces one provider-neutral node table. Measured independently, it has 535 properties, depth 10 (when `anyOf` alternatives, `items` and properties each count as a level), no `$ref`, closed objects with `required` equal to their properties, and a plain object at the root. The lowering of that table back to the nested rule text is checked for index range, ordering (so no cycle), and single naming (so a tree). It uses an explicit work list, and it feeds the same decoder, provenance and validation gates as the nested form.

The catalogue rows match UD-37 section 5 exactly (code, mnemonic, severity Error, exit 2 or 5), and the requirement and design tables agree with them. `negotiate()` is pure and fails closed on Unknown. Requests without a reasoning effort keep their old canonical bytes and identifier, which I recomputed outside the code base.

The store reader and writer for version 2 are strict, and a version 1 batch is read and printed unchanged. Reported metadata is stored in its own members and nothing reads it for identities, approval or decisions. The one gap is the interaction between the flat schema, which lifts the response's container limit of 32 to the decoder's 512, and the store reader's container limit of 64.

<details>
<summary>Issues (6)</summary>

1. **Stored batch unreadable at expression depth about 61 or more (blocking, confirmed)** — a flat response is accepted up to the decoder cap of 512 and stored, but `parse_batch` refuses files nested deeper than 64 containers, which makes the whole store invalid. Cap the depth at acceptance (or emit a refusal diagnostic) so every accepted batch reparses, and add a regression test that stores and reparses a deep batch.
2. **Deep table recursion on untrusted input (non-blocking, possible)** — in a debug build, `accept_batch` overflowed a 1 MiB stack at depth 200 and a 2 MiB stack at depth 300; 4 MiB and 8 MiB were fine up to 500. Windows main threads default to 1 MiB. The nested path was capped at 32 by the response parser. A depth cap before lowering would also fix issue 1.
3. **Out-of-range boundary untested (non-blocking, confirmed)** — mutant `r.index >= count` to `r.index > count` survives. The refusal still happens at the same position but with the message "named more than once". Add a test with index equal to the node count that asserts "out of range".
4. **Schema depth has no margin (non-blocking, possible)** — depth is exactly 10 under the strictest counting. If Meta counts one more wrapper (for example the `schema` envelope), the first live call returns HTTP 400. The task 10.15 stub cannot detect this. Observe it in the live run, or flatten one level (for example move `markers` or `spans` entries into a shared position).
5. **Reported text filter covers only control characters (non-blocking, possible)** — `char::is_control` is Unicode category Cc, so bidi override and other format characters in `reported_model` or `response_id` pass. No reader displays them yet. Consider `escape_controls`-style handling or a printable-only rule before a command shows them.
6. **Over-long doc line (non-blocking, confirmed)** — `crates/rt-ai-adapter/src/request.rs`, the `canonical_bytes` doc comment is about 100 characters, above the 80-character comment rule. `rustfmt` does not reflow comments.

</details>

<details>
<summary>Details</summary>

## Flat schema and lowering (`ac42ecf`)

**Measured against Meta's limits.** I walked `tests/fixtures/proposal-schema/proposal-schema-v2.json` with a separate Python script that shares no code with the generator or the in-repo measurer. It found 707 schema nodes and 535 properties (limit 5,000). It counted 3,567 characters of property names and enum values (limit 120,000), 207 enum values in total, and a largest enum of 6 (limit 1,000 each). There is no `$ref`, `$defs`, `oneOf` or `allOf`, so the expanded size equals the literal size of 31,706 compact bytes, far below 200,000 nodes. The root is `type: object` with `additionalProperties: false` and has no `anyOf`. Every object is closed, and `required` equals the property names for every object. The only keywords used are `type`, `additionalProperties`, `required`, `properties`, `items`, `anyOf` and `enum`.

The depth claim holds. With the root as level 1 and every property, `items` and `anyOf` alternative as one level, the maximum is exactly 10. Counting only instance nesting gives 8. A depth of exactly 10 leaves no margin against a vendor that counts differently, which is issue 4. I did not have a JSON Schema validator available (no `jsonschema` package), so conformance of sample documents rests on the in-repo conformance checker `conforms` in `tests_flat.rs`, which has its own negative test.

**Depth 16 round trip.** `tests_flat.rs` writes the nested text and the flat table from one small expression model (`E`) that is separate from the lowering, then stores both through `accept_batch` and compares the stored draft, the content hash and the findings. Spine depths 16 and 17 and a property test are included. Depth 17 yields exactly `RT-LIM-0003`, as the nested form does. The comparison runs the production nested path as its oracle for the hash, which is acceptable here because the claim is equivalence of the two paths. It relies on the nested path having been tested earlier.

**Lowering against hostile tables.** The ordering rule (a child index must be greater than its parent's) rules out cycles, the single-naming rule rules out shared nodes, and indices are checked against the table length before use. `u64` to `usize` conversion is checked, and `seen` is sized by the node count. Mutants confirm that the ordering rule (3 tests fail) and the naming rule (2 tests fail) are tested. A 20,000-node chain is refused without overflow. Output is linear in the table and the table is bounded by the 1 MiB response limit. The work list means the lowering itself does not recurse.

After lowering, the text goes to `parse_draft` and so through the same decoder, `RT-LIM-0003`, provenance and validator as before. Duplicate keys are refused earlier by the response parser (`first_duplicate`). A version 2 response with a nested rule and the reverse are refused (tested). The lowering drops a null `t` or `type`, which is the only change to the rule text; `t` is a string in the schema, so a conforming provider cannot send null anyway.

The defect is the missing depth cap on this path (issue 1). The nested form was held to 32 containers by `MAX_DEPTH`, while the flat form is parsed with `DECODER_DEPTH_CAP` (512). My probe (a temporary test, not committed) accepted a `not` chain of depth 61, 100, 300 and 500 through `accept_batch` with outcome `RT-LIM-0003`, stored each with `store_batch`, and reparsed the file with `rt_proposals::parse_batch`. All four failed with "nests containers 65 deep ... deeper than the effective limit of 64". Depths 30 and 60 reparsed. `rt-cli/src/review.rs` states that an unparseable batch makes the whole store invalid, and `check_store` stops at the first malformed batch. The existing test `a_flat_table_deeper_than_the_nested_container_limit_still_reaches_the_validator` asserts acceptance at 34 containers and so passes while the effect at 61 or more goes unchecked. A proper regression test stores the batch and reparses it. Per section 4 of the testing policy, it should fail on `4c28650` (it does, per the probe) and pass after the fix.

Stack depth: the same acceptance path at 1 MiB (debug) aborted at depth 200 and at 2 MiB at depth 300, with 4 MiB passing at 500. The manual-rulebook path has the same decoder, so this is not new code, but model-controlled input now reaches it (issue 2).

**The edited tests.** `schema_version_must_be_one` became `schema_version_must_be_one_or_two` with the rejected values `"2"`, `""`, `"1.0"` changed to `"3"`, `""`, `"1.0"`. The change follows from Requirement 23.39 (version "2" is now valid). The test still asserts the same code and field for each rejected value, and `only_versions_1_and_2_are_accepted` in `tests_flat.rs` covers the accepted side. The property test `BadVersion` moved from `"2"` to `"3"` for the same reason. I judge both edits legitimate and not weakening, and both are disclosed in the comment and the commit.

## Catalogue entries (`67b16a9`)

The five rows (RT-AI-0011, 0012, 0013, 0109, 0110) have the mnemonics, severity Error and exit codes of UD-37 section 5 (2, 2, 2, 5, 5) in `catalogue/diagnostics.v1.json`. `Code::ALL` has 152 entries and each new identifier appears once. The new test `catalogue_ud37.rs` writes its expectations from UD-37, not from the catalogue, and also pins four neighbouring rows. The requirement table and the design table were edited only to drop the "task 10.10" remarks. `check-catalogue` passes with 152 codes. No second code was introduced for a failure mode that already had one.

## `negotiate()`, identity and `ProviderMeta` (`1b0e5f9`)

`negotiate` accepts a required capability only when the available value is `Yes`, so `Unknown` and `No` both fail closed. The result is a name-ordered list and the function has no clock, file, environment or network input. Mutant M1 (treat `Unknown` as met) fails four capability tests.

The `ProviderMeta` change replaces `provider: String` by `identity: Identity` and adds `#[non_exhaustive]` with a constructor. A `compile_fail` doctest covers the struct literal. In the test diff, the only assertion edits are `meta.provider` to `meta.identity.provider.as_str()` with the same expected strings (`"mock"` and `"mock-replay"`) in `rt-ai-mock/src/tests.rs` and `tests_replay.rs`. All other test edits are constructor migrations and `reasoning_effort: None` fields. No assertion was removed or relaxed.

The request identifier `b98f2750...7a1f` of the existing pinned test is unchanged, because the same literal existed in the test before the commit, and I recomputed it with `sha256` over the canonical bytes outside the code. The new identifier with `"reasoning_effort":"low"` also recomputed to `76dbc119...7911`. The effort enters the bytes only when set, after `limits` and before the context, and a context key that repeats it gives no bytes. Mutant M6 (always emit) fails two tests. Mock capabilities are `structured_output: Yes`, `reasoning_effort: No`, `usage_reporting: No`, so a run that requires an effort fails closed against the mocks.

## Proposal_Store version 2 (`4c28650`)

The reader accepts versions 1 and 2 only, takes the optional members only in version 2 (mutant M5, which lets version 1 carry them, fails `a_version_2_member_in_a_version_1_batch_is_unknown`), refuses unknown members, requires 1 to 256 characters without a control character for each reported text, and requires at least one count in `usage`. The canonical form check is retained, so member order is enforced. `Format::V1` cannot hold an `Extras`, so an inconsistent pair cannot be built.

The two version 1 goldens under `tests/fixtures/proposal-store-v1/` are byte copies of the demo batches at `1b0e5f9` (`cmp` equal for both). Their reader tests pass and the printer returns them byte for byte. In the demo goldens, a JSON comparison shows exactly two differences for each of the two files: `version` 1 to 2 and a new `protocol` equal to `mock-replay`. Nothing else in the demo directory changed, and the recorded script and request identifiers are untouched.

Reported metadata is read by `Reported::from_meta` and consumed only by `batch_of` in `store.rs`. A search over all non-test sources finds no other reader of `reported_model`, `response_id` or `usage`. The AI identities for the batch come from the configuration (`StoreMeta.ai_identities`), and `rt-review/src/adopt.rs` and `rt-ledger/src/reviewer.rs` read only those. The configured `model` stays apart from `reported_model`. Usage keeps absent counts absent (mutant M7, which derives a total, fails `usage_is_kept_as_reported_and_never_derived_or_invented`) and drops counts above the safe-integer bound.

Credential handling: texts pass `redact_fragments` and are checked again after redaction, because the marker can lengthen them. The tests use a canary credential in `reported_model` and `response_id` and check the whole stored file. Mutant M4 (store unredacted) fails both redaction tests and prints the canary, so the leak check is live. The remaining exposure is issue 5. Reported text reaches no log, summary or standard output: the only path is the batch file.

Idempotence: the stored bytes now vary with reported metadata, but a second run of the same request finds the file and reports it as already stored, so the batch is not rewritten. A version 1 file at the final name makes a version 2 write a no-op (tested).

## Dependency boundaries and platform checks

In the worktree, `cargo run -p rt-xtask -- check-graph check-tree scan-sources scan-api check-catalogue check-decisions check-justification` exited 0 (25 workspace crates, 152 files scanned, 152 codes, 43 external crates justified). `cargo check --target x86_64-pc-windows-gnu --all-targets -p rt-ai-adapter -p rt-ai-tool -p rt-proposals -p rt-foundation` exited 0. `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` for the five touched crates (adding `rt-ai-mock`) exited 0.

## Tests and mutants run

`cargo test -p rt-ai-adapter -p rt-proposals -p rt-foundation` exited 0, and `cargo test -p rt-ai-tool --lib` exited 0 with 556 passed, 0 failed, 0 ignored. No test was skipped. Not run: the remaining workspace tests (for example `rt-cli`, `rt-review`, `rt-ai-tool` integration tests) and any live provider test.

| Mutant | Change | Result |
| --- | --- | --- |
| M1 | `negotiate` treats `Unknown` as met | killed (4 tests) |
| M2 | lowering without the child-after-parent rule | killed (3 tests) |
| M3 | lowering allows a node named twice | killed (2 tests) |
| M4 | store reported text without redaction | killed (2 tests, canary visible) |
| M5 | version 1 reader accepts version 2 members | killed (1 test) |
| M6 | `reasoning_effort` always in the canonical bytes | killed (2 tests) |
| M7 | total tokens derived from input plus output | killed (1 test) |
| M8 | out-of-range check `>=` to `>` | **survived**, classified as a coverage gap (issue 3): the refusal still fires through the "named more than once" branch, only the message differs |

The mutant run left an untracked `proptest-regressions/tests_capability.txt` in the worktree (written by M1's failing property test). It was discarded with the worktree and is not a defect of the commits.

## Spec points not checked

The vendor semantics for depth counting and `store: false` are not verified (UD-37 section 3). The request-body and HTTP tasks (10.1, 10.2) are outside this wave, so nothing here exercises the schema against a provider.

</details>

<details>
<summary>File map</summary>

- `crates/rt-ai-tool/src/proposal_schema.rs`, `response/flat.rs`, `response/closed.rs`, `response.rs`: schema generator, lowering, flat checks, version switch.
- `tests/fixtures/proposal-schema/`: reviewed golden schema and its provenance note.
- `crates/rt-ai-tool/src/tests_flat.rs`, `tests_proposal_schema.rs`: lowering and schema tests, plus two edited version tests.
- `catalogue/diagnostics.v1.json`, `crates/rt-foundation/src/code_generated.rs`, `tests/catalogue_ud37.rs`: five new codes.
- `crates/rt-ai-adapter/src/capability.rs`, `identity.rs`, `adapter.rs`, `request.rs`: capability, identity, usage, `ProviderMeta`, effort in canonical bytes.
- `crates/rt-ai-mock/src/mock.rs`, `replay.rs`: `AdapterInfo` for both mocks.
- `crates/rt-proposals/src/model.rs`, `parse.rs`, `print.rs`: version 2 format, dual reader.
- `crates/rt-ai-tool/src/store.rs`, `run.rs`, `command.rs`, `config.rs`: `Reported`, `StoreMeta`, writer.
- `tests/fixtures/proposal-store-v1/`, `demo/stand-in-ai/golden/.../batches/`: version 1 copies and two version 2 goldens.

Full diff: `git diff ac42ecf^ 4c28650`.

</details>
