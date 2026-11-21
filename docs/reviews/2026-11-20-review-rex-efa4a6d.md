# Review: uncommitted LT-7 real-executable evidence on efa4a6d (baseline rex-efa4a6d)

## 1. Verdict

APPROVED. No blocking finding. Three non-blocking findings (section 4).

## 2. Scope

Branch m3, HEAD efa4a6d, four changed paths (documentation only):

- New: `docs/security/2026-11-20-lt7-lifeline-real-executables.md`
- Modified: `docs/security/laboratory-threat-model.md` (rows R-L3 line 336, LT-7 line 352, review row line 366)
- Modified: `.kiro/specs/ruletrace-compiler/tasks.md` (notes at lines 1661 and 1705)
- Modified: `.kiro/specs/ruletrace-compiler/traceability.md` (section from line 1331)

Compared with: CI run 44753380080 and its three test job logs, `design.md` 3559-3562, `crates/rt-cli/tests/lifeline.rs`, `crates/rt-ai-tool/tests/lifeline.rs`, `crates/rt-cli/tests/lifeline_rig/mod.rs`, `tools/decisions/UD-44.md`, the two life reviews.

## 3. Blocking findings

None.

## 4. Non-blocking findings

1. Record section 2, "about 1.8 MiB" for `evaluate --render`. No code or log states that figure. `crates/rt-cli/tests/lifeline.rs:21-22` has 3000 participants and `MIN_RENDER_BYTES = 1 << 20`, so only "at least 1 MiB" is established. Reword or drop the number.
2. Record, intro: "implemented in `efa4a6d` and reviewed in ...-1070445.md". Both reviews examined the uncommitted tree on base 1070445, and I did not diff that tree against the efa4a6d commit. The wording is accurate, but "reviewed" could be read as a review of the commit. The same applies to the tasks.md note.
3. Threat-model line 366 and the tasks.md 13.9 note cite this report. The report did not exist when they were written, so they now point at a file that exists only after this report is saved. This is intended, and the row says the result is in the report.

## 5. What held up

- Run metadata. `gh run view` returned workflow `ci`, event `workflow_dispatch`, headSha efa4a6d84fc1709c303c19b631d512b1bc524b68, branch `ci/m3-efa4a6d`, attempt 1, conclusion success, 23 jobs, 23 with conclusion success.
- Logs (ANSI stripped). Each platform has both `Running tests/lifeline.rs` binaries and two `test result: ok (0 failed)` lines for them. Each lists the 6 check names of record section 2, with the correct `ruletrace`-only and `ruletrace-ai`-only names for the refused and dry check.
- Times in the logs round to the section 3 table.

| Executable | Event | Linux | macOS | Windows |
| --- | --- | --- | --- | --- |
| `ruletrace` | exit after close | 5.070913 ms | 13.339542 ms | 5.3172 ms |
| `ruletrace-ai` | exit after close | 5.069633 ms | 8.663125 ms | 5.15 ms |
| `ruletrace` | gone after kill | 727.438 µs | 8.462667 ms | 26.7766 ms |
| `ruletrace-ai` | gone after kill | 658.254 µs | 9.312333 ms | 27.9348 ms |

  The maximum is 27.93 ms.
- Totals. The sum of `test result: ok.` lines is 2491 passed on ubuntu, 2491 on macos and 2456 on windows. Each has 0 failed and 0 ignored.
- Code statements.
  - `rt-cli/tests/lifeline.rs` sets the long-running child as `evaluate --render` to an unread stdout pipe.
  - `rt-ai-tool/tests/lifeline.rs` uses a loopback server that accepts and never answers (lines 35-60).
  - The rig re-executes the test binary with `__parent` (`lifeline_rig/mod.rs:31`).
  - `POLL` is 5 ms and the wait loop at line 228 sleeps it, so close times include it.
  - `IMAGE` is `"ruletrace.exe"` at `rt-cli/tests/lifeline.rs:18` and `"ruletrace-ai.exe"` at `rt-ai-tool/tests/lifeline.rs:25`, a label that is the same on all platforms.
- V-1, V-2 and V-4 are quoted exactly from design.md 3560-3562. The zombie limit matches review life2 F1 (rig `mod.rs:388-401`). The Linux "about 1 ms" matches the 0.66 and 0.73 ms measurements.
- Traceability. `git diff --quiet 1070445 efa4a6d -- crates/rt-cli/src/tests_pipeline.rs` returned 0, so the file is unchanged. Its exit-code sets include 70 (lines 504-536, `[0, 2, 3, 4, 70]`).
- Threat model. `git diff -U0` shows R-L3 and LT-7 only appended to (the earlier text is intact, in-line). One review row was added. LT-7 stays OPEN. LT-6 is not changed here. tasks.md has 13.1 `[-]` at line 1636, 13.3 `[ ]` at 1652 and 13.9 `[ ]` at 1697. The notes state that they stay open. I found no overstatement: V-1, V-2 and V-4 and the acceptance of the mitigation are listed as outstanding.
- RN-50 "confirmed" matches design.md 3190 and UD-44.
- Scope. `git status --porcelain` lists exactly the four paths and nothing else. The committed tree is untouched.

## 6. Commands run

All exit 0 unless stated.

- `git rev-parse HEAD`, `git status --porcelain`, `git rev-parse --abbrev-ref HEAD`. The first attempt, using `git branch --show-current`, was blocked by a permission rule and was not run (exit -1).
- `gh run view 44753380080 --json ... --jq` (exit 0).
- `gh api --allow-escape-sequences repos/Greatman62k/ruletrace/actions/jobs/{136346694177,136346694256,136346694013}/logs` (each exit 0, saved under /tmp/rex).
- `sed` to strip ANSI codes, then `grep` and `awk` for lifeline lines and totals.
- `git diff -U0` of the three modified files, `git diff --quiet 1070445 efa4a6d -- crates/rt-cli/src/tests_pipeline.rs` (exit 0), `git diff --stat 1070445 efa4a6d`.
- `sed` and `grep` of design.md, the code files and the reviews.

## 7. Mutants run

None. This is a documentation review, and the task did not ask for cargo or mutants.

## 8. Not verified

- I did not re-run any test or build. The test behaviour is taken from the CI job logs only.
- I did not check the assertions inside the six checks beyond their names and the rig statements above. Only the logs and the code statements named in the task were compared.
- I did not check whether the efa4a6d commit equals the tree the two life reviews examined (non-blocking finding 2).
- I did not check the statement of exactly 12 harness-free checks beyond the 6 names per binary printed in each log.
- The "about 1.8 MiB" figure is unverified (non-blocking finding 1).
- The other 20 jobs were checked only for conclusion success, not by log.

## 9. Integrity

- Before: HEAD efa4a6d84fc1709c303c19b631d512b1bc524b68. Status: ` M .kiro/specs/ruletrace-compiler/tasks.md`, ` M .kiro/specs/ruletrace-compiler/traceability.md`, ` M docs/security/laboratory-threat-model.md`, `?? docs/security/2026-11-20-lt7-lifeline-real-executables.md`.
- After: HEAD unchanged. The status is the same four paths, plus this report as a new untracked file (the one permitted write). I made no other change in the main worktree. Scratch logs are in /tmp/rex, outside the repository.
