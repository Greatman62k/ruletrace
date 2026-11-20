# Review: once-only build guard test (c6da862)

## 1. Verdict
APPROVED. No blocking findings.

## 2. Scope
Range 2efb3af..c6da862. Six files under `crates/rt-demo/tests/`: acceptance.rs,
acceptance_hand.rs, ai_independence.rs, coverage_unreferenced.rs,
origin_independence.rs, stand_in_hand.rs. 84 insertions, 0 deletions
(`git diff | grep -c '^-[^-]'` = 0). No production file touched.

## 3. Blocking findings
None.

## 4. Non-blocking findings
- N1. `BUILD_CALLS` is incremented before the `RT_BIN_DIR` early return
  (e.g. stand_in_hand.rs:31-32). The test therefore also counts calls when
  `RT_BIN_DIR` is set, which is what makes it work in both modes. Moving the
  increment after the check makes the test fail with 0 != 1 when `RT_BIN_DIR`
  is set (mutant M2), so the placement is load-bearing. No comment says so.
- N2. The test proves "one call per test process" only for the calls it makes
  itself. Other tests in the binary reach the guard through the same
  `bin_dir()`, so a regression of the guard shows as count > 1 whenever the
  assertion runs after them, and as 2 even when it runs first (the test calls
  `bin_dir()` twice). It does not depend on scheduling.

## 5. What held up
1. Determinism. `OnceLock::get_or_init` runs the initialiser once, even for
   concurrent callers. The counter is only read in the new test and only
   written in `build_bin_dir`, which only `bin_dir()` calls (grep of
   `BUILD_CALLS`, `build_bin_dir`: no reset, no other reader). The test's own
   two calls happen after any other test's call has either run the initialiser
   or blocked on it, so the value read is exactly 1 in any order.
   Both full runs passed with the test present in all six binaries.
2. Additions only; existing assertions unchanged.
3. Gates (worktree detached at c6da862, loopback-only wrapper):
   - `cargo build --workspace --bins --offline`: exit 0.
   - `cargo test -p rt-demo --all-features --offline`, `RT_BIN_DIR` unset:
     exit 0; 12 result lines, 107 passed, 0 failed, 0 ignored; new test ok in
     6 of 6 binaries.
   - Same with `RT_BIN_DIR=<target>/debug`: exit 0; 107 passed, 0 failed,
     0 ignored; new test ok in 6 of 6.
   - `cargo clippy -p rt-demo --all-targets --all-features --offline -- -D warnings`: exit 0.
   - `cargo fmt -p rt-demo -- --check`: exit 0.
   (An earlier foreground run hit the 120 s tool timeout and was discarded; a
   rerun detached from the shell produced the results above.)

## 6. Mutants
M1 = replace the shared `static BUILT` with a per-call `OnceLock` (a `let`
inside `bin_dir`), run `--test <file> -- the_executables_are_built_once_per_test_process`
with `RT_BIN_DIR` set. All six killed by the new test (`left: 2, right: 1`, exit 101):
acceptance, acceptance_hand, ai_independence, coverage_unreferenced,
origin_independence, stand_in_hand.

On stand_in_hand.rs:
- M2 (counter moved after the `RT_BIN_DIR` check), guard intact, `RT_BIN_DIR`
  set: test fails, `left: 0, right: 1`. This is a false failure caused by the
  mutation of the counter, not a missed defect.
- M2 + M1, `RT_BIN_DIR` set: fails with `left: 0`. It fails, but the reason is
  the count of 0, so it would not discriminate a per-call guard from a correct one.
  With the counter before the check (as committed), M1 is detected with
  `RT_BIN_DIR` set.
- M2 + M1, `RT_BIN_DIR` unset: fails, `left: 2`.
All files restored with `git checkout`; `git diff` in the worktree is empty.

## 7. Does it rule out concurrent nested builds?
Within one test process: yes, for the six rt-demo integration binaries, given
that every nested build goes through `bin_dir()` (verified by grep) and the
test asserts the count is 1 after two lookups. The guard plus the test make a
second nested `cargo build` in the same process detectable.

It does not rule out:
- Several test processes building at once. Each of the six binaries has its
  own `OnceLock`, and cargo runs the integration binaries one after another
  by default, but `cargo test` of different packages, `--jobs`, or another
  runner can overlap them. Each unset-mode binary still starts its own
  nested `cargo build` (cargo's build-directory lock serialises them but does
  not stop a relink while another process runs the executable).
- `cargo nextest`, which runs each test in its own process, so the
  once-per-process guard gives no once-per-run property there.
- The macOS NotFound root cause (run 43892679845). I did not reproduce it; the
  tests show the guard behaves as designed on Linux, not that the macOS failure
  is fixed.

## 8. Not verified
macOS behaviour; CI; nextest; repeated runs for flakiness (two full runs, no
failures); behaviour with more than one process.

## 9. Integrity
Main worktree before: HEAD c6da8620156f7e4b43df62240325eca70ee4887d, status
empty. After: see final line below.

After: HEAD c6da8620156f7e4b43df62240325eca70ee4887d unchanged. Status shows only this report (`?? docs/reviews/2026-11-20-review-once-c6da862.md`). Worktree and target directory removed.
