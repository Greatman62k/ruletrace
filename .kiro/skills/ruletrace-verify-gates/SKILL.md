---
name: ruletrace-verify-gates
description: Run the RuleTrace local quality gates with network isolation. Use before reporting a RuleTrace task as done, before a commit that touches Rust, Cargo or tools files, or when asked to verify the workspace.
---

# RuleTrace verification gates

Every command that runs project code (tests, `cargo run`, the built
executables) runs inside the approved wrapper. The `guard-shell` hook blocks
any other form, including a weaker `unshare` without loopback.

```sh
W() { unshare -rn sh -c 'ip link set lo up && exec "$@"' sh "$@"; }
```

Without `ip link set lo up` the loopback stub tests fail (18 of 21 in
`rt-ai-responses`), so do not drop it.

## Scoped checks (after each increment)

1. Format only the files you changed: `rustfmt --edition 2021 --check <files>`.
   Never run `cargo fmt --all` while other agents edit the tree.
2. Tests of the touched crate: `W cargo test -p <crate> --all-features`.
3. Lint the touched crate: `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`.

## Full gates (at a checkpoint, before a task is reported complete)

Run in this order and record each exit code:

| Gate | Command |
| --- | --- |
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Tests | `W cargo test --workspace --all-features --no-fail-fast` |
| Demo | `W cargo test -p rt-demo --all-features` |
| Architecture | `W cargo run -q -p rt-xtask -- check-graph check-tree scan-sources scan-api check-decisions check-catalogue check-justification` |
| Dependencies | `cargo deny --offline check` |
| Windows compile | `cargo check --target x86_64-pc-windows-gnu --all-targets -p <crate>` (crates that depend on `ring` cannot be cross-checked here: no mingw C compiler) |

The CI workflow (`.github/workflows/ci.yml`) is the reference. Also run the
core-only job's commands when a change could affect the core crates.

## Reporting

Report each command with its exit code and test counts (passed, failed,
ignored). A skipped or not-run gate is reported as such, never as passing.
Follow `testing-integrity-and-anti-gaming.md` for what counts as evidence.
