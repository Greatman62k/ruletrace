# Task 10.15: error bodies never read, host refusal proof, Meta re-read

Commits 49c4cd3 (stub tests), 07f53a5 (counting-provider tests, allowlist), d84ce26 (docs and status). The 10.7 commits (57e03ca, 27652f7, 5f6b754) are out of scope. All three commits add tests and documentation only. The task asks for an accepted, documented structural impossibility, 401/403 body-shape tests, a direct host-before-adapter proof, and a re-read of the Meta documentation. Each of those is delivered, and the tests I mutated detect the defects they claim to.

Watch for: (1) the doc claims every non-2xx status skips the body, but the new tests prove it only for 401 and 403; a mutant that drains the body of 400/402/429/5xx survives (confirmed, non-blocking). (2) The stalled-body test costs 6 s of wall time because the stub joins its 3 s pause twice (confirmed, non-blocking). (3) The re-read table omits a few documented details (listed below, none changes a profile constant).

**Verdict**: APPROVED

## High-level view

Production code is untouched. `status::failure_for` takes a status and `Retry-After` values only, and `transport::read` returns for every non-2xx status before it creates a body reader. The new tests pin that from outside: 14 body shapes for 401 and 403, a stalled declared-1-MiB body, and an oversize body. Every mutant I tried against the 401/403 path was killed, including a drain-and-ignore mutant that only the stall test catches.

The host-refusal proof counts requests for an adapter or catalogue on a `Providers` double that builds nothing. The author's criticism of the older test is correct: its `stub.connections() == 0` is vacuous for a foreign host, and the test fails under a host-check mutant only through the runtime panic in `hosted.rs`. The new tests fail directly under the same mutant.

The re-read table matches the live pages on every row I checked (14 of 14 facts spot-checked, 7 pages fetched). The author's "no profile fact differs" holds. The quotation rule is respected and the table is dated once at section level.

## Issues (5, all non-blocking)

1. **Non-401/403 body drain not detected** (confirmed) — a mutant that drains the body of every error status except 401/403 passes the whole `stub_http` suite. Add a stalled-body case for 400, 429 and 503 (and optionally 504), or narrow the doc sentence to what the tests prove.
2. **Stall test wall time** (confirmed) — `a_stalled_error_body_is_not_waited_for` takes about 6 s because `stub.finish()` waits for the 3 s pause per status. It is acceptable, but a shorter stall (for example 1.5 s with a 500 ms bound) would keep the same 150x margin ratio and halve the cost.
3. **Strict-subset row incomplete** (confirmed) — the structured-output page also forbids root `enum` and `not`, and says the subset is validated inside `$defs`/`definitions`. The row lists neither. Check that `tests_proposal_schema.rs` covers root `enum`/`not`; the schema is a plain object so the risk is nil today.
4. **Documented 504 remedy not recorded** (confirmed) — the error page names `background: true` (not subject to either limit) as the most reliable fix for 504. The 504 row says streaming is exempt but omits background mode. Informational for a later design decision.
5. **Relative URLs in the re-read table** (confirmed) — the table gives `/docs/...` paths without the `https://dev.meta.ai` host. Add the host once above the table.

<details>
<summary>Details</summary>

## Nothing parses error bodies (production code untouched)

`git diff --stat a52d67b..d84ce26 -- 'crates/*/src'` is empty for production files. The one `src` change in the range is the `mod tests_host_before_connection;` line under `#[cfg(test)]` in `rt-ai-tool/src/lib.rs`. The 10.7 commits were excluded by my path diff (`49c4cd3^..d84ce26` touches only `.kiro` specs, two test files, one new test module, `network_guard.rs` and `docs/provider-profiles.md`).

`status.rs::failure_for(status, retry_after: &[&str])` has no body parameter. In `transport.rs`, `read` computes the status, collects the `retry-after` header values, and returns `Err(status::failure_for(..))` for any code outside 200..300. The only body reader (`into_body().into_with_config().limit(u64::MAX).reader().take(read_limit)`) is below that return. `post` and `get` are the only callers of `read`, and `adapter.rs` calls only `post` and `get`. A grep over non-test sources for `read_to`, `.read(`, `into_body`, `into_reader` finds the single site. The agent sets `http_status_as_error(false)`, `max_redirects(0)` and `max_redirects_will_error(false)`, so a 3xx also reaches `read` as a plain response and returns `Other` without a body read. I found no path where a non-2xx body is read. The `models` GET goes through the same `read`.

## 401/403 tests: hand-written, real adapter, mutants

Expected values are literals (`F::AuthFailed`, one request, `RetryStep::Stop(NonTransient(AuthFailed))`). The tests drive the real `ResponsesAdapter` against the loopback `Stub` and call the real `next_step`, so the retry decision is the production function. The 14 shapes count as stated (documented 401 shape taken from the live error-handling page, the documented 402 shape on a 401 status, changed and rate-limit-like `type`, empty body with and without content type, text, invalid UTF-8, truncated JSON, HTML, JSON under XML content type, no content type, canary echoed). The canary assertion is meaningful for the echo case: it fails if a mutant copies body text into `detail`. `a_401_is_not_retried_even_when_a_success_is_waiting` queues a success behind the 401 and asserts one connection.

Own mutants run against `cargo test -p rt-ai-responses --test stub_http` in the worktree (reverted after each):

- M1 read up to 4 KiB of the error body and return `Unavailable` for a 401 whose text lacks `invalid_api_key`: killed by 5 tests (body shapes, not-retried, oversize, stalled, and the older `meta_error_shapes_map_by_status`).
- M2 401 to `Unavailable`: killed by 5 tests.
- M3 403 to `Other { 403 }`: killed by 3 tests (body shapes, stalled, older shapes test).
- M4 drain the whole body and ignore it: killed only by `a_stalled_error_body_is_not_waited_for`, so that test is the sole guard against a silent drain.
- M5 read `read_limit + 1` bytes of the error body and return `Other` when exceeded: killed by oversize and stalled tests.
- M6 401 to `RateLimited` (retryable): killed by 5 tests.
- M7 drain the body for every status except 401 and 403: **survived** (21 passed). This is issue 1.

Timing. Five full runs of the `stub_http` suite: 21 passed each time, 6.02 to 6.03 s. Instrumenting `report.elapsed` in a scratch copy (reverted) gave 5.4 to 9.2 ms for both statuses over three runs. The bound is 1.5 s, so the margin is about 160x on this machine, and a false failure needs a 1.5 s scheduling stall inside one loopback request. A body-reading mutant takes at least the 3 s stall, so the 1.5 s bound separates the two cases with room on both sides. On a slow CI runner the failure mode would be a spurious red only if the runner pauses the test thread for over a second; I rate that risk low. The suite wall time is 6 s because the stub's `finish()` joins the 3 s pause once per status (issue 2). CI results for these commits are not yet available, so the Windows and macOS timing is unmeasured.

## Host refusal precedes any adapter

The claim holds. I applied the mutant `host_permitted` bypass in `endpoint::check` (`Some(host) if false && ..`). The older `a_foreign_host_is_rt_ai_0013_before_any_connection` then failed with a panic at `hosted.rs:30` ("a test built a provider adapter for a non-loopback endpoint"). Its `stub.connections() == 0` assertion never ran and would have held, because the foreign host is not the stub. The new `a_foreign_host_asks_for_no_adapter_in_propose` and `a_foreign_host_asks_for_no_catalogue_in_models` failed on their own assertion (`RT-CLI-0006`, the counting provider's refusal, instead of `RT-AI-0013`). `the_official_origin_does_ask_for_an_adapter` passed under the mutant and unmutated, which is the expected control.

Ordering. In `command.rs`, `load()` (and `load_config()` in `models`) runs the endpoint check inside `config::load_config` at `check_endpoint`, before `check_capabilities`, consent and credential. `providers.adapter(..)` is called at `command.rs:358`, after the plan, credential refusal and store checks, and `providers.catalog(..)` at line 538 or 481, after `authorize_for_catalogue`. `rt_ai_responses` is referenced only from `credential.rs`, `endpoint.rs` and `hosted.rs` in non-test sources, and `hosted::responses_adapter` is the single place that builds a transport. So the real `BuiltInProviders` cannot build or connect before the host check in any ordering I could find. The counting double proves the property for the `Providers` seam, and the source structure proves the seam is the only route. The six spellings produce hand-written codes (RT-AI-0013 for four, RT-AI-0004 for `http://api.meta.ai`, RT-AI-0003 for the userinfo form), exit 2, an empty Transmission_Log and no canary.

One limitation: the counting provider returns `Err` from `adapter`, so the control test asserts only that the count is 1 and does not assert the exit code. That is enough for its stated purpose.

## network_guard.rs allowlist additions

The four new entries are `api.meta.ai`, `api.openai.com`, `api.meta.ai.evil.example` and `evil.example`, all for the new test file. Each appears only inside configuration text that is refused before a `Providers` double that builds nothing, and the runtime guard in `hosted.rs` still backs them. The count changes 6 to 10 and matches the four entries. The `api.meta.ai` reason mentions a refused scheme, which also covers `https://api.meta.ai@evil.example` (the scan reads its host as `api.meta.ai`). Legitimate.

## Re-read of the Meta documentation (live check)

I fetched `https://dev.meta.ai/docs/{structured-output, reasoning, protocols/responses, error-handling, models, authentication, pricing-rate-limits}` with curl on 2026-11-16 (HTTP 200 each) and compared. `web_fetch` was not available to me. Rows checked and confirmed:

- Nesting depth 10, total properties 5,000, 120,000 name characters, 1,000 enum values, 15,000 characters for an enum over 250 values, 200,000 expanded nodes: all as stated.
- Recursive schema is rejected with HTTP 400, and `strict` defaults to false: as stated.
- Effort values `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, with `none` listed and then refused with HTTP 400 for the three reasoning models: as stated, on both pages.
- `max` only for `muse-wonder-1.0`, `muse-spark-1.3`, `muse-ember-1.0`, not `muse-spark-1.3-contributor`: as stated. Default when omitted is `max` on Wonder and Ember and `high` on every Spark version: as stated.
- `store` defaults to true, `store: false` with stateless replay keeps no server copy: as stated.
- 504 `gateway_timeout` for a non-streaming request past the time limit, "do not simply retry": as stated.
- 503 `service_overloaded` and `backend_unavailable` carry `Retry-After: 60`, `server_shutting_down` a few seconds: as stated.
- Error body shape (`type` `authentication_error`, `rate_limit_error`, `billing_error`, `server_error`; `code` `invalid_api_key`, `rate_limit_exceeded`, `billing_not_configured`): as stated, and the new test fixtures reproduce the documented 401 and 402 bodies exactly.
- Do not retry 400, 401, 402, 403, 404, 413: as stated. The page says to branch on status and `type`, RuleTrace branches on status only: as stated.
- Base URL `https://api.meta.ai/v1` with Bearer authentication, key prefix `LLM_`: as stated.
- Models listing sorted newest first with `created`: as stated. Context window 1,048,576 tokens for Wonder and the Spark versions, no value for Ember: as stated.
- Rate limits 3,000 RPM and 4,000,000 TPM for the standard models and 150 RPM for the Contributor variants: as stated. The Contributor TPM (3,000,000) is not recorded, which is immaterial because the row says the limits are unused.
- `max_output_tokens` minimum 16 and shared budget with input: as stated.

"No profile fact differs" holds. The row "Context window changed from unknown" is consistent with it, because `Unknown` is not wrong, only less specific.

Omissions I found, none a defect against a profile constant. The strict-subset row omits root `enum` and `not` and the `$defs` validation (issue 3). The 504 row omits `background: true` as the documented remedy (issue 4). The page also states that requesting more than a model's configured maximum `max_output_tokens` returns HTTP 400, which the table does not mention; the table records only the minimum and shared budget. The page's 502 status is never mentioned, so RuleTrace's retry of every 5xx except 504 is the author's reading, not a documented rule; the table does not claim otherwise.

Quotations: the section says "summarised, not quoted", and I found no verbatim passage over 30 words. Dating: the section header states the read date (2026-11-16, UTC) and the table's URLs are relative paths without the host (issue 5). Tests quote the documented JSON error bodies as fixtures with a "read 2026-11-16" label.

## Gates in the worktree (detached at d84ce26, CARGO_TARGET_DIR=/home/sam/review-target-w9, cargo under `unshare -rn` with loopback up)

- `cargo fmt --all -- --check`: exit 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo test --workspace --all-features --no-fail-fast`: exit 0, 1,868 passed, 0 failed, 0 ignored.
- `cargo run -q -p rt-xtask -- check-graph check-tree scan-sources scan-api check-decisions check-catalogue check-justification`: exit 0.
- `cargo deny --offline check`: exit 0 (advisories, bans, licenses, sources ok; one "unmatched license allowance" warning).

Not verified: three-platform CI (Windows and macOS timing of the stalled-body test), and live provider behaviour. No test, golden or historical evidence was weakened: the only deletion in the three commits is the `ALLOWED` array length 6 to 10 in `network_guard.rs`.

</details>

<details>
<summary>Files</summary>

- `crates/rt-ai-responses/tests/stub_http.rs`: four new tests, 14 error-body shapes, stalled and oversize error bodies.
- `crates/rt-ai-tool/src/tests_host_before_connection.rs`: counting `Providers` double, six foreign spellings, control test.
- `crates/rt-ai-tool/src/lib.rs`: `#[cfg(test)]` module declaration.
- `crates/rt-ai-tool/tests/network_guard.rs`: four allowlist entries, array length 6 to 10.
- `docs/provider-profiles.md`: three new sections (error bodies, host refusal, dated Meta re-read).
- `.kiro/specs/ruletrace-compiler/tasks.md`, `traceability.md`: status notes for 10.15 (still `[-]`).

Diff: `git diff 49c4cd3^..d84ce26`.

</details>
