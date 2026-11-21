# Review L1-d0c821c: Laboratory threat model and design section 11

Reviewer: independent reviewer (`ruletrace-reviewer`). Review of documents only. No code, test or mutant was run (see sections 6 to 8).

## 1. Verdict

**CHANGES REQUIRED.** Three blocking findings (B1 to B3), eleven non-blocking findings (N1 to N11). All are documentation edits. None needs a code change. The evidence claims C1 to C8 largely hold (section 5). No open item was closed (Q4), and 34.10 and 37.21 stay open as instructed.

Blocking findings in one line each:

- B1. The threat model gives mechanism M5 a P1-os "Yes" that its own definition of P1-os and its own section 6.1 do not support.
- B2. Design §11.5 serializes every state-changing request, which includes `POST /session/end`, and that contradicts Requirement 37.17 and the shutdown design of §11.11.
- B3. The design's direct dependency list for `rt-lab` contradicts Requirement 24.13 ("limited to") and is not recorded as an RN.

## 2. Scope

- Commit range `6bde3b8..d0c821c` (HEAD of branch `m3`, read from `.git/HEAD`). One commit, five files, documentation only. `git diff --stat` shows no path under `crates/`, `tools/allowed-edges.v1` or `catalogue/`.
- Examined in full: `docs/security/laboratory-threat-model.md` (342 lines); `.kiro/specs/ruletrace-compiler/design.md` §11.1 to §11.17 (lines 3205 to 3505), RN-31 to RN-43 (lines 3144 to 3156), and every "(Amended by UD-38" / "(Superseded by UD-38" note and traceability row 27, 31, 34, 36, 37 (by diff); `requirements.md` Requirement 36 (all), Requirement 37 criteria 1 to 26, Requirement 24.13, 27.1, 29.1, 34.9, 34.10; `traceability.md` lines 985 to 1008; `tools/decisions/UD-38.md` (all, including section 8); steering `product-principles.md` and `technical-architecture.md` (as loaded with the agent).
- Code read to check claims C1 to C8: `crates/rt-ai-responses/src/credential.rs`, `crates/rt-ai-responses/src/transport.rs`, `crates/rt-ai-responses/tests/proxy_env.rs`, `crates/rt-ai-tool/src/credential.rs`, `config.rs` (lines 370 to 430), `command.rs` (lines 300 to 330), `cli.rs` (lines 95 to 155), `crates/rt-cli/src/args.rs` (lines 325 to 395), `crates/rt-cli/src/review.rs` (lines 235 to 325), `crates/rt-review/src/plan.rs` (lines 255 to 315), `crates/rt-cli/src/evaluate.rs` (lines 440 to 575), `crates/rt-cli/src/tests_pipeline.rs`, `tests_published_kinds.rs`, `crates/rt-foundation/src/limits.rs`, `catalogue/diagnostics.v1.json`, `Cargo.lock`, `tools/dependencies.v1`, `.github/workflows/ci.yml`, `demo/stand-in-ai/**`.
- Scratch worktree `~/review-wt-L1` at `d0c821c`, removed at the end. No target directory was created (no cargo command was run).

## 3. Blocking findings

### B1. M5 is credited with P1-os that the document says only M6 can have

- Where: `docs/security/laboratory-threat-model.md` line 229 (M5 row, P1-os column reads "Yes for the Laboratory process"); contradicts lines 205 to 216 (definition of P1-os: "the operating system prevents the Laboratory process from reading the credential even if its code were malicious. Under X5 ... this holds for no mechanism below except M6") and design §11.10 line 3415 ("P1-os holds only with a separate OS user (M6)").
- Reason: under M5 the user exports the variable, or prefixes the command, in a terminal of the same OS user. A malicious Laboratory process runs as that user (X5 is out of scope, but P1-os is defined as holding under it). On Linux the environment of a same-user process is readable by that user, and Yama `ptrace_scope` 1 restricts attach, not read. Shell history and terminal scrollback of the same user are readable too. So M5 shrinks the exposure (no credential in any process the Laboratory starts) and gives P1-code by construction. It does not give P1-os. The document states both claims and they disagree. This is the specific over-claim the user asked about (Q1).
- I could not test the Linux environment-read claim: the repository hook blocked a command that reads `/proc/*/environ`. The finding rests on the document's own definition and on its own statement for M2.
- Fix: change the M5 P1-os cell to "No (same-user readable; exposure window only)" and add one sentence to §6.3 and §11.10 that M5 reduces exposure and does not isolate. The recommendation can stay.

### B2. Serialization of "every state-changing request" blocks ending the session

- Where: design §11.5 line 3335 ("One mutex per session guards every state-changing request. A request that finds it held gets 409"), route table line 3325 (`POST /session/end` is session-required and CSRF-required, so it is state-changing under Requirement 37.9), and requirement 37.25 ("at most one state-changing request ... refuse a state-changing request that arrives while another runs"). §11.11 line 3428 and Requirement 37.17 say that ending the session kills and reaps running children.
- Reason: a step may run for up to 300 s. During that time the user's `POST /session/end` finds the mutex held and gets 409, so the only way to stop a runaway step is a signal to the process. UD-38 section 6 items 5 and 12 ask for cancellation and shutdown. As written, 37.17 and 37.25 cannot both hold. A test for each, as the requirements demand, would contradict the other.
- Fix: state in §11.5 that `POST /session/end` is exempt from the mutex and pre-empts a running step (and add a requirement note or an RN for 37.25). Add the case "end the session while a step runs" to the §11.5 test list. Also say which status the Launch_Secret exchange gets while a step runs (see N6).

### B3. Direct dependency list of `rt-lab` contradicts Requirement 24.13 and is not an RN

- Where: design §11.3 line 3271 (`deps rt-foundation rt-hash rt-json rt-numeric rt-data rt-compiled rt-trace rt-explain rt-srcmodel prize-check bracket-check`); requirement 24.13 (`requirements.md` line 978): the edges of `ruletrace-lab` "are limited to the two Application_Layer crates, the shared Diagnostic, Location, and Exact_Number crates, and one HTTP server library with a written justification".
- Reason: seven of the listed direct edges (hash, json, data, compiled, trace, explain, srcmodel) are outside the requirement's list, and the design chooses no HTTP server library (option A, standard library). The design may be right and the requirement too narrow, but §9.4 records no RN for it. The user asked whether the design contradicts a requirement without recording it. §11.3 cites 24.13 only for the evaluation-call ban.
- Fix: add an RN (next free number, RN-44) that quotes 24.13, lists the extra edges with a reason each, and states that the "one HTTP server library" is not used under LT-4 option A. Leave the decision to the user.

## 4. Non-blocking findings

### N1. `ruletrace-ai models` reads the credential without capability and consent checks (C1 nuance)

Threat model line 37 says the credential is read "after every configuration, capability and consent check". For `propose` that is true (`config.rs:397-404`: `load_config`, `check_capabilities`, `consent.check`, then `read_credential`). For `models`, `authorize_for_catalogue` (`config.rs:415-424`) skips capability and consent by design (Req 35.18). The Laboratory does not call `models`, so the risk is nil, but the sentence is imprecise. `--dry-run` returns before `load` (`command.rs:305-306`) and reads no variable (verified).

### N2. LT-3 option (c) contradicts the M5 design text

Threat model §6.5 (c), line 289: under M5 "the Laboratory never receives a live configuration". Design §11.10 line 3420 has the Laboratory show an argument vector with "configuration path", and §11.6 line 3348 has it run `ruletrace-ai ... --dry-run` for planning. If a live configuration path reaches the Laboratory, the 37.21 start check applies and (c) does not hold. If it never does, the 37.21 refusal is exercised only on mock configurations (which name no variable), so 37.21 has no live test. State which.

### N3. 37.20 canary test under M5 is weaker than the requirement text

Design §11.16 line 3494: under M5 the harness runs `ruletrace-ai` itself with the canary. Requirement 37.20 says the test "runs a complete proposal workflow" through the Laboratory. That is a departure from 37.20, not recorded as an RN. It is also not stated that, under M5, the Laboratory's own environment never holds the canary by construction and the test therefore shows less than 37.20 intends.

### N4. M5 does not meet the user's statement that the Laboratory orchestrates `ruletrace-ai` for AI-assisted interpretation

The threat model says so itself (line 238, "M5 does not satisfy ... for live runs"). I record it because §11.6, D-19 and §11.10 call M5 "recommended now". It is correctly marked OPEN (LT-1, LT-2). Nothing is decided. Keep the sentence in the user-facing decision request.

### N5. UD-38 section 6 item 1 names a candidate the threat model does not analyse

"A launcher that is not the Laboratory" (UD-38.md line 100) has no row in §6.2. It is close to M3 but different: the launcher starts both processes and supplies the credential only to `ruletrace-ai`. Add a row or say why M3 covers it. Also M4's rejection cites "UD-18" (`threat-model:228`). There is no `UD-18.md`. The supporting text is `consistency-analysis.md` line 23 ("Env var name only (28.2, 28.4)"). The premise is true; the citation is not findable.

### N6. Launch_Secret lifetime, scrollback and exchange rate are not addressed

- An unconsumed Launch_Secret has no expiry and the exchange endpoint has no attempt limit (§11.5, 37.5). With 128 bits the guessing risk is nil. The exposure window is unbounded, and LT-5's recommendation (print a new secret after the session ends) adds a fresh unexpired secret to terminal scrollback each time. LT-5 also contradicts the wording of 37.5 ("at each start", "print it once"); LT-5 is open, so this needs a requirement note if the user picks it.
- Terminal scrollback, terminal logging (tmux, IDE, supervisor capturing stderr) and screen sharing are not in the model (the only "history" hit is URL history, T5). A secret in argv would reach shell history. Printing to stderr avoids that, which is worth stating.
- 37.25 and 37.9 make `POST /session` state-changing, but no session mutex exists before a session does. Say whether a second exchange during a running step gets 401 (37.6) or 409 (37.25).

### N7. Missing threats for a loopback web application (Q2)

Covered and sound: DNS rebinding (T2), cross-port cookie (T7), cookie tossing (T8), SameSite and same-site IP hosts (T3), clickjacking (T9), CSRF (T3), referrer (T5, 37.10), symlink swap (T23, residual stated), uploads (T26, T27). Not covered:

- Request smuggling. §11.4 refuses `Transfer-Encoding` and uses one request per connection, which removes most of it. Duplicate or non-numeric `Content-Length`, obsolete line folding, bare LF and lone CR in header values are not listed.
- Slowloris and connection exhaustion. T12 says "read timeout" and "one request per connection". No value, no header-read deadline, no cap on concurrent connections and no thread model (thread per connection is the likely design, so a cap matters) appears in §11.4 or §11.9. The limits table has no row for it.
- Browser cache. 37.10 lists four headers and no `Cache-Control: no-store`. Approval pages carry the Rule_Content_Hash and the CSRF token; the Back button and the HTTP cache can reshow them. A stale cached form is refused by the state-version guard, but `no-store` should be required.
- Prefetch and prerender (GET only, so side-effect free by 37.9; say so).
- Form resubmission. The design uses 303 after POST (good), but the status page refreshes by `<meta http-equiv="refresh">` with GET only. State that the refresh target is a GET with no side effect.
- Browser extensions and same-user processes reading the page. Same-user is X5; extensions are not named.
- Windows. T23 relies on "mode 0700", which does not exist on Windows. No ACL statement for the session directory, no rule for reserved device names, case-insensitive and 8.3 names, `\\?\` prefixes or alternate data streams in the path check of 37.13, and the SIGKILL-equivalent limits of T17 differ. The design lists V-2 and V-3 for signals and environment only.
- Multipart parser abuse (boundary tricks, duplicate parts, huge part headers). §11.16 plans property tests for it, but the threat table has no row.
- The first-visit login CSRF on `POST /session` is safe only because the secret is unguessable. One sentence would do.

### N8. Exit-code table (§11.8) misses two behaviours of `evaluate`

- `classify` (`evaluate.rs:564-570`) checks `invalid_question`, then `indeterminate`, then `provenance`. A run with an Indeterminate Outcome and a provenance Error exits 4, not 2. The row for exit 4 says nothing about provenance. The Laboratory must read the verification section of the results file for every executed evaluation and must not infer verification from the exit code.
- On exit 2 an existing output file is kept (`tests_pipeline.rs:352`, `invalid_input_exits_2_and_keeps_an_existing_output`). The row "2 with a results file written" therefore needs a fresh, empty output path for every step, or the Laboratory can show a stale file as the result of a failed run. §11.6 and §11.7 say outputs go into `work/` and are marked invalid on a limit breach, but do not say that each step gets a fresh path or that old outputs are removed first.

### N9. Persistence of the user's decisions is not designed (36.4, RN-33)

§11.7 has the Laboratory copy the Rulebook and ledger into `inputs/` and `review approve` act on the copy. 37.17 deletes the session directory at the end. So the approvals a user records in the Laboratory exist only until the session ends, unless something exports them, and exporting would be a Laboratory write of the ledger, which 36.4 forbids. RN-33 covers the initial copy only. Add the question to RN-33 or a new RN. The test of 36.4 (hash the ledger and Rulebook before and after) also needs to say which file it hashes.

### N10. Long proposal runs and the 300 s limit are handled but not recorded as an RN

§11.6 line 3348 states correctly that Requirement 29.1 allows a total deadline of 1 to 300 s per Provider_Request (verified, `requirements.md` 29.1), so a run with several requests, even at the 60 s default, can exceed the 300 s limit of 37.14. The design resolves it by M5 (no live run) and by a refusal rule under M2. It is marked OPEN with LT-1, but the conflict between 36.2 ("proposal generation only by starting a subprocess"), 37.14 (300 s) and live runs has no RN.

### N11. Test requirements do not cover every control that this checkpoint added

- 37.23 (and design §11.16 line 3491) demand a mutant that removes the check only for criteria 3, 4, 7, 9, 13, 14, 19, 20. The checkpoint extended the test list to criteria 24 to 26 but not the mutant list. Controls with no required mutant: 37.6 (one session), 37.8 (idle boundary), 37.11 (CSP), 37.16 (escaping), 37.21 (start refusal), 37.24, 37.25 (mutex, state version), 37.26 (upload limit), 36.4, 36.6, 36.9. Recommend adding at least 25, 26, 6, 24 and 21.
- 37.25 uses "a state of the session older than the current one" without defining state. The design defines a counter (§11.5). Say in the requirement or the design which events increment it, so the stale-state test is exact.
- 37.26 requires refusal "before it reads more of the body than the maximum allows". That is testable with an instrumented reader or with a known `Content-Length`; say which. Whether a refused upload counts toward the 64 MiB session total is not stated.
- The evidence sentence in §11.9 line 3406 ("the largest committed input of any kind is 10,639 bytes") is correct for input kinds (`crates/rt-ai-tool/tests/fixtures/live-run1-prompt-2/batch.json`, 10,639 bytes). The largest tracked file is `tests/fixtures/proposal-schema/proposal-schema-v2.json` (155,147 bytes), a schema and not an upload kind. Say "input of an upload kind".

## 5. What held up

### 5.1 Claims C1 to C8

| Claim | Result | How verified |
| --- | --- | --- |
| C1 | **Holds, with N1.** The only non-test `var_os` read of a credential is `rt-ai-responses/src/credential.rs:18` (`ProcessEnv::get`), reached through `Credential::from_env` called only from `rt-ai-tool/src/credential.rs:35`, called from `config.rs:403` (`load`, after `load_config`, `check_capabilities`, consent) and `config.rs:422` (`authorize_for_catalogue`, no capability or consent). `MockReplay` returns `None` at both sites. `--dry-run` returns at `command.rs:305` before `load` at `command.rs:317`. | `rg "var_os\|env::var\|env::vars"` over non-test, non-demo, non-xtask sources returned three hits: that line and the two `env::args_os` in the `main.rs` files. Files read as listed. |
| C2 | **Holds.** No `process::Command`, `Command::new`, `use std::process` or `Stdio` outside tests, `rt-xtask` and `rt-demo` anywhere under `crates/`. | `rg` over `crates/` with those exclusions returned nothing. Dependencies (`ureq`, `rustls`) were not scanned. |
| C3 | **Holds.** Both `scan` functions match an argument against the exact option names; `--` gives "'--' is not supported" (`cli.rs:121`, `args.rs:357`), `--name=value` is an unknown option (`cli.rs:123`, `args.rs:359`), and a value that starts with `--` is filtered out and reported as "needs a value" (`cli.rs:143`, `args.rs:381`). A value with a single leading `-` is accepted by the CLIs; the Laboratory's stricter refusal (§11.6) is therefore extra, not required by them. | Source read. |
| C4 | **Holds.** `decide_locked` takes `fs.try_lock` on `<ledger>.lock` first (`review.rs:254-255`), maps a held lock to `RT-APR-0212` (`review.rs:270`), runs recovery, loads, and calls `plan_review_with_store`. Under it, `plan.rs` step 5 returns `ReviewInvalidTransition` for a decided revision (line 269) and step 6 returns `ReviewStaleHash` when the supplied hash differs from the recomputed `rule_content_hash` (lines 285 to 296). The catalogue has `RT-APR-0203` (exit 3), `RT-APR-0204` (exit 3), `RT-APR-0212` (exit 70), `RT-APR-0214` (exit 2), `RT-APR-0201` (exit 3). Note the order: a stale hash on an already decided revision returns 0204, not 0203. The design's test list uses the undecided case for 0203, which is right. | Source and `catalogue/diagnostics.v1.json` lines 96, 98, 99, 107, 109. |
| C5 | **Holds, with N8.** `classify` sets exit 4 when `evaluation.diagnostics.has_errors()` or any Outcome is Indeterminate (`evaluate.rs:558-562`). `a_missing_value_gives_exit_4_and_complete_results` asserts exit 4, code `RT-EXE-0001`, a results file with `indeterminate`, and an identical file with and without a proof report. `a_provenance_error_gives_exit_2_with_complete_results_marked_not_verified` asserts exit 2, `RT-PRV-0002`, `not_verified`, and equal outcomes before the `verification` member. Published differences give exit 0 with a warning (`tests_published_kinds.rs:125-146`, codes `RT-EXE-0011`, `0012`, `0013`). A difference with an Indeterminate ranking gives exit 4 (line 154). Tests were read, not run. | Source read. |
| C6 | **Holds, with the wording remark in N11.** `limits.rs` has `MAX_SOURCE_DOCUMENT_BYTES` 16,777,216; `MAX_RULEBOOK_BYTES` and `MAX_LEDGER_BYTES` 8 MiB; `MAX_COMPETITION_DATA_BYTES` 32 MiB; `MAX_AI_CONFIGURATION_BYTES` 64 KiB. `demo/stand-in-ai/documents/stand-in-rules.md` is 547 bytes. `git ls-files` shows no CSV fixture (only `crates/rt-data/src/tabular/csv.rs` and its unit tests). The largest non-golden input is the 10,639-byte batch above. The table values do not exceed the CLI limits (16 MiB upload against the 32 MiB CLI limit for data). Req 1.9 (16 MiB) and 19.9 (accept equal to the limit) confirmed at `requirements.md` lines 218 and 779. The values stay "Recommended-pending" (LT-8), which is honest because the snapshots of task 11.1 do not exist. | `rg`, `git ls-files`, `stat`. |
| C7 | **Holds.** Both criteria are quoted exactly as `requirements.md` lines 1237 and 1238 read. `review list` text shows `origin: manual` for Proposed revisions (`tests_review_adopt.rs:128`). `authored-script.json` says "authored by hand ... not provider output" (line 9); `RUNBOOK.md:9` and `RECORDINGS.md:4` say the same; `mock-replay.toml` exists next to the two recorded scripts. `RuleBody::Ranking` has `key`, `direction`, `tie_breakers`, `ties` only (`rt-ir/src/rule.rs:183-191`). `Question::Ranking` has `participants: Option<BTreeSet<ParticipantId>>` (`rt-data/src/questions.rs:27`), which supports the §11.3 constraint. `RANKING_SCOPE_LABEL` exists at `rt-explain/src/report/proof.rs:386` and `FIELD_SET_BRACKET` is used in `tests_app_report.rs`. Task 11.8 is "Deferred" (`tasks.md:108`, 11.8 listed as Deferred), and no required-statement list exists in the code. The design rewrites neither criterion. | Source and file reads. |
| C8 | **Holds.** `transport.rs:56` sets `.proxy(None)` and the header comment says the agent never reads a proxy variable. `proxy_env.rs` sets `HTTP_PROXY`, `http_proxy`, `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY`, `all_proxy` and asserts zero connections to the proxy stub. `ureq` is built with `default-features = false, features = ["rustls"]`, `rustls` with only `std`; `Cargo.lock` lists `webpki-roots` as the `ureq` root store and no `rustls-native-certs` or platform verifier (the lock's ureq entry, lines 818 to 832). A certificate-file variable such as `SSL_CERT_FILE` is not read by that stack. The test was read, not run. | Source, `Cargo.toml`, `Cargo.lock`. |

### 5.2 Answers to Q1 to Q6

**Q1. Credential isolation claims.** One over-claim: B1 (M5 P1-os). Everything else is claimed no stronger than the evidence: the design says it claims P1-code only (§11.10 line 3415, D-19), rates M2 P5 "Unproven", and calls M6 the only P1-os mechanism. M1's rejection rests on a true premise (the value is in the Laboratory's environment block from its start; UD-38 D2 says such a mechanism is rejected). M2's "same-user readable" and M3's "the Laboratory can connect to the same socket" are reasonable for an X5 same-user actor. The statements that the Laboratory, as parent, may trace and read a child it started are consistent with M2 and M3 having no P1-os. M4's rejection premise is true (N5, citation only). M7's rejection (no terminal in the canary test) and M6's (needs administrator set-up) are true by construction. I did not verify the claims about `keyring-core` and per-platform store crates pulling D-Bus; those are marked "to be decided at L1b" and stay unverified. The five properties (P1 to P5) are demonstrated for no mechanism yet; the document says so.

**Q2. Missing threats.** See N7 and N6. Covered: DNS rebinding, cross-port cookie, cookie tossing, SameSite with IP hosts, clickjacking, referrer, symlink swap. Missing or thin: request smuggling edge cases, slowloris values and connection cap, browser cache (`no-store`), prefetch, Windows paths and ACLs, extensions, multipart parser, Launch_Secret scrollback, lifetime and rate.

**Q3. Contradictions without an RN.** Recorded: exit-code classification vs 36.21 (RN-31), copying files vs 36.4 (RN-33), time field vs 36.6 (RN-40), `--`/`name=value` vs 37.14 (RN-32). Not recorded: B3 (24.13 edges), B2 (37.25 vs 37.17), N3 (37.20 canary under M5), N9 (persistence of decisions vs 36.4), N10 (300 s vs live propose vs 36.2), LT-5 vs 37.5 (N6). The 300 s case is handled in the design text (§11.6) but not as an RN.

**Q4. Open items closed or silently decided?** None closed. 37.21 text in `requirements.md` line 1327 still says OPEN; the threat model status line and §9 mark LT-1 to LT-8 OPEN; design §11.17 repeats it. 34.10 is not rewritten (§11.14 line 3478), RN-34 to RN-38 await a decision. Soft decisions that go further than "recommended": §11.13 says the demo label "must" say "authored script" (ahead of the RN-35 decision); §11.6 says that under M5 the Laboratory starts `ruletrace-ai` only with `mock-replay` and `--dry-run` (this fixes the live behaviour before LT-1 is decided, although it is flagged "OPEN with LT-1"); `docs/security/laboratory-threat-model.md` §6.3 item 1 says "Now (L2 and L3)" for M5. Reword to "if LT-1 selects M5" when the user decides.

**Q5. 37.25, 37.26 and the checkpoint record.** Both are testable and fit the rest of 37, with the exceptions N6 (exchange status), N11 (state definition, "before it reads", session total, no mutant requirement) and B2. 37.25 matches the user's instruction (one active session, serialized or stale-guarded, approval rechecks revision and hash in any tab, tests for two tabs, concurrent changes, stale hashes, second session). 37.26 matches the limits instruction (explicit upload maximum, no unbounded body, 413). The 37.14 clarification matches the instruction on overflow, termination, reaping and cleanup. The traceability section (lines 985 to 1008) records all seven points faithfully as I read them: single session approved, no inline styles approved, Host exactness approved, 37.21 "Not closed" with the five properties, 34.10 "Kept open" with the instruction to quote and compare and the exit-4 attention, limits approved with the added upload maximum, no unbounded bodies, no silent truncation, reaped and cleaned up. The old and new texts of 37.6, 37.14 and 37.23 shown there equal the diff. I cannot check the user's original wording; I compared against the summary in the review request. One small point: the record says the upload maximum is "chosen after checking the planned snapshots and fixtures"; the design says the check found no snapshot and no CSV fixture and leaves the value pending. The record's last paragraph says so, so it is faithful, but the table row alone reads as complete.

**Q6. Test sufficiency.** Not sufficient as planned: see N11 for the mutant list gap. The design correctly says "no test was run for this review" (§11.14 line 3461) and the threat model labels its code facts "read". I found nothing in the documents that claims a test or mutant was run that was not. Planned tests that would fail if a control were removed exist for 3, 4, 7, 9, 13, 14, 19, 20 by requirement. The stand-in approach (recording stand-ins written by a child process) is sound for 36.2, 37.14 and 37.21. The T17 residual (no kill on parent death) is accepted openly (LT-7).

### 5.3 Other things that held up

- The requirement edits are exactly the ones named: `git diff` of `requirements.md` shows 6 and 14 extended, 23 changed to "24 to 26", 25 and 26 added, nothing else.
- The UD-38 §8 table lists 12 items; each points to a design section that exists (§11.4 to §11.13, §11.5, §11.11). "OPEN" and "Designed; ... open" labels match the design.
- Steering conformity: no steering rule is contradicted by §11 (the Laboratory holds no credential, has no ledger-append path, calls no evaluation function, starts the two CLIs by argument vector). The amendment note at design line 3108 says the steering needs a later change only if LT-1 selects an OS credential store; that matches `rust-engineering-and-testing.md`.
- Commit `fc0d799` cited in design 9.3 item 12 exists (`git cat-file -t` returns `commit`).
- `getrandom` is in `Cargo.lock` under `ring` (0.2.17) and under `proptest` (0.4.3). `tools/dependencies.v1` states that `std` has no entropy API, so the design's "or a stable `std` API" hedge cannot resolve to `std` and L2 needs a dependency decision. `ci.yml` has an `ubuntu-24.04, macos-15, windows-2022` matrix, so P5's "Linux, macOS, Windows in CI" is true of the existing workflow.
- The 16 MiB combined-output counting by two reader threads and `Child::kill` plus `Child::wait` (§11.6) is consistent with 37.14's "terminate, reap". Process tree depth one is supported by C2. V-1 (kill during a store write) is correctly left to L4.

## 6. Commands run

All inside the repository or the scratch worktree; none executed project code, so the loopback wrapper was not needed.

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD`, `git status --porcelain`, `git log`, `git diff --stat 6bde3b8..d0c821c` (main worktree) | 0 |
| `git branch --show-current` | blocked by the permission rule (not run); branch read from `.git/HEAD` instead (`ref: refs/heads/m3`) |
| `git worktree add --detach ~/review-wt-L1 d0c821c` | 0 |
| `git diff 6bde3b8..d0c821c` for requirements, traceability, UD-38, and design (diff only) | 0 |
| `rg` and `grep` searches over `crates/`, design, requirements, `Cargo.lock`, `tools/` (about 25 calls) | 0 |
| `git ls-files ... | stat | sort` (file sizes), `git cat-file -t fc0d799` | 0 |
| A search for credential function references and a first-lines read of `credential.rs` through the shell | blocked by the tool; redone with the file reader |
| A command to read `/proc/<pid>/environ` of a child (for B1) | blocked by the repository hook (credential exposure); not run |
| `cargo` (any subcommand) | not run |

## 7. Mutants run

None. This review is of documents, and the repository's permission rules deny `cargo test`. The mutants that the change calls for belong to L4. Mutants I would run once code exists: remove the exact-Host comparison (37.3); remove the Origin check (37.4); remove the CSRF check (37.9); remove the state-version comparison and the mutex (37.25); skip the `Content-Length` check before reading (37.26); drop `env_clear()` (37.14, 37.20); accept a `Transfer-Encoding` header; accept a second `rt_lab_session` cookie (37.24).

## 8. Not verified

- No test, build, clippy or `cargo deny` run. The tests cited in C5, C7 and C8 (`tests_pipeline.rs`, `tests_published_kinds.rs`, `proxy_env.rs`, `tls_verification.rs`) were read, not executed, so "the tests pass" is not established here.
- The Linux claim for B1 (a same-user process can read another process's environment, Yama scope 1 behaviour) was not tested; the hook blocked the test.
- `keyring-core`, per-platform store crates, D-Bus dependency, Secret Service and keyutils behaviour (M2), the Windows `SystemRoot` need (V-3), safe signal handling (V-2), and whether `std::fs::File::try_lock` behaves the same on all three platforms for the session lock (§11.7). The pinned toolchain `1.99.1` was not checked against the std API used.
- The user's original instruction text (checkpoint) is known to me only through the review request summary.
- Dependencies of `ureq` were not scanned for subprocess use (C2) or for environment reads (C8) beyond the lock file and the explicit `.proxy(None)`.
- Requirements 28, 35 and the full Requirement 27 table were read in part only (27.1, 27.2 to 27.4, the table).

## 9. Integrity

| | `git rev-parse HEAD` | `git status --porcelain` |
| --- | --- | --- |
| Before | `d0c821cc14cbb800164775681810c44cbe88f4aa` | empty |
| After | `d0c821cc14cbb800164775681810c44cbe88f4aa` (unchanged) | one untracked path, this report `docs/reviews/2026-11-18-review-L1-threat-model-d0c821c.md` (the one permitted write); no tracked file changed |

The scratch worktree `~/review-wt-L1` was removed at the end with `git worktree remove --force` (exit 0; `git worktree list` shows only the main worktree). No commit, push, reset, checkout or branch switch was made.
