# LT-7 spike: lifeline pipe (design §11.18.6, option L)

Scratch program, outside the RuleTrace workspace (empty `[workspace]` table in
`Cargo.toml`). Standard library only, `#![forbid(unsafe_code)]`.

## What is shown

One binary, `lt7-lifeline`:

- `parent --pids FILE --children N [--gap-ms M] [--no-lifeline] --tag T`
  starts N stand-in children one after another (gap 0 means back to back), each
  with a fresh `Stdio::piped()` standard input whose write end the parent keeps,
  records the child PIDs in FILE and sleeps.
- `child --lifeline-stdin`: one watcher thread blocks reading standard input and
  calls `std::process::exit(70)` on end of file or a read error; the main thread
  sleeps. Bytes on the pipe are ignored.
- `child` (no flag): no watcher; the control.

`tests/crash.rs` is the harness. It starts `parent`, waits for the PIDs, kills the
parent with `Child::kill` (SIGKILL on Unix, TerminateProcess on Windows) and polls
the recorded PIDs for at most 5 s. Tests:

- `a_` one child, crash: gone within 5 s.
- `b_` two children back to back, two with a 300 ms gap, five back to back: gone.
- `c_` control: children without `--lifeline-stdin` are still running 5 s after the
  crash, then they are killed.
- `d_` closing the pipe makes the child exit with code 70 (data on the pipe does not);
  a child without the watcher ignores a closed pipe.
- `e_` (Unix) the liveness probe tells running, zombie and absent apart.

Measured times print as `LT7-MEASURE ...` lines (use `-- --nocapture`). Two values:
kill to no child running, and kill to every PID absent (reaped). The time includes
`Child::wait` of the killed parent and the cost of each probe, which spawns
processes, so it is an upper bound with a few milliseconds of granularity.

Liveness without `unsafe`: Unix `kill -0 <pid>` through `sh`, then `ps -o stat=,args=`.
A zombie is reported separately and the crash tests require the PID to disappear
(terminated and reaped). The command line must carry the harness tag, so a process
that reuses a PID counts as absent. Windows uses `tasklist /FI "PID eq <pid>"` and
the image name `lt7-lifeline.exe`. The Windows and macOS code paths are written but
run only by the CI workflow `.github/workflows/spike-lt7.yml`.

Run (loopback only):

    unshare -rn sh -c 'ip link set lo up && exec "$@"' sh \
      cargo test --manifest-path spikes/lt7-lifeline/Cargo.toml -- --nocapture

## What is not shown

- Real `ruletrace` or `ruletrace-ai` children and their `--lifeline-stdin` option
  (task 13.3).
- Journaled writes killed in the middle of a commit (`review approve`, store write):
  recovery after a real process kill, V-1 and V-4, belongs to 13.3.
- A read error on standard input (only end of file is exercised).
- Handle leakage between children on Windows beyond the back-to-back tests, and a
  deliberately injected leak (not possible with safe `std`).
- A reaper-less container (a PID 1 that does not reap): the strict absence check
  would then report a zombie.
- Power loss, or a kill of the whole process tree (the harness kills the parent only).
