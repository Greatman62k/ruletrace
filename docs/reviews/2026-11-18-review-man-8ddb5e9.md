# Review man-8ddb5e9 (bd3c65f..8ddb5e9, branch m3)

## 1. Verdict
INCOMPLETE. No defect found by reading, but the test and the mutants were not executed. Approval needs a run by an agent allowed to run `cargo test`.

## 2. Scope
Commit range bd3c65f..8ddb5e9. One file changed: crates/rt-xtask/tests/app_layers.rs (+24/-1).

## 3. Blocking findings
- B1: `cargo test -p rt-xtask` and both mutants were not run. The reviewer profile denies `cargo test*`. I did not bypass the rule by other means. Requirement 15.5 evidence (the test passing and the mutant failing) is therefore missing.

## 4. Non-blocking findings
- N1 (low): `assert!(md.members.contains_key("rt-engine"))` proves rt-engine is a member. It does not prove rt-engine has Core class in tools/allowed-edges.v1. If the class were not Core, the filter would skip it and the mutant would survive. The mutant run in B1 settles this. A direct assertion on `list.crates["rt-engine"].class` would be stronger.
- N2 (low): `check_graph(...) == vec![]` is exact. It does not assert the offending list names a specific edge, but the mutant check covers that.

## 5. What held up
- The diff (`git diff bd3c65f..8ddb5e9`) only adds a use-line import (`edges`, `graph`) and the new test. All existing tests in the file are unchanged, with no deleted or relaxed assertions.
- The new test uses `cargo_metadata(&root)` (the existing helper) and `parse_metadata`, and loops over every dependency of every member. By reading the code, it covers all dependency kinds only if `parse_metadata` keeps dev and build dependencies. I did not check that.
- Expected value is the literal empty `Vec<String>`, independent of the implementation.

## 6. Commands (worktree ~/review-wt-man, CARGO_TARGET_DIR ~/review-target-man, all cargo inside the unshare loopback wrapper)
- `git diff bd3c65f..8ddb5e9 --stat` and test diff: exit 0
- `git worktree add --detach ~/review-wt-man 8ddb5e9`: exit 0
- `cargo fmt -p rt-xtask -- --check`: exit 0
- `cargo clippy --offline -p rt-xtask --all-targets --all-features -- -D warnings`: exit 0
- `cargo test -p rt-xtask`: NOT RUN (denied by permission rule)
- Worktree and target directory removed.

## 7. Mutants
- rt-engine dev-dependency on prize-check (expect `["rt-engine -> prize-check"]`): not run.
- rt-hash normal dependency on bracket-check: not run.

## 8. Not verified
Test pass status, both mutants, Cargo.lock restoration (nothing was modified, so no diff exists), whether `parse_metadata` retains dev and build dependencies.

## 9. Integrity
Main worktree HEAD before and after: 8ddb5e9b4a513fbec4dd0467d3163ea76367501d. `git status --porcelain` before: empty. After: see below (the report file is the only expected change).
