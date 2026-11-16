# Optional `live-openai` CI job and the recorded closure of Task 10.17

Commits 438b700..9a2821a add a `live-openai` job to `.github/workflows/ci.yml`. The job does nothing unless the secret `RULETRACE_OPENAI_API_KEY` and the variable `RULETRACE_OPENAI_MODEL` are both non-empty, and `ci-gate` does not wait on it. The commits also add 21 structure and script tests in `crates/rt-xtask/tests/live_openai_job.rs`, the UD-15 section 6 record, and UD-37 section 8 with the matching `tasks.md` and `traceability.md` notes that close 10.17 by user decision. I reviewed this as a GitHub Actions security reviewer and ran no network or credential step. All local gates pass. No defect blocks the requirements you listed, and the mutation run found test gaps and one hardening item.

Watch for: (1) the secret is in the job-level `env`, so every step of the configured job sees it, including `cargo build` and its build scripts (confirmed). (2) The structure test does not detect three edits that touch your binding requirements: `pull_request_target` added, a different `endpoint`, and `live-openai` in a block-style `needs` list of `ci-gate` (confirmed by mutation). (3) The model-charset check depends on the locale and accepts non-ASCII letters under `en_US.UTF-8` (confirmed). (4) The bash tests are `cfg(unix)` and the docs do not say that only 12 of the 21 tests run on Windows (confirmed).

**Verdict**: APPROVED

## High-level view

The gate step is a plain emptiness test on two variables that arrive through `env`. It writes `configured=true|false` to `$GITHUB_OUTPUT`, and every later step carries `if: steps.gate.outputs.configured == 'true'`. With no secret or no variable, every later step is skipped and the job concludes `success`. Fork pull requests, which receive empty secrets and variables, take the same path.

The configuration written at run time passes the model text through a bash-regex allow-list, then writes two mode-600 TOML files under `$RUNNER_TEMP`. They name the credential by variable only and fix the endpoint at `https://api.openai.com`. The real `ruletrace-ai` parses these files. `models` does not negotiate capabilities, so it proceeds with the `openai` provider id.

The tests mix a line-based structure checker with eight mutants, five gate runs with a canary, and the configuration step with ten hostile model texts. They kill the mutants they were written for. They do not pin the endpoint, the trigger set or the block-list form of `needs`.

The documentation is accurate. Both CI run ids and head SHAs match, the wave table matches the review files, and 10.5 and 10.15 stay `[-]`. The user review and the independent reviews are kept apart.

## Issues (8)

1. **Secret visible to every step (non-blocking, confirmed)**: `RULETRACE_OPENAI_API_KEY` is set at job level, so `actions/checkout`, rustup and `cargo build` (build scripts, proc-macros) all see it. Move it to step-level `env` on the gate, `models` and `propose` steps, and move the test's "job-level env" rule to match.
2. **`pull_request_target` not guarded (non-blocking, confirmed gap)**: adding the trigger to the workflow leaves all 21 tests green. Add a rule that the file never contains `pull_request_target`.
3. **Endpoint not pinned by any test (non-blocking, confirmed gap)**: changing it to `http://api.openai.com.evil.example` leaves all 21 tests green. Assert `endpoint = "https://api.openai.com"` in the generated files.
4. **Block-style `needs` evades the check (non-blocking, confirmed gap)**: `needs:` followed by a `- live-openai` list line passes `the_committed_workflow_satisfies_every_live_job_rule`. Parse the `ci-gate` job's `needs` as a list or scan every line in that job.
5. **Locale-dependent charset (non-blocking, confirmed)**: under `LC_ALL=en_US.UTF-8` the check accepts `é`, `a٣` and `Ａbc`. The test environment is cleared, so it runs under the C locale and cannot see this. The text stays inside a TOML basic string with no quote, backslash or control characters, so there is no injection. Put `LC_ALL=C` at the top of that step and add a test run under a UTF-8 locale where one exists.
6. **Windows coverage undisclosed (non-blocking, confirmed)**: `mod gate_script` is `cfg(unix)`, so on Windows 12 of the 21 tests run and the other 9 are absent, not reported as ignored. State this in the `tasks.md` 10.5 note and in the UD-15 record.
7. **Generated configuration never parsed by a test (non-blocking, confirmed gap)**: no test feeds the written TOML to the real config parser. I did it by hand and it parses. A schema change could make a configured job fail without any test noticing. Add a test that parses it.
8. **Failure of `models` fails the job (informational)**: the `models` step runs under `bash -e`, so a provider outage or a bad key makes the job red. The job is not in `ci-gate`, so merges are unaffected. If a red mark on PRs is unwanted, decide this explicitly.

<details>
<summary>Details</summary>

## Gate step and the skip path

The gate reads `RULETRACE_OPENAI_API_KEY` and `RULETRACE_OPENAI_MODEL` only through `env`. No `${{ }}` expression appears in any shell text of the job. The only `${{` expansions in the job are the two job-level `env` lines, plus `inputs.propose` in an `if:`. The gate expands the secret only inside `[ -z "${...:-}" ]` and prints neither value. The five-environment test with a canary confirms exact stdout, empty stderr and `configured=false|true` in the output file.

A missing secret and an empty secret are treated identically. Empty is also what a fork pull request receives, so forks skip. A skipped step does not fail a job, and a job with every later step skipped concludes `success`. `ci-gate` uses `toJSON(needs)` and requires every need to be `success`. `live-openai` is not in its single-line `needs`, so a skip cannot turn the gate red. The `workflow_dispatch` input is `type: boolean`. In an `if:` expression `inputs.propose == true` compares booleans correctly, and `github.event_name == 'workflow_dispatch'` guards the other events, where `inputs.propose` is empty. Top-level `permissions` is `contents: read` and the job does not override it. There is no `pull_request_target` and no artifact step in the job. `actions/checkout@v5` matches the other jobs. The workflow contains no other third-party action beyond `upload-artifact@v6`, which this job does not use.

The residual exposure is that the secret reaches every step. For a same-repository pull request or a push to `main`, a build script of any dependency, or any edit to the workflow by someone with write access, can read it. Dependencies are locked by `--locked` and `cargo deny`. The risk is the standard one for repository secrets, but a step-level `env` on three steps removes it for the build at no cost.

## Runtime configuration and the `models` command

I ran the step's script with these model texts. Accepted: `gpt-5:x`. Refused (existing tests): quote, space, newline, trailing newline, empty, leading `-`, backslash, `$(id)`, and a 101-character value. Under C and C.UTF-8 all non-ASCII texts I tried were refused. Under `en_US.UTF-8`, `é`, `a٣` and `Ａbc` were accepted, because bracket ranges follow the locale's collation. The GitHub runner default is most likely `C.UTF-8`, so this is a hardening point and not an exploitable one. Both files had mode 600, the canary appeared in neither, and the log path is under `$RUNNER_TEMP`.

The written file passes the real parser. `ruletrace-ai models --config` on it, run in an isolated network namespace with a dummy key, got past validation, reported the endpoint host `api.openai.com` and failed only at the wire (`RT-AI-0109`, exit 5). `tests_e2e_models.rs::models_does_not_negotiate_capabilities` covers the `openai` id with the stub. The propose step's flags work as written: ingest returned 0, and `propose` built one request for 5 clauses to host `api.openai.com` and failed only at the wire (exit 5). Exit 5 is among the informational codes, as documented. A usage error, exit 2, is also treated as informational, which could hide a broken invocation. Nothing in the job checks that.

## Test quality and mutants

The suite passes 21 of 21 on Linux and the 8 documented mutants and 2 script mutants are detected by design. I ran my own mutants on `ci.yml`:

- A build step without the gate condition: killed (the committed-workflow rule fails).
- A `curl` of `$RULETRACE_OPENAI_API_KEY` added to a run step: killed.
- The regex `$` anchor removed: killed by the hostile-text test (`ok\nbad`).
- `umask 077` and `chmod 600` removed: killed by the mode assertion.
- Endpoint changed to an http look-alike host: survived.
- `pull_request_target:` added: survived.
- `live-openai` added to `ci-gate` as a block-style `needs` list: survived the committed-workflow rule. Only the one-line mutant test fails, and it fails because the file no longer matches its pattern.

The structure checker is line-oriented. It relies on the formatting of the current file, which is acceptable for a pinned workflow, but each of the three survivors is something your requirements name. Issues 2 to 4 above list the fixes.

## Documentation accuracy

`gh run view` returned `success` for both runs: 43799230103 with head `a2ce2369f550…` (attempt 1, 22 jobs) and 43847096409 with head `438b7009adf1…` (attempt 2, 22 jobs, all success). That matches the note about one rerun. The UD-37 section 8.5 verdicts match the `**Verdict**` line of each review file (waves 1, 3, 4, 5 NEEDS_CHANGES; 2, 6, 7 APPROVED). The test names cited for the pinned body and the Meta error shapes exist in `stub_http.rs`, and `docs/provider-profiles.md` exists. `tasks.md` has 10.17 `[x]`, with 10.5 and 10.15 still `[-]`, and the closing note says the blockers are not claimed complete. It is explicit that the user review is distinct from the independent reviews. The CI evidence predates the `live-openai` job, so no CI run has executed it yet. The 10.5 note says so. I could not verify the UD-37 8.2 statement that no agent read the user's credential.

One wording item: the tasks note says the tests were "verified locally". It should add that on Windows only the 12 structure tests exist (issue 6).

## Repository state

`transmission.log.jsonl` at the root is untracked, excluded only in `.git/info/exclude`, and its mtime (08:09) and 148 bytes are unchanged. `git ls-files` lists only `demo/stand-in-ai/golden/artifacts/transmission.log.jsonl`, an existing golden fixture. No M3 content, prompt change or live call appears in the three commits. The three commits touch only the six listed files.

## Gates (separate worktree, `unshare -rn` with loopback only, `CARGO_TARGET_DIR` under `/home/sam`)

- `cargo fmt --all -- --check`: exit 0.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo test --offline --workspace --all-features --no-fail-fast`: exit 0, 1840 passed, 0 failed, 0 ignored.
- `rt-xtask` check-graph, check-tree, scan-sources, scan-api, check-catalogue, check-decisions, check-justification: all exit 0.
- `cargo deny --offline check`: exit 0 (advisories, bans, licenses and sources ok; one unmatched-license-allowance warning).
- Not verified: a real Actions run of the `live-openai` job (skip path or live path), the Windows and macOS test results for this commit, and any live call.

</details>

<details>
<summary>File map</summary>

- `.github/workflows/ci.yml`: new `workflow_dispatch` trigger with a boolean `propose` input, and the `live-openai` job.
- `crates/rt-xtask/tests/live_openai_job.rs`: 21 tests (structure rules, mutants, gate script, configuration step).
- `tools/decisions/UD-15.md`: section 6, the secret and variable names and the job rules.
- `tools/decisions/UD-37.md`: section 8, the 10.17 closure, evidence, open blockers and the independent review table.
- `.kiro/specs/ruletrace-compiler/tasks.md`: 10.5 status note, 10.17 marked `[x]` with its note.
- `.kiro/specs/ruletrace-compiler/traceability.md`: two new sections for 10.17 and 10.5.

Full diff: `git diff 438b700..9a2821a`.

</details>
