---
record: LT-7 cross-platform prototype evidence (lifeline pipe, design §11.18.6)
task: 13.9 (UD-39)
date: 2026-11-20
status: Prototype evidence delivered. LT-7 stays OPEN (formal decision, RN-50, real executables, V-1, V-2, V-4 outstanding)
---

# LT-7 lifeline-pipe evidence (2026-11-20)

This record holds the evidence that task 13.9 asks for LT-7: the lifeline pipe
of design §11.18.6, prototyped with stand-in children, killed on Linux, macOS
and Windows. It is evidence for a candidate mechanism. It does not close LT-7,
task 13.9 or task 13.1, and it does not show the real `ruletrace` or
`ruletrace-ai` executables.

## 1. What was run

| Item | Value |
| --- | --- |
| Prototype | `spikes/lt7-lifeline` (standalone package outside the workspace, standard library only, `#![forbid(unsafe_code)]`), committed in `263a662` |
| Workflow | `.github/workflows/spike-lt7.yml` (`spike-lt7`), separate from the required `ci` workflow |
| Run | `44733286392`, attempt 1, event `pull_request` (draft PR #1, not merged), conclusion success, 2026-11-20 12:08 to 12:14 UTC |
| Commit tested | merge ref `refs/pull/1/merge` = `21cabb3`, parents `5644e52` and `263a662`; its tree `a4c2798` equals the tree of `263a662` |
| Platforms | `ubuntu-24.04` (Linux 7.0.0-1017-azure, x86_64), `macos-15` (Darwin 24.6.0, arm64), `windows-2022` (Windows Server build 20348, x86_64); rustc 1.99.1 on all three |
| Sources checked | the job logs of jobs `136278205123` (Linux), `136278204698` (macOS), `136278205138` (Windows) and the uploaded artifacts `spike-lt7-<os>` (`test-lt7.log`, `test-lt6.log`, `system.log`) |

## 2. What the harness measures

Each crash test starts a `parent` process, which starts stand-in children one
at a time, each with a fresh pipe as standard input and the option
`--lifeline-stdin`. The parent keeps the write end and sleeps. The test waits
until every child PID is recorded and running, then calls `Child::kill` on the
parent (SIGKILL on Unix, TerminateProcess on Windows) and `wait()` on it. It
then polls the child PIDs every 5 ms until none exists, bounded at 5 s.

The kill-to-gone values below run from just before `Child::kill` of the parent
to the first poll at which every recorded child PID is absent. They include the
parent's `wait()` and the polling granularity (5 ms sleep plus the time to
start the liveness probe: `kill -0` and `ps` on Unix, `tasklist` on Windows). In
every run the time to "no child running" equalled the time to "absent", so no
zombie was observed.

## 3. Measurements (milliseconds, one run per platform)

Kill of the parent until every child process is gone:

| Case | Linux | macOS | Windows |
| ---: | ---: | ---: | ---: |
| One child | 1 | 15 | 26 |
| Two back to back | 1 | 9 | 44 |
| Two with a gap | 1 | 10 | 41 |
| Five back to back | 10 | 33 | 109 |
| Control: children without the watcher | still running at the end of the 5 s bound | still running at the end of the 5 s bound | still running at the end of the 5 s bound |

Orderly close, a separate measurement with a different start event: from the
test closing the write end of a child's pipe (the parent alive) until that
child exits with code 70.

| Case | Linux | macOS | Windows |
| ---: | ---: | ---: | ---: |
| Orderly pipe close until exit 70 | 10 | 44 | 10 |

The orderly-close test waits for the child's exit by polling every 10 ms, so its
values have a granularity of about 10 ms; the Linux and Windows values of 10 ms
are at that quantum (the exit came within the first poll interval).

Each platform log holds exactly one `orderly_close` line. An earlier summary in
the session listed this row twice; that was a duplicated transcription (two
searches printed the same log line), not two test cases.

Results:

- With the watcher, every tested child exited within the 5 s bound on all
  three platforms.
- Without the watcher (the control), the children were still running at the
  end of the bound on all three platforms, so the crash test can fail on each.
- On Windows the two-child and five-child back-to-back cases passed. The
  handle-inheritance risk of §11.18.6 (a child holding another child's write
  end) did not appear in these cases.

## 4. Test counts

| Package | Linux | macOS | Windows |
| --- | ---: | ---: | ---: |
| `lt7-lifeline` crash tests | 8 passed | 8 passed | 7 passed |
| `lt6-basic-auth` (unit and server tests) | 23 passed | 23 passed | 22 passed |

All with 0 failed and 0 ignored. On Windows the zombie-probe test
`e_liveness_probe_distinguishes_running_zombie_and_absent` is Unix only, and one
`lt6` unit test (reading `/dev/urandom`) is Unix only. The `lt6` server tests ran
in the same workflow; they are not the LT-6 browser protocol, which is still to
be run (section 7).

## 5. The 5 s bound and its margin

Design §11.18.6 states that the 5 s harness timeout "is not a measured value:
13.9 measures the actual time per platform and the bound is set from that
measurement with a stated margin."

- Observed maximum: 109 ms (five children back to back, Windows), one run per
  platform on GitHub-hosted runners.
- The 5 s bound is about 45.9 times that maximum (5000 / 109 = 45.87), with
  4,891 ms of headroom.
- Proposal, not yet a decision: keep 5 s as the acceptance limit. Reasons:
  the margin is large against the observed times; hosted and loaded machines can
  be much slower than one sample shows; the real children (13.3) start a larger
  executable and may be stopped during a store or review write, so their times are not known;
  and a test bound only rejects a child that outlives it, so a generous bound
  does not weaken detection of a child that never exits (the control and the
  watcher-removal mutant fail it).
- These are experimental results from one run on each of three hosted runner
  images. They are not a worst-case guarantee for every operating system, load
  or deployment. The bound is to be measured again with the real executables in
  13.3.

The design asks for the bound to be "set from that measurement with a stated
margin". This record states the margin. Setting the bound is part of the formal
LT-7 decision, so the design text needs no change now. If the user keeps 5 s,
the smallest record change is one dated line in §11.18.6 citing this record,
the 109 ms maximum and the 45.9 times margin.

## 6. Outstanding for LT-7

1. Real executables: the children are stand-ins. `--lifeline-stdin` in
   `ruletrace` and `ruletrace-ai` and the Laboratory's use of it belong to 13.3.
2. V-1 (design verification list): "Kill during a store write leaves no file
   that a later run rejects (§11.6)". Not shown; to verify in 13.3.
3. V-4: "Kill of a real `ruletrace review approve` and `ruletrace-ai propose`
   during the write leaves a recoverable state". Not shown; to verify in 13.3.
4. V-2: "Safe signal handling on three platforms (§11.11)", which the design
   lists as "To decide with LT-7". Not addressed by this prototype.
5. Environment: GitHub-hosted runners only, one run each; not every operating
   system version, load or deployment environment.
6. Formal LT-7 decision and RN-50 (the `--lifeline-stdin` option) are pending
   with the user. LT-7 stays OPEN in the threat model and in UD-39.

LT-6 is separate and still open: the browser protocol
(`spikes/lt6-basic-auth/BROWSER-PROTOCOL.md`) has not been run, and the user
decisions on the check order and on the amendment of Req 37.5 and 37.6 are
pending.
