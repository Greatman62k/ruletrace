# Prompt-3 recording export and "proposer claimed by the model" label (bb42a13..a3beb8c)

This range does two things. It exports the user's prompt-3 live capture (run2) as a second recorded script with its replay configuration, replay and export tests, and notes. It also changes `ruletrace review list` so an AI origin reads `(proposer claimed by the model: '<p>', provider '<prov>', model '<m>')` (option A), with spec text, two demo goldens and tests updated. The one production change is a format string in `crates/rt-cli/src/review.rs`. Nothing in `rt-ai-*`, `rt-ledger`, `rt-review`, `rt-ir` or `rt-validate` changed, and the prompt-2 recording is byte-identical to bb42a13.

Watch for: no blocking defects. Two small notes: an overclaiming test comment (confirmed) and a "no connection" assertion that rests on the exit-code assertion beside it (confirmed, disclosed in the test).

**Verdict**: APPROVED

## High-level view

The prompt-3 recording is a faithful export of the user's run2 batch. The capture text is exact, every entry field is read from the batch, the payload hash recomputes, and the committed fixture is byte-identical to the file in `~/ruletrace-live/run2`. Replay tests compare against the live batch fixtures and literal hashes, not against replay output, and each recording is pinned to its own configuration and script.

The label change is wording only. The goldens differ from bb42a13 by exactly the inserted text `claimed by the model: `. Manual-origin rendering and control-character escaping are unchanged, and no other document or transcript prints the old form. The Req 23.12 statement holds for both pending and adopted revisions because both are built from the same stored fields.

<details>
<summary>Issues (2)</summary>

1. **Overclaiming test comment** — in `replay_live_recording.rs`, the comment above `each_configuration_names_only_its_own_recording` says the recording, live batch and configuration "agree on every identifying member". The test only checks which script and log each configuration names. Reword the comment (confirmed, non-blocking).
2. **Endpoint non-connection assertion** — the `listener.accept()` WouldBlock check in `assert_endpoint_refused` can only fail if a connection occurs, so exit code 2 plus RT-AI-0002 and the "absent for provider" message do the real work. Honest in the test's own comment. I did not mutate it (possible, non-blocking).

</details>

<details>
<summary>Details</summary>

## Recording export and fixture provenance

Prompt-2 recording sha256 is `628dc20dd933c69a…`, identical to `git show bb42a13:…prompt-2.json`. Production files changed by the range are `crates/rt-cli/src/review.rs` plus tests in `rt-cli`, `rt-tests-ai` and `rt-xtask`; the only addition under `rt-ai-tool` is `tests/fixtures/live-run2-prompt-3/` (data).

The fixture `batch.json` has sha256 `9230882b8ca93ca4…`, equal to the file at `~/ruletrace-live/run2/work/store/batches/1c574c2d….json` (read only). I compared the recording to the batch with a script of my own: `request_id`, `completion`, `payload` and `adapter_version` equal the batch, `provider` is `{kind openai-responses, id meta, model muse-spark-1.3}`, and `sha256(payload)` equals `response_sha256` (`7f3371ca…`). The capture text matches the instructed string character for character. The reported model, response id (`resp_6afac…`), usage, `created_at` and `ai_identities` stay only in the batch, as RECORDINGS.md states, and the replay tests assert the replayed batch lacks them. The `mock-replay` provider and protocol labels are not fabricated provider data. No credential-like marker appears in the fixture or the recording. Run1 and run2 batches are untouched.

## Replay and export tests

Expected values come from the live batch fixtures and literal hashes and adapter versions, not from replay output. The replayed batch is parsed and compared field by field with the live batch (request id, response and hash, proposals, model, `created_at`, adapter version). A separate test shows the two replays differ in payload hash and adapter version, which catches a replay pointed at the wrong script. Same-Request_Identifier handling is tested three ways: a merged script is refused with RT-CLI-0004 "repeated request_id", a list or duplicate `script` key in the configuration is refused with RT-AI-0002, and an export from a store mixing prompt-2 and prompt-3 batches is refused with nothing written. The export test builds its store from the fixture and checks byte equality of two exports against the committed recording, no overwrite of an existing output, and that the store is unchanged.

The "zero connections" claim: the replay rig passes no environment or credential, the configuration is checked to hold no endpoint or credential key, and the endpoint test adds a loopback endpoint and expects exit 2. The accept check can fail only if something connected, so the refusal assertions carry the proof (issue 2). The whole suite also ran in a namespace with no route.

The comment above `each_configuration_names_only_its_own_recording` claims more than the test checks (issue 1).

## Label change: wording, goldens and consistency

`origin_text` in `review.rs` is the single place that renders an AI origin, used for pending and adopted revisions. The golden diff check: applying the substitution `(proposer '` → `(proposer claimed by the model: '` to the bb42a13 versions of `s02b.stdout` and `s03f.stdout` reproduces the committed files exactly, with nothing else changed in any golden. Requirements 23.12, design, the format string and the three tests use the same wording. A grep for the old form (`(proposer '`) finds no remaining occurrence in docs, transcripts, user guide, runbook, goldens or code. The transcripts only print "script entry origin", and `review approve` does not render the origin.

New tests cover escaping of a claimed proposer with control characters, the adopted-revision listing, and manual origin (with a control-character author) lacking the new wording. Escaping, manual rendering and the refusal set are unchanged: `AiIdentitySet::from_origin` still includes `proposer`, `provider`, `model` and the batch identities, so Req 23.12's sentence about criterion 15 is true.

## Accuracy of the label

Provider and model in the batch are `config.profile.id` and `config.model` (`plan.rs`, `command.rs`), tool-assigned. The proposer is the model's `proposer` text (run1 `"model"` x5, run2 `"muse-spark-1.3"`, as RECORDINGS.md says). `adopt.rs` builds the adopted revision's `AiOrigin` from the same batch fields, so the Req 23.12 statement holds for adopted revisions too. The store files are not authenticated, so "provider and model come from the configuration" describes how the tool writes them, not a verified property of a hand-edited store. That is existing behaviour and out of scope.

## Gates and mutants

Run in the worktree with `unshare -rn` and loopback only, no credential set. `cargo fmt --all -- --check` exit 0. `cargo clippy --workspace --all-targets --all-features -D warnings` exit 0. `cargo test --workspace --all-features --no-fail-fast`: exit 0, 1819 passed, 0 failed, 0 ignored. xtask `check-graph check-tree scan-sources scan-api check-decisions check-catalogue check-justification` all ok. `cargo deny --offline check`: advisories, bans, licenses, sources ok (warning: unmatched allowance Unicode-3.0).

Mutants, each reverted and the worktree left clean:

| Mutant | Killed by |
|---|---|
| prompt-3 config points at the prompt-2 script | 4 replay tests, including `each_configuration_names_only_its_own_recording` |
| wording `claimed by model` | 3 `rt-cli` tests, 3 `rt-demo` golden tests |
| capture text `prompt-3 captured` | `the_prompt_3_store_exports_twice_…` |
| `s03f.stdout` altered (`authored-scrip`) | 3 `rt-demo` tests |
| payload text changed with hash kept | 5 replay and export tests |
| proposer escape removed | `a_claimed_proposer_is_escaped_in_the_pending_origin` |

The red/green requirement for the wording change is met by mutants, not by a recorded failing run of the new tests against bb42a13. Not run: live calls, approvals, compile, evaluate, prompt-4, M3, as instructed. Mutation coverage was not measured beyond these six.

</details>

<details>
<summary>File map</summary>

- `crates/rt-cli/src/review.rs` — format string in `origin_text`
- `crates/rt-cli/src/tests_review_adopt.rs` — updated expectation, three new tests
- `crates/rt-tests-ai/tests/replay_live_recording.rs` — parametrised over both recordings, same-id tests
- `crates/rt-xtask/tests/export_script.rs` — prompt-3 export tests
- `crates/rt-ai-tool/tests/fixtures/live-run2-prompt-3/` — batch copy and provenance note
- `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-3.json`, `ai/meta-replay-prompt-3.toml`, `provider/RECORDINGS.md` — recording, replay config, notes
- `demo/stand-in-ai/golden/steps/s02b.stdout`, `s03f.stdout` — wording only
- `.kiro/specs/ruletrace-compiler/{requirements,design,tasks,traceability}.md` — Req 23.12 and notes

Full diff: `git diff bb42a13..a3beb8c`.

</details>
