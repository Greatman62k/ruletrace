---
record: LT-7 V-1 and V-4, kill of the real executables during their writes (UD-45 section 3)
task: 13.3 (UD-44/UD-45 scoped exception), 13.9
date: 2026-11-20
status: Statistical evidence on three platforms, no rejected or unrecoverable state. LT-7 stays OPEN (V-2, review acceptance)
---

# V-1 and V-4 kill evidence, 2026-11-20

Design verification items:

- V-1: "Kill during a store write leaves no file that a later run rejects (§11.6)".
- V-4: "Kill of a real `ruletrace review approve` and `ruletrace-ai propose` during the write leaves a recoverable state".

The harness (`crates/rt-cli/tests/kill_harness/mod.rs`,
`crates/rt-cli/tests/kill_review.rs`, `crates/rt-ai-tool/tests/kill_store.rs`,
commit `ce53296`, review `docs/reviews/2026-11-20-review-kill-94f36de.md`,
APPROVED) kills the real executables after delays drawn from a fixed, recorded
seed across their measured run, classifies the files on disk, and requires the
next run to accept or recover the state with the exact codes of the design.
It is test code only. Its evidence is statistical: "no rejected or
unrecoverable state in K kills, of which D during the write". It is not a proof
for every instant, and a process kill cannot show missing fsync or directory
sync; durability rests on the in-process fault tests
(`crates/rt-cli/src/tests_commit_faults.rs`, `tests_commit_recovery.rs`).

## 1. CI run

`ci` run `44772326047`, `workflow_dispatch`, branch `ci/m3-ce53296`, head
`ce53296`, attempt 1. Its conclusion is failure only because
`clippy (macos-15)` (job `136412944748`) and `demo-offline (macos-15)` (job
`136412944868`) were cancelled with 0 steps ("The job was not acquired by
Runner of type hosted even after multiple attempts"), which fails `ci-gate`.
Every `test` job and `core-only (linux)` succeeded, with 0 failed and 0 ignored
(libtest totals: Linux 2491, macOS 2491, Windows 2456; core-only 1562).

## 2. Results (from the job logs; seeds V-1 `0x56455f3153544f52`, V-4 `0x56455f3452455649`; N = 110 trials each)

| Scenario | Job | Killed | Before | During | After | Completed before the kill | Refused by design | Max delay |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| V-1 `ruletrace-ai propose` | test (ubuntu-24.04) | 84 | 75 | 7 | 2 | 26 | 0 | 20.4 ms |
| V-1 | test (macos-15) | 13 | 8 | 4 | 1 | 97 | 0 | 27.5 ms |
| V-1 | test (windows-2022) | 58 | 52 | 3 | 3 | 52 | 0 | 18.6 ms |
| V-4 `ruletrace review approve` | test (ubuntu-24.04) | 90 | 53 | 31 | 6 | 20 | 0 | 8.2 ms |
| V-4 | core-only (linux) | 83 | 54 | 27 | 2 | 27 | 0 | 8.6 ms |
| V-4 | test (macos-15) | 23 | 1 | 21 | 1 | 87 | 0 | 12.0 ms |
| V-4 | test (windows-2022) | 60 | 42 | 12 | 6 | 50 | 2 | 9.3 ms |

No scenario printed `INCONCLUSIVE`: every platform had at least one kill during
the write. The samples during the write are small on macOS for V-1 (4) and on
Windows for V-1 (3); they are evidence, not coverage.

Local Linux runs (implementer and reviewer, slower machine): V-4 81 of 94 and
65 of 75 kills during the write; V-1 48 of 97 and 47 of 87.

## 3. Refused by design on Windows (V-4)

Two Windows trials left only `ledger.jsonl.journal.tmp`, and the next
`ruletrace review approve` refused it with `RT-APR-0208` (exit 70), changing no
file. Design §3.8.3 decision (h): a torn attempt marker "is refused and kept,
with a message that names it", and manual removal is safe under the stated
conditions. The harness then removed the marker, as the design's manual step
says, and the rerun produced the exact final bytes.

- Recovery accepts a marker only if it is the complete journal text of the same
  commit (`claim` in `crates/rt-cli/src/commit.rs`); anything else is refused.
  Locally on Linux, kills that left only the marker were all recovered, never
  refused, so the Windows refusals fit a marker torn while being written.
- The harness cannot confirm that the two markers were torn: it accepts any
  `RT-APR-0208` refusal when only the marker is present (review
  `kill-94f36de`, finding 4). A regression that refused a complete marker would
  also pass this branch.
- Consequence for the Laboratory: a kill of `ruletrace review approve` by the
  Laboratory's limit can, on Windows, leave a state that needs the manual step
  of §3.8.3 (h). That is recoverable by design, not automatic. It is a residual
  for the reviewer's acceptance of LT-7.

## 4. Mutants (review `kill-94f36de`)

Detected: swapping the ledger and Rulebook renames in `commit.rs`; writing the
batch to its final name in two halves in `store.rs`. Not detected: a single
direct write to the final name (its window is microseconds; process kills
cannot show it).

## 5. Limits

1. Statistical, single CI run per platform, hosted runners only.
2. In the local Linux runs (review `kill-94f36de`) no kill landed on an empty or
   partial temporary batch file (V-1 during-states were all a whole temporary
   file), so the prefix check never fired there. The CI logs print no state
   classes, so this is not known for the CI runs.
3. `INCONCLUSIVE` is a printed line, not a test failure; a reader of the log
   must look for it.
4. On Windows a kill is recognised by exit code 1, with a narrow race against a
   process that exits with 1 by itself.
5. Power loss and missing fsync are not covered.

## 6. Still open for LT-7

V-2 (signal handling, UD-45 section 2) is verified only with `ruletrace-lab`
in 13.4/13.6. An independent reviewer has not yet accepted the LT-7 mitigation
in the threat model; the Windows manual-recovery residual of section 3 is part
of that decision.
