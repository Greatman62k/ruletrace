# Follow-up review app2: spikes/lt6-basic-auth (uncommitted), base 0dfa5d5

## 1. Verdict
APPROVED. M02 is now killed. No new blocking finding.

## 2. Scope
Uncommitted changes under `spikes/lt6-basic-auth/` on branch m3 (HEAD
0dfa5d5), copied to `~/review-app2/`: tests/server.rs, README.md,
BROWSER-PROTOCOL.md, and the unchanged-claimed src/*.rs.

## 3. Blocking findings
None.

## 4. Non-blocking findings
- Production code unchanged: only partly confirmed. I kept no copy of my
  earlier review tree, so no diff was possible. mtimes: main.rs 09:53,
  http.rs and recorder.rs 10:16, all before my earlier report (10:28:29).
  src/lab.rs has mtime 10:29:52, after that report, which fits the
  orchestrator's M02 mutate/restore. I read lab.rs:209 (`if n == name {`,
  correct). I cannot prove the rest of lab.rs equals the earlier version.
- Earlier non-blocking findings (race-test window limit, inert target-syntax
  cases, Sec-Fetch-Site lists, recorder echo test) stay non-blocking. None
  must block a spike prototype. Record them in the evidence record.
- README.md:29-34 states that the full Requirement 37 criterion 4 check is
  owed by ruletrace-lab (13.4) and that the prototype serves a GET with a
  wrong Origin, valid credentials, path and cookie. Accurate, matches scope.
- BROWSER-PROTOCOL.md:83-89 (step 4 redaction), :94-102 (step 5 history
  advice, Laboratory restart, `<token>` in the request line): present as
  described. Line 101-102 is a long line; cosmetic only.

## 5. What held up
- New "name suffix" case at tests/server.rs:515 asserts the exact status
  (assertion at :540) for `Cookie: <name>x=<identifier>`.
- All gates pass (section 6).
- `git status --short` in the main worktree shows only the seven modified
  files under spikes/lt6-basic-auth/ plus my earlier untracked report.

## 6. Commands (each inside `unshare -rn ... ip link set lo up`; cargo with
`--manifest-path ~/review-app2/lt6-basic-auth/Cargo.toml`,
CARGO_TARGET_DIR=~/review-target-app2)
- `cargo fmt --check`: 0 (an attempt with `--locked --offline` is rejected
  by cargo fmt, exit 2, a usage error and not a result; rerun without them).
- `cargo clippy --locked --offline --all-targets -- -D warnings`: 0
- same with `--target x86_64-pc-windows-gnu`: 0
- `cargo test --locked --offline`: 0 (8 unit, 39 server, 0 ignored)
- `cargo test --locked --offline -- --test-threads=1`: 0 (8, 39, 0 ignored)

## 7. Mutants
- M02 `if n == name {` to `if n.starts_with(name.as_str()) {` (lab.rs:209):
  KILLED by `right_path_and_credentials_without_a_matching_cookie_are_403`,
  panic at tests/server.rs:540 "name suffix" (38 passed, 1 failed, exit 101).
  Restored; `cmp` against the pre-mutation copy and the main worktree file
  both identical.

## 8. Not verified
- No browser protocol run. No diff of src/*.rs against my earlier copy.
- Test results on Windows (clippy only, not run).

## 9. Integrity
Main worktree HEAD 0dfa5d586f063dc3c7232ec9e6d0c28b5c89f991 before and
after. Status before: the seven modified spike files plus my untracked
earlier report; after: the same plus this report. Scratch copy and target
dir removed (below).
