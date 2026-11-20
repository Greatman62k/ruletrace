# Review final-9.4-ecf9550

## 1. Verdict
APPROVED

## 2. Scope
Range 31e4377..ecf9550 (715db8c, ecf9550). Files: rt-cli `src/evaluate.rs`, `src/lib.rs`; rt-explain `src/report/common.rs`, `src/tests_app_gaps.rs`, `src/tests_app_report.rs`; rt-tests-core `tests/difference_kind_table.rs`; rt-demo `tests/live_guard.rs`. Worktree `~/review-wt-final94`, target `~/review-target-final94`.

## 3. Blocking findings
None.

## 4. Non-blocking findings
- No Bracket golden file exists in the repo. `demo/*/golden/**` contains only Prize text and is untouched by the range (`git diff --stat -- demo tests` is empty). The "hand-written goldens" for Bracket are the in-code expectations `bracket_human_matches_the_hand_written_layout` and `bracket_json_matches_the_hand_written_document` (`tests_app_report.rs` about 200-270). They build the line from the `BRACKET_SCOPE` constant, so by themselves they would follow a wrong constant. The literal is pinned independently by `assert_eq!(BRACKET_SCOPE, "...")` in `tests_app_gaps.rs:271`.
- `a_freshly_installed_script_runs_while_other_threads_spawn` is statistical (1600 installs). It fails reliably with the old helper (below) but cannot prove the race absent. It takes about 2.6 s.
- I did not reproduce the claimed 3% rate in a scratch harness.

## 5. What held up
(a) rt-cli visibility
- The diff makes `difference_code` `pub` and adds `pub use evaluate::difference_code;`. The function body is unchanged. `mod evaluate` stays private. This is the minimum for a cross-crate call. rt-tests-core already depended on rt-cli, and Cargo files are unchanged. `scan-api` is ok.
- The new test `the_evaluate_command_and_the_report_table_agree_on_every_code` asserts, for each kind in `ALL`, `rt_cli::difference_code(kind)` against the code in `expected()` (written from Req 13.12) and against the table row's code. It also panics if the row is missing.
- Mutants:
  - rt-cli `TieNotInComputedResult` mapped to `PublishedTieGroupSplit`: the new test FAILED (left PublishedTieGroupSplit, right PublishedTieNotComputed).
  - rt-explain `difference.rs:51` changed the same way: the new test FAILED. `every_engine_kind_has_a_row_with_its_name_and_code` and `the_catalogue_describes_each_code_with_its_kind` also FAILED.
  - Scratch variant `ScratchKind` added to `DifferenceKind`, with every production match fixed: `rt-tests-core` test `difference_kind_table` fails to compile (E0004 at `difference_kind_table.rs:35` and `:48`, `expected` and `ordinal`). A variant added only to the engine fails to build earlier, in `rt-engine` (E0004 at `compare.rs:390`).
  - All mutants were reverted. The worktree is clean (`git status --porcelain` empty).

(b) Bracket sentence
- The literal "Standings are calculated solely from the declared ranking rules; this report does not apply eligibility filtering." occurs once each in `common.rs:27`, `tests_app_report.rs:18` and `tests_app_gaps.rs:272` (`grep -cF`). The match is byte-exact.
- The Prize `SCOPE` and `report/proof.rs` are unchanged (0 diff lines). `header.ranking_scope` is still asserted in `FIELD_SET_BRACKET`. Bracket field names have no eligibility entries.
- Removed assertions were `BRACKET_SCOPE.ends_with(tail)`, `SCOPE.ends_with(tail)` and `BRACKET_SCOPE.starts_with(...)`. They are replaced by exact `assert_eq!` on the full sentence plus `assert_ne!(BRACKET_SCOPE, SCOPE)`. The human and JSON `contains` checks and the Prize and `!prize.contains(BRACKET_SCOPE)` checks remain.
- Mutants:
  - Dropping "solely" in `common.rs`: 3 tests FAILED (the gaps test and both hand-written layout tests).
  - Dropping the final "." in `common.rs`: the same 3 tests FAILED.
  - Both were reverted.

(c) ETXTBSY fix
- The explanation is correct. A fork by another thread copies the open write descriptor, and the child keeps it until its own exec. While any writer is open, exec of that inode returns ETXTBSY. The descriptor is closed on exec, not on fork. The restriction is on the inode, so link or rename would not help.
- Writing from a `/bin/sh` child confines the writer descriptor to a process that has exited before `status()` returns. The parent holds no write descriptor, so no other thread's fork can copy one.
- Non-vacuity, run by me: I replaced `write_executable` with the old `fs::write` plus `set_permissions` and ran the test 5 times. It failed 5 of 5 (`scripts that failed to execute with ETXTBSY`, `live_guard.rs:297`). With the committed helper it passed 10 of 10 isolated runs, and passed in the full rt-demo run.
- The change is confined to `crates/rt-demo/tests/live_guard.rs`, which is test code. The diff has no production file.

(d) Gates: see section 6.

## 6. Commands run (all in the worktree, loopback wrapper where code runs)
- `cargo test -p rt-explain -p rt-cli -p rt-tests-core --all-features`: no FAILED lines.
- `cargo test -p rt-demo --all-features`: all suites ok (40 in `live_guard`).
- `cargo test --workspace --all-features --no-fail-fast`: exit 0. 2160 passed, 0 failed, 0 ignored (sum of `test result` lines).
- `cargo clippy -p rt-cli -p rt-explain -p rt-tests-core -p rt-demo --all-targets --all-features -- -D warnings`: exit 0.
- `rustfmt --edition 2021 --check` on the 7 changed Rust files: exit 0.
- `cargo run -q -p rt-xtask -- check-graph check-tree scan-sources scan-api check-decisions check-catalogue check-justification`: exit 0. All seven print ok. `check-catalogue` skipped its baseline because no release tag exists.

## 7. Mutants run
| Mutant | Result |
| --- | --- |
| rt-cli code for TieNotInComputedResult | killed by `the_evaluate_command_and_the_report_table_agree_on_every_code` |
| rt-explain table code `difference.rs:51` | killed by the new test, `every_engine_kind_has_a_row_with_its_name_and_code` and `the_catalogue_describes_each_code_with_its_kind` |
| Scratch `DifferenceKind::ScratchKind` | the test target fails to compile (E0004) |
| Bracket label without "solely" | killed by 3 rt-explain tests |
| Bracket label without final "." | killed by 3 rt-explain tests |
| Old `write_executable` helper | the regression test fails 5 of 5 |

## 8. Not verified
- The 3% race rate from the scratch harness.
- Windows and macOS behaviour (the race test is `cfg(unix)`).
- CI results.
- `cargo deny` and the Windows cross-check, which were not requested.

## 9. Integrity
Main worktree before: HEAD ecf9550b70c9f50efbcd145445d92248761804b2, status empty. After: HEAD unchanged (ecf9550b70c9f50efbcd145445d92248761804b2); status shows only the untracked report file `docs/reviews/2026-11-17-review-final-9.4-ecf9550.md`. The scratch worktree and target directory were removed.
