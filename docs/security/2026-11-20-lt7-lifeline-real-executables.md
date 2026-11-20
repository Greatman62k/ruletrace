---
record: LT-7 lifeline with the real executables (--lifeline-stdin, UD-44)
task: 13.3 (UD-44 scoped exception), 13.9
date: 2026-11-20
status: Real-executable tests pass on three platforms. LT-7 stays OPEN (V-1, V-2, V-4, review acceptance of the threat-model mitigation)
---

# LT-7: `--lifeline-stdin` with the real executables (2026-11-20)

This record follows the prototype evidence
`docs/security/2026-11-20-lt7-lifeline-evidence.md`. It covers the option
`--lifeline-stdin` of `ruletrace` and `ruletrace-ai` (UD-44, design §11.18.6),
implemented in `efa4a6d` and reviewed in
`docs/reviews/2026-11-20-review-life-1070445.md` and
`docs/reviews/2026-11-20-review-life2-1070445.md` (both APPROVED). Both
reviews examined the uncommitted tree on `1070445`; the review-integrity check
`life2-1070445` passed immediately before that tree was committed as
`efa4a6d`. It does not close LT-7, 13.3, 13.9 or 13.1.

## 1. What was run

| Item | Value |
| --- | --- |
| Run | `ci` run `44753380080`, event `workflow_dispatch`, branch `ci/m3-efa4a6d`, head `efa4a6d`, attempt 1, conclusion success, 23 of 23 jobs |
| Jobs checked | `test (ubuntu-24.04)` `136346694177`, `test (macos-15)` `136346694256`, `test (windows-2022)` `136346694013` (job logs read in full) |
| Tests | `crates/rt-cli/tests/lifeline.rs` and `crates/rt-ai-tool/tests/lifeline.rs` (harness-free binaries; shared rig `crates/rt-cli/tests/lifeline_rig/mod.rs`), six checks each |

## 2. Checks and results

Each executable has the same six checks; all 12 passed on each platform
(`test result: ok (0 failed)` twice per log).

| Check | What it shows |
| --- | --- |
| `normal_run_is_unchanged_by_the_option` | with an open, idle pipe the outputs, files and exit code equal a run without the option |
| `a_refused_command_is_unchanged_by_the_option` (`ruletrace`), `dry_and_refused_runs_are_unchanged_by_the_option` (`ruletrace-ai`) | refused and dry runs are unchanged by the option |
| `a_misplaced_or_repeated_option_exits_2_like_any_unknown_option` | the position rule: first argument only |
| `closing_the_pipe_stops_the_child_with_70` | a running child exits 70 and writes `RT-CLI-0007` after the pipe closes, within 5 s |
| `killing_the_parent_leaves_no_child` | after `Child::kill` of the process that holds the write end, the child is absent (terminated and reaped) within 5 s |
| `without_the_option_the_child_outlives_the_parent` | control: without the option the child is still running at the end of the 5 s bound |

Long-running children, without any test hook in production code: `ruletrace
evaluate --render` blocked writing more than 1 MiB (the test asserts at least
1 MiB; about 1.8 MiB in the implementer's local run) to a standard-output pipe
that is never read, and `ruletrace-ai propose` waiting on a loopback server that
accepts and never answers. The parent of the crash checks is the test binary
itself, re-executed with `__parent`.

## 3. Measured times (one run per platform, from the job logs)

| Measurement | Executable | Linux | macOS | Windows |
| --- | --- | ---: | ---: | ---: |
| Pipe closed until exit 70 | `ruletrace` | 5.07 ms | 13.34 ms | 5.32 ms |
| Pipe closed until exit 70 | `ruletrace-ai` | 5.07 ms | 8.66 ms | 5.15 ms |
| Parent killed until child absent | `ruletrace` | 0.73 ms | 8.46 ms | 26.78 ms |
| Parent killed until child absent | `ruletrace-ai` | 0.66 ms | 9.31 ms | 27.93 ms |

The closing times include the 5 ms polling step of the test. The log lines name
the executables `ruletrace.exe` and `ruletrace-ai.exe` on every platform; that
is the rig's Windows image-name label, not the file name on Linux or macOS.

The maximum, 27.93 ms, is far inside the 5 s bound of UD-44. These are single
runs on GitHub-hosted runners, not a guarantee for every system or load.

## 4. Totals

`test` job totals, 0 failed and 0 ignored on each: Linux 2491 and macOS 2491
libtest tests, Windows 2456 (Unix-only tests excluded), plus the 12
harness-free lifeline checks on each platform.

## 5. Outstanding for LT-7

1. V-1: "Kill during a store write leaves no file that a later run rejects
   (§11.6)". Not shown.
2. V-4: "Kill of a real `ruletrace review approve` and `ruletrace-ai propose`
   during the write leaves a recoverable state". Not shown; the crash checks
   above kill the parent while the child waits on output or on the network,
   not during a store or review write.
3. V-2: "Safe signal handling on three platforms (§11.11)". Not addressed.
4. The Laboratory's own use of the option (`ruletrace-lab`, 13.4 onward) does
   not exist and stays blocked.
5. The zombie rule (a child counts as gone only when reaped) is not exercised
   on Linux, where init reaps the orphan within about 1 ms (review
   `life2-1070445`, F1).
6. An independent reviewer has not yet accepted the LT-7 mitigation in the
   threat model, so LT-7 stays OPEN. LT-6 stays OPEN.
