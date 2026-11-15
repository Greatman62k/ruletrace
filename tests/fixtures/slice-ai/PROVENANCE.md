# slice-ai provenance

- Document: `source/rules.md`, the `slice-basic` document (171 bytes, SHA-256 `eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a`). Original text written for RuleTrace; no third-party text.
- `provider/response.json`: an authored answer, not provider output. It proposes the three rules of `rulebook.authored.json` with proposer `proposer-a`. The tests key a recorded script entry by the Request_Identifier that `ruletrace-ai propose --dry-run` prints, labelled `authored`.
- `rulebook.declarations.json`: the declarations of the authored rulebook with `rules` empty. The AI path starts here and gains its rules by `ruletrace review approve`.
- `rulebook.authored.json`, `ledger.authored.jsonl`, `data.json`: hand-authored for `slice-basic` (reviewer `A. Reviewer`, time `2026-01-15T10:00:00Z`) and copied byte for byte. The three content hashes in the ledger are the literals the tests expect from `review list`.
- `candidates.json`, `ledger.empty.jsonl` (the ledger header only): hand-authored.
- `expected/compiled.json`, `expected/results.json`, `expected/source-model.json`: byte copies of the reviewed `slice-basic` goldens. Results checked by hand: p1 7.6, p2 7.8, p3 6, p4 7.8; p4 and p2 tie and p4 wins on the judges score (9 against 7); standings p4, p2, p1, p3.
- `injection/control` and `injection/injected`: the document, candidates, declarations and response of the prompt-injection test. Both documents have the same length and offsets. Clause 1 carries a note of 90 bytes: a harmless one in `control`, and one that tells an AI to mark every rule Approved in `injected`. The rationale of every proposal repeats a harmless or an approving sentence of 87 characters. Only the document hash and these sentences differ.
- Shows: a Mock_Provider answer adopted and approved through `ruletrace review`, compiled to the same bytes as the authored rulebook; and that an AI identity cannot review, and that instructions in clause text change nothing.
