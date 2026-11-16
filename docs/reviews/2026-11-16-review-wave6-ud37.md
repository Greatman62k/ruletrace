# Wave 6 review: prompt-3, HTTP 504 final, export-script and offline replay (a2ce236..HEAD)

Range reviewed: a2ce236..f476e9e (5 commits, 57 files). Reviewed in a separate worktree at f476e9e (removed afterwards). Every cargo command ran inside `unshare -rn` with only loopback up. No credential was read or used. `~/ruletrace-live/run1` was read only (its store was copied to `/tmp` for the export check). No run2 script that needs a key was run.

The range does three things. Prompt profile 3 adds seven preamble lines and seven description sentences (on rule `type` and node `t`) and moves the adapter version to `responses-1/prompt-3`. HTTP 504 becomes a typed, non-transient `ProviderFailure::GatewayTimeout` (ends after one attempt with RT-AI-0105, exit 5). `rt-xtask export-script` plus an offline replay turns the first live batch into a committed `recorded` script and replays it through `mock-replay`.

Watch for: nothing blocks. The prompt-3 text says nothing about `field` versus `ref`, so the sibling-score failure of run 1 (RT-VAL-0002) will probably repeat in run2 (likely). The "no connection reached the listener" assertion in the replay test cannot fail (confirmed). One 504 message branch (`Other{504}` from a script) survived a mutant (confirmed).

**Verdict**: APPROVED

## High-level view

The core is untouched: `git diff --stat a2ce236..HEAD` over rt-ir, rt-validate, rt-compile, rt-engine, rt-review, rt-ledger and rt-foundation is empty. The schema golden changed by `description` members only (verified independently). Prompt-3's sentences match what the validator enforces, with one omission (sibling targets).

504 handling is typed once, in the adapter crate, and every consumer of the failure goes through `is_transient`. The pure retry function is unchanged. The real adapter and the mock both produce a final failure, the e2e tests prove one attempt and an unmoved simulated clock, and neighbouring 5xx statuses are still retried. Mutants on it were killed except one message branch.

Export is a pure function in rt-proposals with a thin file layer in rt-xtask. It refuses mixed adapter versions, providers, models and replay-origin batches, recomputes every payload hash, never overwrites, and a fresh export of the run1 store is byte-identical to the committed recording. The credential scan is the prefix heuristic of Req 28.4 and nothing stronger.

The replay stores the same payload and proposals as the live batch and labels itself `mock-replay`. It cannot be taken for a live call, but the original provider identity survives only through configured `ai_identities`, which the tool does not tie to the recording.

<details>
<summary>Issues (11)</summary>

1. **Vacuous no-network assertion** (confirmed) — `replay_live_recording.rs` binds a loopback listener whose port the program never receives, then asserts nothing connected. It cannot fail. Replace it with an assertion that can (for example the loaded profile has no endpoint) or delete it. The real evidence is that the test passed in a network namespace with no route.
2. **Surviving mutant on the 504 message** (confirmed) — changing `Some(504)` to `Some(999)` in `is_gateway_timeout` (`rt-ai-tool/src/run.rs`) passed the whole rt-ai-tool suite. The ` (HTTP status 504)` note for a scripted `{"failure":"other","http_status":504}` is untested at run level. Add one mock-path run test asserting one attempt, RT-AI-0105 and the note.
3. **Prompt-3 is silent on `field` versus `ref`** (likely) — see the prompt-4 sentence below. Not false, but the model has no guidance and run1's `field score` never clears RT-VAL-0002.
4. **Replay provenance relies on configuration** (confirmed) — the replayed batch records `provider`/`protocol` `mock-replay`, no response id and no usage, so nobody can mistake it for live. The recording's `provider.id` (`meta`) and `captured` text are not carried into the batch or the log. `meta` appears only through `ai_identities` from the user's TOML. No test asserts that those identities are present or that self-approval is refused for a replayed batch (the refusal code is unchanged and untouched).
5. **Credential scan is a prefix heuristic** (confirmed) — see section on export. It misses `s\nk-`, `\u0073k-`, base64, lowercase `bearer`, `key-sk-…` and any prefix not in Req 28.4 (the Meta key format is not in the list). The exact-value check at store time (RT-AI-0206) is the real defence. Keep the wording "credential-looking" and never "credential-free" in docs.
6. **`export-script` argument order** (confirmed) — it works only as the first argument. `rt-xtask --root . export-script …` fails with "unknown argument `export-script`", while the USAGE string lists `[--root <dir>]` first. Fix the usage text or accept `--root`.
7. **`transmission.log.jsonl` at the repo root** (confirmed) — untracked, not in `.gitignore`. It holds one `models` entry (provider, host, model) and no secret. Delete it or ignore it so `git add -A` cannot commit it.
8. **Catalogue meaning of RT-AI-0105** (confirmed) — the JSON says "Other provider error, 4xx, redirect"; the requirements table now says "…4xx other than 429 and HTTP 504, not retried". Not a blocker (see section). Align it before the first release tag, while `check-catalogue` has no baseline.
9. **Wildcard arms hide new variants** (confirmed) — `transient_code` and `final_code` in `run.rs` end in `_ =>`. `GatewayTimeout` reaches RT-AI-0105 through the wildcard, correctly, but a future variant would be classified silently. Prefer listing variants.
10. **run2 binary is not rebuilt by `run-live.sh`** (likely) — the script execs `target/debug/ruletrace-ai` as it is. The binary on disk today contains `responses-1/prompt-3` and is newer than the last production change, but rebuild (`cargo build -p rt-ai-tool`) before the live call so the batch cannot be labelled prompt-2.
11. **Stale historical sentence** (confirmed) — `traceability.md` line 530 ("No Rust code … allowed-edges.v1 … changed … Req 29.2 is unchanged") sits in the dated "spec only" UD-37 section. It is correct for that commit and wrong for the range. Add "(superseded by the sections below)".

</details>

<details>
<summary>Details</summary>

## Core and golden diffs

The six core crates named in the task and rt-foundation show no change. The only non-test production crates touched are rt-ai-adapter (enum variant and `is_transient`), rt-ai-mock (`failure_from_http`, replay `provider.id`, one capability), rt-ai-responses (status map, prompt constants), rt-ai-tool (schema descriptions, profile, message note), rt-proposals (new `export` module) and rt-xtask (new command). Nothing in rt-review, ledger or validation semantics moved.

The schema golden (`tests/fixtures/proposal-schema/proposal-schema-v2.json`): with every string `description` member removed, the old file at a2ce236 and the new file are equal as JSON (Python). Independent measurements on the new file: schema nesting depth 10 (root 1, each property, `items` and `anyOf` alternative one level deeper), 535 properties (old 535), 3,567 characters of property names and enum values, descriptions 87 → 173 occurrences and 6,064 → 14,238 characters, compact bytes 39,249 → 48,885. These equal the figures in the PROVENANCE note. All stay under the UD-37 limits (10, 5,000, 120,000). Each request carries about 9.6 KB more schema (+24.6%), of which 80 copies are the same node `t` sentence. That is a token cost, not a correctness issue.

The golden change has the user's binding requirement as its authority. The exact new bytes have no separate approval entry in traceability (the earlier approvals section is dated before this change); this review is the first independent check of them.

## Accuracy of the prompt-3 text against the validator

Read from `rt-validate/src/walk.rs`: `rule_type` fixes eligibility → boolean, derived and weighted_score → number, ranking → ranking, each mismatch an Error RT-VAL-0001; `default` compares the rule type with the declared type of the target input and an undeclared target is RT-VAL-0002; an unrecognized kind is RT-VAL-0003 (Unsupported, blocking); a node whose `t` differs from the type its form computes is RT-VAL-0001. Every sentence of the preamble and every description sentence agrees with this. Two precision notes: a ranking key may be a number or a date, so "its key is a number" is true but not the whole rule; and "the declared type of the target input for default" is right only when the target is a declared input, which the text does not say.

The prompt-3 tests (`tests_prompt_types.rs`, `tests_prompt_contract.rs`) build payloads by hand, run them through `try_accept` (the production parse, provenance and validate path) and compare exact code and message text written by hand from the validator rules. The pinned-text and sentence-to-row tests fail when a sentence is deleted or changed. Mutants: deleting the ranking preamble line is killed (`preamble_text_is_pinned`, `body_without_effort_is_exactly_these_bytes`); changing "Must be ranking, not number" to "Must be number" is killed by four tests including the golden comparison; moving `PROMPT_PROFILE`/adapter version back to prompt-2 is killed by two.

`tests_live_run1.rs` re-runs today's validator on the stored prompt-2 payload and expects the recorded findings, hand-written. This shows the prompt-3 tests catch the original model errors.

## `field` versus `ref`

`ExprKind::Field` resolves only against declared inputs (`decls.inputs`). `ExprKind::Ref` resolves against rule targets of the rulebook being validated (`targets`, built from current items, ranking targets excluded). `sibling_reference.rs` drives the real review path: both `field score` and `ref score` give RT-VAL-0002 and RT-APR-0205 while the score rule is only proposed. After the score rule is approved, the `ref` form is approved and the `field` form is still refused.

Prompt-3 says nothing false about this, and nothing about it at all. It tells the model to make "a value computed as a number, such as a score" its own derived rule, which pushes the model toward a ranking that reads that score, and then gives no way to read it. Minimal accurate sentence for a possible prompt-4, to be backed by `sibling_reference.rs`:

> A `field` node reads an input that the rulebook declares and never the result of a rule; to read the result of another rule use a `ref` node whose `target` is that rule's target, and know that a `ref` to a rule that is only proposed is stored with a blocking RT-VAL-0002 finding until that rule is approved.

Changing it moves the adapter version to prompt-4 and changes the schema or preamble hash pins; the Request_Identifier stays the same.

## HTTP 504 as a final failure

Requirement 29.2 now reads "an HTTP 5xx response other than 504 … and SHALL NOT retry an HTTP 504 response"; 35.14 maps 504 to a gateway timeout ending after the attempt with RT-AI-0105 and a message that names the status and none of the provider's text; the RT-AI-0105 table row, R-21, R-D16, UD-37 §4 (decision, options offered, implementation record), provider-profiles.md, design §3.7.3 and tasks are consistent with each other. A search for "504" with "transient/retried/open" finds no remaining stale statement except issue 11.

Code: `ProviderFailure::GatewayTimeout` is non-transient. `is_transient` is `true` for `Unavailable`, `Timeout`, `RateLimited` and `Other{500..=503 | 505..=599}`; `Other{504}` and `GatewayTimeout` are false, as are Auth, TLS and other 4xx. Matches on the enum: `is_transient` (exhaustive), `models.rs::failure_kind` (exhaustive, new arm), `run.rs::is_gateway_timeout`, `transient_code` and `final_code` (wildcards, issue 9). The workspace compiles, so no exhaustive match is missing. `rt-ai-responses/status.rs` maps 504 before the `500..=599` arm. The mock maps 504 to `GatewayTimeout` in `failure_from_http`, and a scripted `other` with status 504 gives `Other{504}`; both are final.

Tests: the retry hand table has rows for `Other{504}`, `GatewayTimeout`, 500 and 502 (transient) and "504 after a transient failure stops without a wait". The property oracle `transient()` is a one-line restatement of the rule, but the hand table is independent of it, and the strategies list 504 and `GatewayTimeout` explicitly so they occur often. E2E (`tests_e2e_responses`): with two retries and with `max_retries = 0`, one connection, no wait, simulated clock still zero, RT-AI-0105, exit 5, no warning, one log entry, "HTTP status 504" in stderr and the provider sentinel text absent, nothing stored; 500, 502 and 503 still make three attempts on 2 s and 8 s. The `models` command ends after one attempt with RT-AI-0109. The earlier tests that asserted 504 retried were rewritten to the decision (the models exhaustion test now uses 503): disclosed expectation changes, authorised by the user's decision, and the retry coverage they gave is kept by the 5xx test.

Mutants: removing the `504 =>` arm: killed (2 tests). `Other{504}` made transient in `is_transient`: killed (4 tests). `GatewayTimeout` moved into the transient group: killed (3 e2e tests). `is_gateway_timeout` `Other{504}` arm changed to `Other{999}`: **survived** (issue 2).

RT-AI-0105 in the catalogue: `check-catalogue` validates the JSON itself (codes, severities, executable names, one-line meaning, generated enum, release-tag baseline). It never reads the requirements table, so the mismatch does not fail it; the JSON text was already narrower than the requirement row before this range. It must not be fixed in this wave, but should be before the first release tag because the generated rustdoc quotes the meaning.

## Export, credential scan and the committed recording

`export_script` sorts by Request_Identifier, recomputes SHA-256 of every payload (refusing a difference with file and request id), refuses `mock-replay` batches and a `mock-replay` header, and refuses any batch whose model, adapter version, protocol or provider id differs from the header or the first batch. Req 33.8 justifies the replay refusal: the script holds "the unmodified Provider_Response bytes that a real AI_Provider returned" and an authored entry must never be described as provider output. A replay-origin batch is exactly that case.

Run against a read-only copy of the run1 store: a fresh export with the committed arguments is byte-identical (`cmp`) to `demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json`. The recording's payload equals the batch payload, its hash recomputes (9585ce1d…), the capture text equals the required string exactly (checked by string comparison), and the batch fixture is byte-identical to the run1 store file (sha256 `cc8e155e…`). Script format `ruletrace-recorded-script` version 1 is unchanged, `provider.id` is an optional member added to the parser (`allowed` includes `id`, `required` does not), and an old script without it still parses. Nothing is fabricated: reported model, response id and usage are not exported because the format has no member for them. A second export to the same path fails (`create_new`); a wrong `--provider-id` and a `mock-replay` provider are refused with no file written; a header with `sk-` is refused. The output is created only after the export succeeded.

Credential scan variants, tried through `export_script` with hand-built batches: `sk-…` refused, `sk-\n…` refused, `Bearer …` refused; accepted: `s\nk-…`, `\u0073k-…` (JSON escape), base64 of `sk-…`, lowercase `bearer`, `key-sk-…` (prefix not at a word start), and `LLM|…` style prefixes. The set is that of Req 28.4 and the scan checks the payload and the header text only (the other entry members are ids and hashes). That is what the requirement asks for; it is not a barrier against a determined or unusual key (issue 5). Mutants: dropping `sk-` or `ghp_` from the list: killed; skipping the hash comparison: killed; removing the replay refusal: killed; `create_new` → `create+truncate`: killed.

The edge `rt-xtask → rt-hash` is a `[dev-dependencies]` entry with a stated reason (tests compute payload hashes). `allowed-edges.v1` lists it on the rt-xtask line and `check-graph` and `check-tree` pass. rt-xtask now has a normal edge to rt-proposals, which the allowed-edges file already listed.

## Offline replay and labelling

`ai/meta-replay.toml` has no endpoint and no credential field (the test asserts both), `provider.id = "mock-replay"`, the model and effort of the live run, and `ai_identities = ["mock-replay","meta","muse-spark-1.3"]`. I ran `ruletrace-ai propose` on a scratch copy with an empty environment and no network route: exit 0, "run summary: complete", RT-AI-0305 only, the same Request_Identifier `1c574c2d…`. The stored batch equals the live batch in `response`, `response_sha256`, `proposals`, `model`, `created_at` and `adapter_version` (still `responses-1/prompt-2`). It differs only in `provider` and `protocol` (`mock-replay`), `ai_identities` (adds `mock-replay`) and the absence of `reported_model`, `response_id` and `usage`. The transmission log line names provider `mock-replay` and an empty `endpoint_host`.

So a replay cannot pass for a live call: the batch says mock-replay and the metadata a live call would carry is missing. The cost is issue 4: the original identity `meta` is in `ai_identities` only because the user wrote it, and the replayed batch does not record the recording's `captured` text. The recording file itself keeps `provider.id` and the capture text. I judge the labelling honest; it is not self-verifying.

The replay test (`replay_live_recording.rs`) compares against the live batch fixture rather than anything derived from the replay output, checks one log entry and no document text in it, and that a second run stores nothing new (RT-AI-0301). Its listener assertion is vacuous (issue 1). The Windows expectation `stored_with_blocking_findings_codes()` is a per-platform exact list (RT-AI-0305, plus RT-AI-0309 on Windows where the directory sync is skipped), matching Req 29.19; it is not a loosened assertion.

## Negotiation effect of `reasoning_effort = yes` on mock-replay

The change is in the replay provider and the `mock-replay` profile. The effort is part of the Request_Identifier, so a configuration whose effort differs from the recorded one produces another identifier and the script lookup misses (RT-AI-0105). That is still a fail-closed result, with a different reason from the old capability refusal. A configuration that lowers the capability (`[capabilities] reasoning_effort = "no"`) is still RT-AI-0011, now asserted by a new test. The authored step provider keeps `No`, and the negotiation unit test still asserts the exact `Unmet{reasoning_effort, No}` for it. The three changed expectations (`tests_profile`, `tests_config_v2`, `tests_info`) follow the user's decision and keep a negative test each. Nothing else in negotiation moved.

## run2 scripts

`setup.sh` builds, refuses an existing store, ingests and `cmp`s candidates and source model against run1. `dry-run.sh` uses `--dry-run` and prints the Request_Identifier. `run-live.sh` differs from run1's only by comments and a guard that refuses an existing `work/store/batches`; it uses the real flags, execs the tool and never prints or reads the key (the key is read inside the tool from `MODEL_API_KEY`). `meta.toml` is byte-equal to run1's, mode 600, and names the variable only. `compare.py` reads only stored batch files, prints stored values and prints `absent` for missing ones; it also prints "same kind, type and findings" or "differs" per clause, a comparison of stored facts although its docstring says it derives nothing. `review list` in `inspect.sh` uses a nonexistent ledger path and writes nothing. Issue 10 covers the stale-binary risk.

## Quality gates and evidence

Commands (all inside `unshare -rn`, offline, worktree at f476e9e):

- `cargo test --workspace --all-features --no-fail-fast`: exit 0, 1,801 passed, 0 failed, 0 ignored.
- Core-only (CI's `AUX_EXCLUDE` list, `--skip ai_profile_`): exit 0, 881 passed, 0 failed, 0 ignored; the core-only target holds `ruletrace` and no `ruletrace-ai` or `rt_ai_tool` artifact.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo fmt --all --check`: exit 0.
- `rt-xtask check-graph check-tree scan-sources scan-api check-decisions check-catalogue check-justification`: exit 0 (check-catalogue baseline skipped: no release tag).
- `cargo deny --offline check`: exit 0 (advisories, bans, licenses, sources ok; only an unmatched-license-allowance warning).
- `cargo check --target x86_64-pc-windows-gnu --all-targets` for rt-proposals, rt-xtask, rt-ai-mock and rt-ai-adapter: exit 0.

Not verified: rt-ai-responses, rt-ai-tool and rt-tests-ai on Windows (the cross C compiler `x86_64-w64-mingw32-gcc` is not installed, so their dependency build fails before our code); macOS; the golden job on three platforms; any live call; `~/ruletrace-live/run2` (prepared, not executed).

Mutants of my own (each applied to the worktree, tested, restored): 504 status arm removed (killed), `Other{504}` transient (killed), `GatewayTimeout` transient (killed), export hash check disabled (killed), `sk-` prefix dropped (killed), `ghp_` prefix dropped (killed), replay-origin refusal removed (killed), output `create_new` removed (killed), ranking type description inverted (killed), ranking preamble line deleted (killed), adapter version back to prompt-2 (killed), `is_gateway_timeout` `Other{504}` arm changed (survived, issue 2). Eleven of twelve killed. This is a hand-picked set, not a measured mutation score.

Assertions or goldens changed in the range, all disclosed in the commits and requirements: the schema golden (description members only); the 504 retry tests and the `models` exhaustion test (now 503); the three reasoning-effort capability expectations for mock-replay; the adapter-version expectations (prompt-2 → prompt-3); `tests_retry` oracle and strategies (504 excluded from the transient set). No test was skipped, ignored or loosened to pass.

## Files

The range touches: spec documents (requirements, design, tasks, traceability, UD-37, provider-profiles); rt-ai-adapter, rt-ai-mock, rt-ai-responses, rt-ai-tool (504, prompt-3, schema descriptions, replay capability); rt-proposals `export.rs`; rt-xtask `export.rs`, `main.rs`, `Cargo.toml`; `tools/allowed-edges.v1`; the live-run1 fixtures; the committed recording, replay config and RECORDINGS.md; the schema golden and its PROVENANCE note; rt-tests-ai tests (`replay_live_recording`, `request_id_pin`, `sibling_reference`). Full diff: `git diff a2ce236..HEAD`.

</details>
