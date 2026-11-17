---
name: ruletrace-catalogue-change
description: Add or change a RuleTrace diagnostic code (RT-DGN, RT-PRV, RT-APR, RT-AI and others). Use when a requirement or task introduces a new diagnostic, changes a code's meaning, severity or exit code, or when check-catalogue fails.
---

# Diagnostic catalogue change

The catalogue is the single source of every diagnostic code. One failure mode
has exactly one code.

1. Confirm the change is required by a requirement, design section or decision
   record. Do not add a code that the specification does not name. If two
   failure modes would share a code, stop and ask.
2. Edit `catalogue/diagnostics.v1.json`: code, mnemonic, severity, exit code,
   meaning, executable. Never renumber or remove an existing code (Req 7.11,
   7.12).
3. Regenerate the enum: `cargo run -q -p rt-xtask -- gen-catalogue`. Only
   `crates/rt-foundation/src/code_generated.rs` may change.
4. Update the code tables in `.kiro/specs/ruletrace-compiler/requirements.md`
   and `design.md` that list the code, with the same wording.
5. Verify: `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh cargo run -q -p rt-xtask -- check-catalogue`.
   It fails when the generated file differs from a fresh generation.
6. Add a test that provokes the new code and asserts its exact code, severity,
   exit code and location.
7. Record the change in `traceability.md`.

A tag in this repository is treated as a release tag by `check-catalogue`,
which then requires `catalogue/baseline.v1.json` (task 12.12). Do not create
tags.
