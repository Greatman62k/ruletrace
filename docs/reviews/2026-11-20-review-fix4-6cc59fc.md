# Review fix4-6cc59fc

## 1. Verdict
APPROVED (local evidence). D3, D4 and the Windows clippy of three crates still need CI.

## 2. Scope
Range 890fa85..6cc59fc (7cdbc4d, fdc4c8f, 34f46f2, 6cc59fc). Files: crates/rt-demo/tests/live_guard.rs, crates/rt-tests-core/tests/{app_report_goldens,proptest_floor_scan,workflow_module_environment}.rs. Logs of CI run 44716508559 (jobs 136221163823, 136221163845, 136221163709) read with `gh api`, ANSI stripped.

## 3. Blocking findings
None.

## 4. Non-blocking findings
- Windows clippy log: cargo stopped after the first error ("waiting for other jobs to finish"), so that log cannot show other errors. Local Windows-target clippy (below) found none outside the three ring crates, which CI must confirm.
- D3 on Windows: the first failed assertion aborts the test, so the CI log (app_report_goldens.rs:971) could not show later ones. Reading them: the remaining assertions use "rel/dir" and "" (not absolute on any platform) and the new `C:\x` is absolute. No further failure is expected.
- D2 mutant covers `list_path` only. Removing the `list_path` call in `workspace_sources` (proptest_floor_scan.rs:63) is not detectable on Linux. Not run.
- D4 on macOS: the race test finally asserts `busy == 0` (ETXTBSY, errno 26, Linux only). That is trivially true on macOS, so the test checks little there. This was already so before the fix.

## 5. What held up
- CI failures match the stated causes exactly, and are the only ones:
  - D1: clippy windows, `unused import: Command`, workflow_module_environment.rs:60:20 (log line 270). One error.
  - D2: test windows, proptest_floor_scan `the_workspace_has_no_unprotected_property_test`, backslash paths in left list (log 3014-3016), right `[]`.
  - D3: test windows, app_report_goldens.rs:971, left `Err("... must be an absolute path, got '/tmp/x'")`, right `Ok(Some("/tmp/x"))`.
  - D4: test macos, live_guard.rs:278:30, `NotFound` from `Command::new("/bin/true")`, 4 threads, plus the scope panic at 271.
  - Windows: "2 targets failed" (app_report_goldens, proptest_floor_scan). macOS: "1 target failed" (live_guard). Both ran `--no-fail-fast`. grep for FAILED, `test result: FAILED` and `error: test failed` found no others.
- Diff touches only test files. No production file changed. `bless_target` (app_report_goldens.rs:70-84) is unchanged (it is test-side code and not in the diff).
- D1 fix: `Command` removed from the import. The only use, workflow_module_environment.rs:453, is in a `#[cfg(unix)]` test and now uses the full path.
- D3: every original assertion is kept, with `/tmp/x` replaced by `ABSOLUTE` (`C:\x` on Windows, `/tmp/x` elsewhere). A Windows-only assertion that `/tmp/x` is refused with "absolute path" was added. Nothing else is removed.
- D4: the 4 spawner threads still run concurrently inside `thread::scope` (live_guard.rs:272-283) and the installer logic and `assert_eq!(busy, 0)` are unchanged. `/bin/sh -c :` exists on Linux and macOS.
- D2: new regression test `windows_separators_match_the_crate_rule_and_the_list` uses `scan()` with a crate rule file and a below-floor list entry, expecting no findings.

## 6. Commands (all cargo inside `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh`, CARGO_TARGET_DIR=~/review-target-fix4)
- `gh api --allow-escape-sequences .../jobs/{id}/logs` x3: 0.
- Clippy at 890fa85 (worktree ~/review-wt-fix4-base), `--offline --target x86_64-pc-windows-gnu -p rt-tests-core --all-targets --all-features -- -D warnings`: exit 101, same `unused import: Command` at line 60:20 (red).
- Same at 6cc59fc: exit 0 (green).
- Windows-target clippy, `--workspace --exclude rt-ai-responses --exclude rt-ai-tool --exclude rt-tests-ai --all-targets --all-features -- -D warnings` at 6cc59fc: exit 0.
- `cargo test --offline -p rt-tests-core --test proptest_floor_scan` with the mutant: exit 101.
- `cargo test --offline --workspace --all-features --no-fail-fast` at 6cc59fc: exit 0. 140 result lines: 2463 passed, 0 failed, 0 ignored. This matches the claim.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`: exit 0.

## 7. Mutants
- `list_path` without `.replace('\\', "/")`: killed by `windows_separators_match_the_crate_rule_and_the_list` (proptest_floor_scan.rs:350). The other 9 tests in that file passed, including the workspace scan, because Linux paths have no backslash. Reverted, and `git status` of the worktree was clean.
- No mutant run for D3 or D4. The platform-specific behaviour cannot occur on this host.

## 8. Not verified
- D3 (`C:\x` and the Windows-only assertion) on Windows. D4 (`/bin/sh` race test) on macOS.
- Windows clippy of rt-ai-responses, rt-ai-tool and rt-tests-ai (ring cannot be cross-built here).
- Windows and macOS tests beyond the failures logged. I did not run a new CI.
- No red run of D3 and D4 against 890fa85 locally (platform-only defects).

## 9. Integrity
Main worktree before: HEAD 6cc59fcf67e4d741df313c641406082bccd61c70, status empty. After: HEAD unchanged (6cc59fcf67e4d741df313c641406082bccd61c70). Status shows only `?? docs/reviews/2026-11-20-review-fix4-6cc59fc.md`, this report. Worktrees and target dir removed.
