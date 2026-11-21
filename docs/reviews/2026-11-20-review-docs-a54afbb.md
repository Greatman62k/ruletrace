# Review: uncommitted documentation changes on m3 (baseline docs-a54afbb)

## 1. Verdict
APPROVED. No blocking findings.

## 2. Scope
Uncommitted diff on `m3` at HEAD `a54afbb`: `.kiro/specs/ruletrace-compiler/tasks.md` (line 1420),
`.kiro/specs/ruletrace-compiler/traceability.md` (lines 1305-1312),
`docs/kiro-usage-log.md` (line 58). Sources checked: commits `c6da862`, `5644e52`,
`466de67`, review `docs/reviews/2026-11-20-review-once-c6da862.md`, CI runs
`44711229145` and `43896108442`. No code was built or run.

## 3. Blocking findings
None.

## 4. Non-blocking findings
- N1. tasks.md:1420 says "`main` and `m3` are unchanged there". The remote shows
  `main` = `5644e52` and `m3` = `5eb7d1a`. `5eb7d1a` is an ancestor of local HEAD
  (`git log a54afbb..5eb7d1a` is empty), so the remote `m3` is behind the local one.
  "Unchanged" has no before-value to compare with in this review. Consider "`main` is
  at `5644e52`, remote `m3` is at `5eb7d1a`".
- N2. usage-log.md:58 and tasks.md:1420 name who did what (orchestrator added the
  test, user cherry-picked, pushed and dispatched, orchestrator found the escape-sequence
  problem). The run event (`workflow_dispatch`), branch and SHA fit this. Who acted
  cannot be verified from git or `gh`; the commit author is the user's identity.

## 5. What held up
1. `c6da862`: test-only; six files in `crates/rt-demo/tests/` (`acceptance`,
   `acceptance_hand`, `ai_independence`, `coverage_unreferenced`, `origin_independence`,
   `stand_in_hand`), 14 added lines each, 84 in total. Each adds `BUILD_CALLS`, increments it in
   `build_bin_dir`, and adds `the_executables_are_built_once_per_test_process`
   (two `bin_dir()` calls, `assert_eq!(first, second)`, count equals 1).
2. Review file: verdict APPROVED, "No blocking findings" (line 4). Non-blocking N1 is
   the missing comment on the counter placement before the `RT_BIN_DIR` check (lines
   16-21), as the records say. Mutant result "left: 2, right: 1" in all six
   (line 51). The records' "(2 calls counted, 1 expected)" matches.
3. `466de67`: its parent is `5644e52`. The added/removed lines of `git diff 5644e52
   466de67` are identical to those of `git show c6da862` (same six files, 84
   insertions). `git ls-remote origin ci/once-guard` returns `466de67`.
4. Run `44711229145`: attempt 1, conclusion success, event `workflow_dispatch`,
   branch `ci/once-guard`, head `466de67`, created 2026-11-20T09:30:03Z. 23 jobs, 23
   success, all started 2026-11-20. Logs (stripped of ANSI): the new test `... ok`
   6 times in each of `test` ubuntu/macos/windows and 3 times in each of the three
   `demo-offline` jobs; no failed counts, 0 ignored. Sum of passed counts: Linux 1924,
   macOS 1924, Windows 1891.
5. Run `43896108442` attempt 5 (head `5644e52`, success): `test` jobs sum 1918
   Linux, 1918 macOS, 1885 Windows, no occurrence of the new test. Difference is +6 on
   each platform, as claimed. (The record states +6 against macOS and Windows
   figures 1918 and 1885; both match.)
6. `live-openai` job: log prints "live-openai skipped: the repository secret
   RULETRACE_OPENAI_API_KEY is not set." The job conclusion is `success` (skipped by
   script, not a real test). The records say skipped, which is accurate.
7. No overclaim: both new records keep "root cause of the macOS failure in run
   `43892679845`" unconfirmed, "builds by separate test processes" and the guard
   across processes uncovered, "two clean macOS runs only", "no CI run of `95b109f`"
   (the closure text at tasks.md:1351 and 1391 already carries the `95b109f` limit). The diff
   has 0 removed lines (11 insertions in 3 files), so the closure note and earlier text are
   unchanged.
8. Usage-log escape-sequence detail: `gh api .../jobs/136203186097/logs` without the flag
   exits 1 with "the response contains terminal escape sequences; pass
   --allow-escape-sequences". The row's account fits that behaviour.

## 6. Commands run (exit codes)
- `git rev-parse HEAD`, `git status --porcelain --branch`, `git diff --stat`, `git diff`: 0.
- `git show --stat/-p c6da862`, `git diff 5644e52 466de67` and comparison of changed lines: 0.
- `git ls-remote origin ci/once-guard main m3`: 0.
- `gh run view 44711229145 --json ...`, `gh run view 43896108442 --json ...`: 0.
- `gh api .../runs/44711229145/attempts/1/jobs`, `.../runs/43896108442/attempts/5/jobs`: 0.
- `gh api --allow-escape-sequences .../jobs/<id>/logs` for seven jobs of run
  `44711229145` and three of `43896108442`: 0 each.
- `gh api .../jobs/136203186097/logs` without the flag: 1 (expected, see 5.8).
- Two blocked attempts (`git branch --show-current`, `git merge-base`): refused by the
  permission rules, not run; replaced by `git status --branch` and `git log`.
- One `gh run view` run from `/tmp` failed with "not a git repository" (exit 1) and was
  repeated from the repository: 0.

## 7. Mutants run
None. The change is documentation. The mutants of `c6da862` are those of the earlier
review, not re-run here.

## 8. Not verified
- That the user and orchestrator performed the actions attributed to them (N2).
- That the macOS failure of run `43892679845` has no code cause (the records do not claim it).
- That `RT_BIN_DIR` unset/set test counts of the workspace (2462 in the `c6da862` message) hold;
  no cargo run.
- Job counts of other jobs (clippy, golden, and so on) beyond the 23/23 conclusion.

## 9. Integrity
Before: HEAD `a54afbb61aa46ea6445ad7b32a5176cc1df4d4de`; status ` M` on the three
paths named above. After (before writing this report): identical. This report is the only
added file.
