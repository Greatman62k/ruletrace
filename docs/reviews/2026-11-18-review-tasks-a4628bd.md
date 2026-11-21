# Review tasks-a4628bd

## 1. Verdict

APPROVED WITH FINDINGS. No requirement, deferral or checkbox was weakened. One medium finding (F1) and several low findings need a text fix before the phase is relied on.

## 2. Scope

Commit range `66ef6ec..a4628bd`, branch m3.

- `.kiro/specs/ruletrace-compiler/tasks.md` (82 insertions, 3 deletions)
- `docs/security/laboratory-threat-model.md` (status line and review table)

Compared against `tools/decisions/UD-38.md` (D1 to D5, sections 5 and 8), design §11 and §9.4, `requirements.md` 27.15, 32, 33, 36, 37, and the "Approved deferrals" section of `tasks.md` (lines 83 to 115). No worktree was needed (documentation only, nothing executed).

## 3. Blocking findings

None.

## 4. Non-blocking findings

F1 (medium) `tasks.md` 13.1 Observable (about line 1565): 13.1 closes when the user has decided LT-1 to LT-9 "or recorded each as deferred with its consequence". 13.4 is only blocked by 13.1. A deferred LT-1 therefore lets Laboratory code start with no credential mechanism. UD-38 D2 and §6 item 1 require the mechanism to be settled before implementation, and Req 37.20's canary test needs it. Fix: LT-1 (and LT-2, LT-3) must be decided, not deferrable, for 13.1 to close. Alternatively, state that 13.5 live mode and the 37.20 test stay blocked while LT-1 is deferred.

F2 (low) `tasks.md` 13.4 Observable: "the security tests of 13.6 ... pass". 13.6 is blocked by 13.5, and 13.5 is blocked by 13.4. This is a circular dependency, and 13.4 can never be observed as done. Fix: name the 13.4 security tests as part of 13.4, or move the observable to 13.6.

F3 (low) `tasks.md` line 143 says no Laboratory code before "13.1 and 13.2 are closed". The Phase 13 intro and 13.4 block only on 13.1. 13.2 applies only if LT-1 selects M2, and no later task depends on it. The two statements disagree. Fix: say "13.1, and 13.2 if LT-1 selects M2", and add 13.2 to the blockers of 13.5 live mode in that case.

F4 (low) 9.6 (line 1088): the original `Blocked by: 1.12, 9.1, 9.3, 9.4` and `_Requirements_ ... 32.3` are kept, and the annotation says the task proceeds without 9.3. The only authority named is "the user's sequence of 2026-11-18". That sequence is not in UD-38 or any decision record I found (grep of `tasks.md` and UD-38). The annotation does not say what state 9.6 ends in. Fix: record the sequence as a decision, and say that 9.6 is a reduced scope that cannot be marked `[x]` while Req 32.3 is unsatisfied. Also add 9.6 to the "Approved deferrals" table, or mark it clearly as a new reduction awaiting approval. See item 3 below.

F5 (low) 13.3 `_Requirements_` lists "7.x". Name the criterion.

F6 (low) The threat-model review table says the F1 to F7 fixes "addressed in `66ef6ec`". No review of `66ef6ec` is cited. This is a claim, not evidence.

F7 (low) 9.8 and 9.8's `_Requirements_` still cite 27.15 (amended). See item 4: 27.15 is not met until 13.4. 9.8 must not be marked complete as if 27.15 were satisfied.

## 5. What held up, each with how I verified it

1. **Original text preserved.** `git diff -U0` removes three lines. Line 40 (gate row 1.12) and line 116 (Submission sequencing context) are each re-added with the same text plus a "(superseded by UD-38 ...)" annotation. The third is wave line 60, which only gained a trailing comma. No checkbox changed. No `Deferred (approved ...)` line (1063 to 1544) appears in the diff. 9.3, 9.9, 9.12 to 9.15, 9.17, 9.19, 11.8 and 11.12 are untouched, and the added notes on 9.17 (line 1170), 9.19 (1187) and 11.8 (1417) say "the deferral stays". 9.17 and 9.19 are untouched in their deferral lines. The approved-deferrals table is unchanged.
2. **D4 reclassification.** 9.7 is "Superseded" and stays `[ ]` and in wave 47, with no `main.rs` or `io.rs`. 9.8, 9.10, 9.11, 9.16 and 9.20 are annotated and retained. 9.18 is reworded to run through the Laboratory harness (13.6) and to 13.5, with byte identity with s06 unchanged. 9.3 is not touched and stays deferred and unsatisfied. This matches UD-38 §5.
3. **9.6 without 9.3.** Partly honest. It names the Rank_Reference sentence as unsatisfied and says Req 32.3 stays unsatisfied. It does not mention 32.7. The threshold fixture of 32.7 is stated only in the 9.16 annotation (line 1162, "stays deferred with 9.3"), so 32.7 is disclosed elsewhere, not hidden. 9.6's own Observable (a Tie_Group, an Indeterminate position, a per-participant Outcome) does not require the threshold sentence, so the task could read as complete. See F4.
4. **check-executables.** An exact check at every commit, not a weakening of the check. It compares the set of binary targets of core, aux and lab packages with the list file, so the set must be equal, and 13.4 adds `ruletrace-lab` to the list in the commit that creates `rt-lab`. The app class has no binary and tool and test classes are excluded by rule (RN-25). However, Req 27.15 (`requirements.md` line 1086) names three executables. Until 13.4 the check passes with two. That is an accurate reading of a workspace with no `rt-lab`, but 27.15 is unmet in that interval (F7). The list file is also editable in the same commit as the binary, so the check depends on review of `tools/executables.v1`.
5. **Phase 13.** 13.1 is `[-]` with a status note ("Not complete until the user decides them"), correctly not `[x]`. 13.4 to 13.7 are blocked by 13.1, directly or through 13.5. 13.3 (core changes) is blocked by 13.1. Referenced criteria exist: Req 36 has 24 criteria and Req 37 has 26 (36.22, 36.24, 37.12 to 37.26, 33.18, 33.19). 13.6 names the mutants of 37.23 plus RN-48 additions, the 37.20 canary test, the 256-case property tests, and "each mutant is shown to fail". These do not weaken 37.20 or 37.23. Exceptions are F1 to F3. 13.8 is not in the waves, like the other checkpoints (9.20, 11.14, 12.15).
6. **No claim of a test run; no OPEN item decided.** Added text contains only instructions ("Ensure all tests pass"), and review verdicts refer to review documents. RN-31, RN-44 and RN-38 are phrased "as the user decides" or "whether". 11.8 reads "the user decides whether 11.8 moves". LT-1 to LT-9 stay "OPEN" in the threat model (line 344). The status line change claims only "independently reviewed", which matches the two review files.

## 6. Commands run, each with its exit code

All inside the main worktree, read-only:

- `git rev-parse HEAD && git status --porcelain && git diff --stat 66ef6ec..a4628bd && git diff -U0 ... tasks.md`: 0
- `git diff ... docs/`, `cat tools/decisions/UD-38.md`, `grep` for wave lines: 0
- `grep`, `sed`, `awk` on `requirements.md` and `tasks.md`: 0 (several calls)
- `git -C /home/sam/Desktop/ruletrace status --porcelain`: blocked by permission rule, exit -1 (not run)

No build, test or mutant was run.

## 7. Mutants run

None. This change is documentation only.

## 8. Not verified

- The user's "sequence of 2026-11-18" (9.6 without 9.3) and any LT or RN decision. No record of them was found.
- That `docs/reviews/2026-11-18-review-L1-*.md` verdicts are as quoted. The files were not opened.
- That `66ef6ec` addresses F1 to F7 of the L1 fix review.
- The content of design §11 and §9.4 beyond RN-25, RN-36 to RN-38 and RN-44 table rows. Requirement text was checked only for the numbers cited.
- That the 9.8 annotations match the unread tail of the 9.8, 9.10, 9.11 and 9.20 task texts beyond the lines shown in the diff.

## 9. Integrity

Main worktree HEAD before: `a4628bd4475155d2276a42a2b2d6bfc1cfdb9462`. `git status --porcelain` before: empty.
After: HEAD and status are re-checked below the report in the final message. The only change expected is this report file as an untracked file.
