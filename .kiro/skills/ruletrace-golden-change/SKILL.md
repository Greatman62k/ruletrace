---
name: ruletrace-golden-change
description: Change a RuleTrace golden file, fixture or recording (demo/*/golden, tests/fixtures, crates/*/tests/fixtures, recorded provider scripts). Use when a golden comparison fails, when an output format changes intentionally, or when the guard-golden hook blocks a write.
---

# Golden, fixture and recording changes

Golden files are reviewed specifications, not generated output. The
`guard-golden` hook blocks every write to them until the user approves the
exact change.

1. Establish why the output changed. A failing golden is first a possible
   defect. Change a golden only when the new behaviour is required by a
   requirement, design section or user decision. Name it.
2. Show the old and the new bytes. For small files, the full diff. For large
   files, the changed hunks and a proof that nothing else changed (for example
   a script that strips the intended change and compares with the old file).
3. Preserve unrelated content byte for byte. Never bless a whole directory to
   change one file.
4. Get approval:
   - Agent write tools: the hook prints a pending id and a diff file under
     `.git/ruletrace-golden/pending/`. The user reviews it and runs
     `tools/hooks/golden_approve.py <id>` on a terminal. The approval is
     bound to the path, the old and new content hashes, expires, and works once.
     Then repeat the identical write once.
   - A shell command that regenerates goldens (`RT_DEMO_BLESS=1 ...`): the
     user approves the exact command with
     `tools/hooks/golden_approve.py --command "<exact command>"`.
5. Re-run the comparison test and show it passes.
6. Record the change, its reason and the approval in the commit message and in
   `traceability.md`.

Recorded provider scripts are evidence of a live call and are never edited.
A new recording gets a new file.
