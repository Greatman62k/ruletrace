# Review: V-1/V-4 kill harness (uncommitted, base 94f36de)

## 1. Verdict

APPROVED. No blocking finding. Six non-blocking findings (section 4); items 4.1 and 4.2 change what the evidence record may claim and should be fixed or stated in the record.

## 2. Scope

Uncommitted change on local branch m3 over HEAD 94f36de: `crates/rt-cli/tests/kill_harness/mod.rs` (405 lines), `crates/rt-cli/tests/kill_review.rs` (504), `crates/rt-ai-tool/tests/kill_store.rs` (466), one `[[test]]` entry each in `crates/rt-cli/Cargo.toml` and `crates/rt-ai-tool/Cargo.toml`. Read against UD-45 section 3, design V-1/V-4 (design.md:3559-3560), §3.8.3 (h) (design.md:1250), `crates/rt-ai-tool/src/store.rs:355-445`, `crates/rt-cli/src/commit.rs`, `crates/rt-cli/src/io.rs:159-191`, `catalogue/diagnostics.v1.json`. Work was done in a scratch worktree with the changed files copied in; both the worktree and the target directory are removed.

## 3. Blocking findings

None.

## 4. Non-blocking findings

4.1 **Kills are fewer than N.** `kill_harness/mod.rs:260-330` counts completed trials in N. Measured: V-4 N=110 gives killed=75 (35 completed); V-1 N=110 gives killed=87 (23 completed). UD-45 asks for at least 100 trials and states the result as "N kills, of which M during". The assertion `trials >= 100` (mod.rs:249) holds, but nothing requires 100 kills. Cause: the range `hi` is 1.1 x the median of runs measured while a spin poller was running (mod.rs:208-226), and unpolled runs are faster, so the late delays outlive the process. The record must quote killed and during counts (75/65 and 87/47), not N. Better: draw until 100 kills, or measure without the poller.

4.2 **V-1 evidence covers only a whole temporary file.** All 47 during-write kills are `tmp-whole` or `tmp-whole+final-linked`. No `tmp-empty` or `tmp-partial` state occurred (the 10 504-byte batch is one `write_all`), so the prefix check in `kill_store.rs` observe is not exercised by kills on Linux. There were no `after` kills in V-1 (after=0), and 40 of 87 kills landed before any store directory existed. The window covered is the span from the completed temp-file sync to the temp-file removal, including the hard link and directory sync.

4.3 **The torn-marker path is not reached by kills.** `refused_by_design=0` in the V-4 run. `journal.tmp` alone was seen 19 times, all evidently whole (they recovered with exit 0). The marker is one small write, so a torn marker is unlikely to occur by a kill. The branch at `kill_review.rs:387-410` is therefore unexercised. Per-step evidence exists in `tests_commit_faults.rs:113` (`a_torn_attempt_marker_is_refused_and_kept`).

4.4 **The torn-marker branch also accepts a whole marker that is refused.** The condition is `left == ["journal.tmp"]` and exit 70 (kill_review.rs:~387). It does not check that the marker is incomplete. A regression that refuses a whole marker (design: a whole marker of this attempt is recognised and cleaned up, `commit.rs:claim`) would be counted as `RefusedByDesign` and pass. Fix: compare the marker bytes with the expected journal text, or require its length to be shorter than that of a marker from an uncut run.

4.5 **On `refused_by_design`.** Accepting the refusal is correct against §3.8.3 (h): the state is refused, kept, and named in the message; the harness checks RT-APR-0208, exit 70, the file name in the message and that no file changed (`dir.state()` before and after); then performs the documented manual removal and requires the rerun to succeed with the exact final bytes. It is a recoverable state with a manual step, not an automatic recovery. It is reported visibly (`refused_by_design=N` in the summary line, never merged into the accepted count). The V-4 record must say that this window is recoverable only by the manual step of (h).

4.6 **INCONCLUSIVE is a printed line only.** `mod.rs:~358`. The test still exits 0 and prints `ok`. This follows UD-45 ("reported as inconclusive") and the failure path is not hidden (any bad state panics before the summary), but a CI reader that looks at the exit status sees a pass. Shown by the direct-write mutant below. Consider a CI step that greps for `INCONCLUSIVE`, or a distinct exit code.

4.7 **Windows kill detection (`mod.rs:~120`).** `is_kill_status` treats exit code 1 as a kill on Windows. A process that exits with 1 by itself in the gap between `try_wait` and `kill` would be counted as killed. On Unix any signal death (for example SIGSEGV) is counted as a kill. Both windows are narrow; the first `try_wait` already turns a non-zero self-exit into an error, and exit 70/2/3 cannot be mistaken. Not verified on Windows (section 8).

## 5. What held up

1. Test-only. `git diff --stat`: two files, 5 insertions each, both `[[test]]` entries with `harness = false`. The other three files are untracked test files. `grep unsafe` finds nothing in them. No `[dependencies]` or `[dev-dependencies]` change. `git status --porcelain` of the main worktree is unchanged between start and end, so nothing under demo/ or tests/fixtures changed; the V-4 scenario reads `demo/stand-in-ai/...` and writes only scratch copies.
2. Protocol match. V-4 file list and order match `commit.rs`: `prepare` (marker `.journal.tmp` first, then both `.new`, then `.old` links, directory sync), `publish` (marker renamed to `.journal`), `replace` (ledger then Rulebook, directory sync after each), `cleanup` (temp and backup files, marker, journal last). Observed during-states in the run are all consistent with it (for example `journal+rulebook.new+rulebook.old` with ledger=End, rulebook=Start). V-1 matches `store.rs:395-440` (`store/.tmp/<id>.tmp`, `create_new`, `sync_all`, hard link to `store/batches/<id>.json`, directory sync, temp removal) and the slot names `<id>.<n>.tmp`. Windows: `io.rs:180-191` makes the directory sync a no-op, which changes no file, so the classification is the same; `.old` creation falls back to a copy, still in the classified name set.
3. Acceptance checks. Exit codes and severities were checked against `catalogue/diagnostics.v1.json`: RT-APR-0213 exit 2, RT-APR-0214 exit 2, RT-APR-0204 exit 3, RT-APR-0208 exit 70, RT-AI-0301 Warning exit 0. The harness asserts exit 2 with RT-APR-0214 while a journal exists, exit 2 with RT-APR-0213 for a missing store, exit 3 with RT-APR-0204 after a committed state, RT-AI-0301 reported if and only if the batch was already stored. A new Rulebook with the old ledger is an error (V-4 observe); a new ledger with the old Rulebook is accepted only with the journal present. Stale temp files: V-1 requires the temp set unchanged after the repeated run, as design says; V-4 requires no leftovers after recovery and the final pair equal to the bytes of an uncut run. Expected ledger bytes are tied to the reviewed golden `demo/stand-in-ai/golden/artifacts/ledger.jsonl` (leading 2 and 3 lines), and the V-1 id to the pin `1c574c2d...54b2`, so expected values are not derived from the code under test alone.
4. Statistics. `assert!(trials >= 100)`; TRIALS=110. The seeds and delays are printed in the summary and in every failure message. SplitMix64 is checked against vectors for seed 0 (`e220a8397b1dcdaf`, `6e789e6aa1b965f4`, `06c45d188009454f`) that I recognise as the published ones; the code does not compute them. Range from measured uncut runs (2 warmups, 10 measured; lo = half of earliest onset, hi = 1.1 x median) is a defensible choice, with finding 4.1. Completed trials are asserted to be in the final state and are excluded from before/during/after counts. INCONCLUSIVE path was exercised by the direct-write mutant and prints as specified (finding 4.6).
5. Gates in the scratch worktree: see section 6.

## 6. Commands run (all cargo commands inside `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh`, `--offline`, `CARGO_TARGET_DIR=~/review-target-kill`)

| Command | Exit |
|---|---|
| `cargo test -p rt-cli --test kill_review` | 0 (2:00 including 33 s build) |
| `cargo test -p rt-ai-tool --test kill_store` | 0 (58 s including 35 s build) |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 |
| `cargo clippy -p rt-cli --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | 0 |
| `cargo clippy -p rt-ai-tool --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | 101: `x86_64-w64-mingw32-gcc` missing (C build of a dependency); not a lint result |

Summary lines (Linux x86_64):

- V-4: `N=110 killed=75 before=3 during=65 after=7 completed=35 refused_by_design=0 max_delay=709.96ms (range 3.62ms..720.99ms)`; run min/median/max 612/655/676 ms.
- V-1: `N=110 killed=87 before=40 during=47 after=0 completed=23 refused_by_design=0 max_delay=142.8ms (range 30.5ms..144.7ms)`; run min/median/max 102/132/255 ms.

CI suitability: about 85 s (V-4) and 25 s (V-1) of test time in a debug build, plus build; acceptable. Both rely on timing, so the during counts vary between runs; no minimum is enforced (finding 4.6). V-1 builds `ruletrace` through `cargo build --offline` if it is missing (kill_store.rs, `ruletrace_exe`), which needs the offline cache in CI.

## 7. Mutants run

| Mutant | Result |
|---|---|
| `commit.rs` `replace`: rename the Rulebook before the ledger | Killed: V-4 trial 12, delay 361 ms, "a new Rulebook with the old ledger (Req 26.30)" (exit 1) |
| `store.rs`: write the batch to the final name in two halves with a 20 ms pause before the normal protocol | Killed: V-1 trial 1, "a partial or different file at the final name ... 5252 bytes of 10504" (exit 1) |
| `store.rs`: hard link to the final name before the write | Killed, but by a side effect: the repeated run reported RT-AI-0301 for a store that was empty (V-1 trial 10), not by a partial file |
| `store.rs`: one direct `File::create` plus `write_all` to the final name, no temp file, no sync | **Survived**: 110 trials, `during=0`, `INCONCLUSIVE V-1` printed, exit 0 |

The surviving mutant is expected. A single `write_all` of 10 KB is not torn by SIGKILL, and a missing `sync_all` or directory sync is visible only after power loss or a system crash, not after a process kill. The record for V-1 and V-4 must say so: the harness shows that no killed process leaves a partial file at a final name and that recovery works; it cannot show durability (fsync) defects, which the in-process fault-injection tests cover per step. Production files were restored after each mutant; `cmp` of `commit.rs` and `store.rs` with the main worktree copies reported no difference, and `git status` of the scratch tree showed only the five expected paths.

## 8. Not verified

- Windows and macOS runs, including exit code 1 for `TerminateProcess` and the Windows during-write states. Only the Windows-target clippy of rt-cli (compiles and lints, does not run).
- Windows-target clippy of rt-ai-tool (no mingw C compiler here).
- Whether a torn marker, an empty temp file or a partial temp file ever appears on any platform; none did on Linux in 110 trials each.
- A second run of each scenario to see how stable the during counts are.
- Mutation coverage beyond the four mutants above.
- Directory-sync and fsync behaviour, by design (finding in section 7).

## 9. Integrity

Main worktree at start: HEAD 94f36dedf3c26f5478e9a5b556379afe8850567f; status: ` M crates/rt-ai-tool/Cargo.toml`, ` M crates/rt-cli/Cargo.toml`, `?? crates/rt-ai-tool/tests/kill_store.rs`, `?? crates/rt-cli/tests/kill_harness/`, `?? crates/rt-cli/tests/kill_review.rs`. At the end: see the final message; the only addition is this report.
