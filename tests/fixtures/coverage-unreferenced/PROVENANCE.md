# coverage-unreferenced provenance

- Document: `source/rules.md` (118 bytes, SHA-256 `ddb2c3a8122f35c3e7424f4c4d2a946d4e8c5e8de1703a9204dc4f1020850bf1`).
- Source: original text written for RuleTrace. It contains no third-party text and is free to redistribute under the repository terms.
- Rulebook, data-free: hand-authored for this repository. One rule (`elig.age`) references clause 1. Clause 2 (bytes 72..117) is referenced by no rule on purpose.
- Ledger: written with `ruletrace review approve` (reviewer `A. Reviewer`, fixed time `2026-01-15T10:00:00Z`).
- Shows: `ruletrace validate --candidates` exits 0 and reports the Warning RT-ING-0006 for clause 2. The Coverage_Statement generator classes clause 1 `rule_approved` and clause 2 `not_a_rule` through `coverage-notes.json` (design section 8.11.8). Removing that note makes the generator fail naming the clause.
- Independent of Slice 0: no `slice-*` fixture or `demo/stand-in-hand` file uses it.
- Expected files: `source-model.json`, `candidates.json`, `validation.json`, `stderr.txt`, `exit-code` from the real binary and `coverage-statement.json` and `.md` from the generator, all through `RT_DEMO_BLESS=1 cargo test -p rt-demo --test coverage_unreferenced`, reviewed in the diff.
