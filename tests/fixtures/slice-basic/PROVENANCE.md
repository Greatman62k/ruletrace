# slice-basic provenance

- Document: `source/rules.md` (171 bytes, SHA-256 `eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a`).
- Source: the worked example of design section 4.1.7 of this repository (original text written for RuleTrace).
- Permission: authored by the project; free to redistribute under the repository terms. It contains no third-party text.
- Rulebook, ledger, data: hand-authored for this repository (synthetic participants). The ledger was written with `ruletrace review approve` (reviewer `A. Reviewer`, fixed time).
- Shows: eligibility, weighted score with exact decimal weights and a ranking with a tie-breaker (p2 and p4 tie on 7.8 and p4 wins on the judges score).
- Expected files: `source-model.json` from `ruletrace ingest`; the others through `RULETRACE_BLESS=1 cargo test -p rt-tests-core --test golden golden_slice`, reviewed in the diff.
