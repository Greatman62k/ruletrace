# Review src-fixture-7934de8: dedicated application source fixture

## 1. Verdict

**APPROVED WITH FINDINGS.** No blocking finding. The v2 candidates are ready
for the user's golden approval, subject to the approval presentation in
section 4 (F-1) and the non-blocking notes below.

## 2. Scope

- Base: HEAD 7934de8 on branch m3. The change is uncommitted, so there is no
  commit range.
- Examined, read-only in the main worktree: `git diff` of
  `crates/rt-tests-core/tests/app_report_goldens.rs` (175 insertions, 77
  deletions) and the new file
  `crates/rt-tests-core/tests/support/app_source.rs` (77 lines).
- Examined outside the repository: `/home/sam/golden-staging/app-reports`
  (v1), `app-reports-v2`, `app-reports-v1-to-v2.diff`, `strip_compare.py`,
  `strip_compare.out`.
- Also read: `tools/hooks/hooklib.py` (protected patterns),
  `tools/hooks/guard_golden.py`, `support/app_eval.rs`.
- Built and tested in the scratch worktree `~/review-wt-src` at 7934de8 plus
  the two files, target dir `~/review-target-src`. Both removed.

## 3. Blocking findings

None.

## 4. Non-blocking findings

Severities: LOW or NOTE. None blocks.

- **F-1 (LOW, placement).** The source document lives in test code
  (`support/app_source.rs`). This path is not protected by `guard_golden`
  (`hooklib.PROTECTED_PATTERNS`: `demo/*/golden/**`, `tests/fixtures/**`,
  `crates/*/tests/fixtures/**`, `crates/*/tests/golden/**`), so the document
  text, spans and rulebook JSON were written without a guard prompt. I judge
  this does not circumvent the hook's intent. The hook protects expected
  results, and the expected results are the six goldens. At bless, each of
  the six case directories gets `app-rules.md`, `source-model.json` and
  `compiled.json` (verified by regeneration, see section 5). The test then
  compares the code-produced bytes against those protected copies, so any
  later edit of `TEXT`, the span table or the rulebook JSON fails six golden
  comparisons until a reviewed re-bless. This matches the accepted precedent
  (the earlier test-local `elig.qual` rule and its `compiled.json`).
  **Recommendation: accept the placement.** Condition: the golden approval
  must name `app-rules.md`, `source-model.json` and `compiled.json` in each of
  the six directories as authoritative expected bytes, and show the document
  text and sha256 `57685b6d…431e`, because these are new specification
  content, not only regenerated reports. Optional follow-up, not required:
  keep one canonical `app-rules.md` under `tests/fixtures/` and read it in the
  test. That would put the text under the guard before the bytes exist in
  code.
- **F-2 (NOTE, clause 3 wording).** Clause 3 says equal competitors "share the
  same position". It does not state that the next position skips ahead
  (1, 1, 3, 4). The engine's competition ranking does skip, and the goldens
  show it. The clause does not contradict this and is not ambiguous about the
  rule it cites, but a reader cannot derive "3" from the text alone. Req 34.4
  says "verified" does not prove interpretation, and the README says the same.
  No change needed.
- **F-3 (NOTE, boundary not exercised).** Scores are 7.6, 7.8, 6.0 and 7.8, so
  none equals 7.7. Clause 4 "at least 7.7" matches `ge` by reading and by the
  compiled expression. The expected outcomes do not distinguish `ge` from
  `gt`. Mutant M5 (below) survived the expectation check and would be caught
  only by the `compiled.json` golden comparison after bless. This data is
  unchanged from v1. Equality at the threshold is the deferred
  Tie_Group-at-threshold topic and I did not change it.
- **F-4 (NOTE, scope of `strip_compare.py`).** The script is sound for its
  purpose, with the limits listed in section 5, item 4.
- **F-5 (NOTE).** All six rules cite the clause that states them, and
  `rank.standing` is reported together with `score.weighted` (clause 2) in
  `rules`. The ranking reports do not cite `elig.qual` or `elig.basic`, so
  RN-36 ("eligibility does not filter ranking") is consistent: clause 3 says
  "Competitors" without an eligibility condition, and the rank rule has none.

## 5. What held up

1. **Citations.** Clause 3 against compiled `rank.standing` (key `score`
   desc, tie-breaker `judges` desc, `ties: group`):
   "ranked by score, highest first" is key desc. "Equal scores are separated
   by the higher judges score" is tie-breaker `judges` desc. "Competitors
   equal on both share the same position" is `ties: group`. It implies no
   further tie-breaker and no eligibility filter, and I found no
   alternative reading that changes an outcome. Clause 4 against `elig.qual`
   (`score ge 7.7`): "at least" is `ge`. Clause 2 against `score.weighted`
   (0.6 judges + 0.4 technical, no rounding): matches. Clause 1 against
   `elig.basic` (`age ge 16 and licence`): matches. The scoring clause is no
   longer reused for ranking: in the v2 reports `rank.standing` cites
   171..327 (clause 3), `score.weighted` cites 92..170 (clause 2), and
   `elig.qual` cites 328..383 (clause 4). Checked in `report.json`/`report.txt`
   of `bracket-qualification-weighted-score` and by the diff of two other
   cases.
2. **Spans and hashes.** I recomputed with Python on the staged
   `app-rules.md`: 384 bytes, sha256
   `57685b6d15dae6272fe4f644a0ed2719851666ef7330efc7f861c421eb19431e`.
   `source-model.json` has the same `sha256`, `bytes: 384`, and its
   `content` equals the file bytes. All five `sha256` fields in
   `compiled.json` equal that hash. Clause offsets 21..91, 92..170,
   171..327, 328..383 with lines 3 to 6 and column 1 are correct, and each
   span ends before a `\n`. The Explainer reports `provenance: verified (4
   of 4 traces)` in the qualification case because the document hash matches
   and each span resolves, which is the unchanged meaning of "verified".
3. **Real path.** `build_app` runs `ruletrace ingest`, then `review approve`
   once per rule (the first with `--create-ledger`), `review list` to assert
   `Approved` for each rule, `validate`, and `compile`, all through
   `rt_cli::run` over the in-memory file system of `support/app_eval.rs`. The
   content hash is computed with `rule_content_hash`. No ledger text is
   written by the test. Expected citation values in `check_citations` are
   written by hand and not derived from the rulebook builder.
4. **Outcomes unchanged.** I re-ran `strip_compare.py`: `RESULT OK`, exit 0.
   It strips only the header `rulebook_hash`, the fields inside `sources`
   other than rule, revision and code, each diagnostic `location`, the
   `rulebook sha256:` line, the source line and its quoted line in the
   text report, and the `at <loc>` suffix of `RT-EXE` lines. It keeps
   results, ranks, counts, provenance status, codes, messages, exit-relevant
   diagnostics, rule lists and the order and count of sources. It also asserts
   v1 locations equal `rules.md 92..170` and v2 locations equal the clause
   table, and checks quoted text starts with the right clause number.
   Hand check of two cases by `sed`-normalised diff of `report.txt`:
   `bracket-published-breaks-tie` differs only in the `rank.standing` source
   line, its quote, and the `score.weighted` document id.
   `bracket-standings-indeterminate` differs in the same lines plus the
   `RT-EXE-0006` location suffix (`rules.md bytes 92..170 line 4` to
   `app-rules.md bytes 171..327 line 5`). Message text, error severity and
   the missing input `judges` are identical.
5. **Unaffected cases.** The five unaffected cases are byte-identical v1 to v2
   (script output, 7 or 8 files each). Their test names also still pass
   their code path unchanged (`Rulebook::Basic`). `git status --porcelain` in
   the main worktree shows only the two paths. The new `source-model.ref`
   handling stays for `Basic` only.
6. **Regeneration.** I ran the test binary with `RT_APP_GOLDEN_BLESS` pointing
   at a directory outside the repository (`~/review-regen`, in the scratch
   worktree run, not a bless against the repository). The result: 14 passed.
   `diff -r ~/review-regen app-reports-v2` printed nothing, so v2 is exactly
   what the code under review produces. The directory was deleted.
7. **Tests.** `check_citations` fails if any citation of the four rules is
   wrong or absent (it asserts at least one citation was found).
   `app_source_spans_equal_the_hand_written_offsets_and_cover_one_clause_each`
   compares the hand table to independently found spans, checks char
   boundaries, the clause prefix, no newline inside a span, length 384, ASCII,
   and the phrases "highest first" and "at least 7.7.". Mutants in section 7.
8. **Gates.** Workspace test: 2395 passed, 11 failed, 0 ignored. The 11
   failures are exactly the 11 `app_report_golden_*` comparison tests, which
   fail with "missing or unreadable" because no app-reports directory exists
   at 7934de8, including the five unaffected cases. Clippy with
   `-D warnings` is clean. `rustfmt --check` on the two files is clean.
   The three tests that need no goldens pass (the span test, the bless-target
   test and one more).

## 6. Commands run (exit codes)

All cargo commands ran inside
`unshare -rn sh -c 'ip link set lo up && exec "$@"' sh <cmd>`.

- `git rev-parse HEAD`; `git status --porcelain`; `git diff`; `cat` of the
  new file: 0.
- `git worktree add --detach ~/review-wt-src 7934de8`, copy of the two files: 0.
- A first `cargo test` attempt run as `CARGO_TARGET_DIR=... unshare ... cargo`
  with a `~` path in a subshell failed with "could not find Cargo.toml"
  (exit shown 0 because of a pipe). I re-ran with an exported absolute
  `CARGO_TARGET_DIR`. This was a command-line error of mine, not a finding.
- `cargo test -p rt-tests-core --test app_report_goldens`: 101 (3 passed, 11
  failed: goldens absent, expected).
- Same with `RT_APP_GOLDEN_BLESS=/home/sam/review-regen`: 0 (14 passed).
- `diff -r review-regen app-reports-v2`: 0 (identical).
- `python3 strip_compare.py`: 0, `RESULT OK`.
- Python recomputation of sha256, offsets, lines, columns: 0.
- Mutant runs (section 7): each exit 101 as expected, files restored (diff
  against saved copy: 0).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: 0.
- `cargo test --workspace --all-features --no-fail-fast`: 101. Totals
  2395 passed, 11 failed, 0 ignored.
- `rustfmt --edition 2021 --check` on the two files: 0.
- `git worktree remove --force`, `rm -rf` of my scratch target, regen and
  log files, `git worktree prune`: 0.

## 7. Mutants run

All applied in the scratch worktree and reverted.

| Id | Mutant | Result |
|----|--------|--------|
| M1 | `rank.standing` span set to clause 2 (`s2 = s[1]`) in `rulebook_json` | Killed by `check_citations` in `bracket_standings_tie_group`: message `rank.standing: "app-rules.md","start":92,...`. |
| M2 | `SPANS[3]` end 383 to 382 | Killed by `app_source_spans_equal_the_hand_written_offsets_and_cover_one_clause_each` (left/right span lists differ). |
| M3 | Clause 4 text "at least" to "over" (changes lengths) | Killed by the span test (offsets shifted). |
| M4 | Clause 3 "highest first" to "lowest first" (changes lengths) | Killed by the span test through the offset shift. A same-length wording change would be caught by the span test's `contains("highest first")` assertion, and after bless by the golden comparison. I did not run a same-length variant. |
| M5 | `elig.qual` operator `ge` to `gt` | Survived `check_expectations` (no score equals 7.7); failure seen was only the missing golden. After bless the `compiled.json` comparison would catch it. See F-3. |
| M6 | Tie-breaker `judges` direction desc to asc | Killed by `check_expectations` in `bracket_published_equal` (published differences `[2,0,0]` expected `[0,0,0]`). |

Note on M1 to M4: with goldens absent every comparison test fails, so I read
the panic text to attribute each kill to the specific assertion, and did not
count a bare FAILED line.

## 8. Not verified

- Comparison against committed goldens: none exist at 7934de8 and I did not
  write under `tests/fixtures/**`. Equality with v2 was shown by regeneration
  into a directory outside the repository.
- Same-length wording mutants of the clause text (M4 variant).
- CI behaviour, the `cargo deny` check and the dependency-graph checks: not
  run, as the change touches only test files and adds no dependency.
- Whether v1 (`app-reports`) was ever intended to be approved. I treated it
  only as the comparison baseline, as instructed.
- No provider or credential was involved. Nothing was read or called.
- The independent `fmt`/`clippy` result covers the scratch worktree with the
  two files, not a separate clean checkout of the final commit.

## 9. Integrity

- Main worktree HEAD before: `7934de8d4b9f06b854c6b96fcf00141bc29b010a`.
  After: unchanged.
- `git status --porcelain` before:
  ` M crates/rt-tests-core/tests/app_report_goldens.rs` and
  `?? crates/rt-tests-core/tests/support/app_source.rs`.
  After my scratch cleanup: identical (checked before writing this report).
  The only later difference is this report file under `docs/reviews/`.
- No file under `tests/fixtures/**` or other protected paths written. No
  bless against the repository. No stage, stash or commit.
- A file `~/review-w5-probe.rs.txt` exists in the home directory. It is not
  mine and I did not touch it.
