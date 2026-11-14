# Stand-in demonstration, `hand` profile

**This is a stand-in.** The text in `documents/stand-in-rules.md` is a short synthetic rulebook
written for the RuleTrace project. It is not the rules of any real competition, it is not part of the
Demo_Corpus, and nothing here claims to reproduce a real document. All participants and values in
`data/` are `synthetic`.

The `hand` profile uses no AI: the revisions in `rulebook/hand-authored.json` were written by hand
and a human reviewer approves each one with `ruletrace review approve`, which creates the ledger.
Step s02 (`ruletrace-ai propose`) belongs to the `ai` profile and is skipped.

Reviewer `Demo Reviewer` and time `2026-01-15T10:00:00Z` are fixed arguments. No clock is read.
The content hashes in the approve steps are the Rule_Content_Hash of each revision; `review
approve` refuses a stale hash. Output files go to `work/`.

Steps: s01 ingest, s03a to s03c approve the three revisions, s04 validate, s05 compile, s06
evaluate against the equal published result, s06b evaluate against the published result with one
seeded difference (Warning RT-EXE-0011), s07 evaluate again with `--render` and the proof report.

Participant `p2` has no `age`, so its eligibility is Indeterminate by design. Therefore `evaluate`
exits 4 in s06, s06b and s07. That is the correct result of this demonstration. Participants `p3`
and `p5` tie on score and on the tie-breaker and form one Tie_Group.

Golden files under `golden/` are produced by the real binaries and blessed with `RT_DEMO_BLESS=1`.

```ruletrace-runbook
{
  "format": "ruletrace-runbook",
  "version": 1,
  "profiles": {
    "hand": {
      "since_slice": 0
    }
  },
  "steps": [
    {
      "id": "s01",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "ingest",
        "--document",
        "documents/stand-in-rules.md",
        "--document-id",
        "stand-in-rules.md",
        "--out",
        "work/source-model.json",
        "--candidates",
        "work/candidates.json"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s01.stdout",
      "stderr_golden": "golden/steps/s01.stderr",
      "outputs": [
        {
          "path": "work/source-model.json",
          "golden": "golden/artifacts/source-model.json"
        },
        {
          "path": "work/candidates.json",
          "golden": "golden/artifacts/candidates.json"
        }
      ]
    },
    {
      "id": "s03a",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/hand-authored.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--rule",
        "elig.entry",
        "--revision",
        "1",
        "--content-hash",
        "0b86ad7b084217b9ed8f0156f0b5c7a8b4f92269dd9fcdb1e637d77be9c15bd9",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z",
        "--create-ledger"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03a.stdout",
      "stderr_golden": "golden/steps/s03a.stderr",
      "outputs": []
    },
    {
      "id": "s03b",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/hand-authored.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--rule",
        "score.weighted",
        "--revision",
        "1",
        "--content-hash",
        "190408a6dbd4a51d1053cffce2cda86645fc50ba3d3894dccc268894c7159484",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03b.stdout",
      "stderr_golden": "golden/steps/s03b.stderr",
      "outputs": []
    },
    {
      "id": "s03c",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/hand-authored.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--rule",
        "rank.standing",
        "--revision",
        "1",
        "--content-hash",
        "b6be5b3d2abf7855cb0b228c1e0ebfed8d7f008e2a575b1712c892fcec284de9",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03c.stdout",
      "stderr_golden": "golden/steps/s03c.stderr",
      "outputs": [
        {
          "path": "work/ledger.jsonl",
          "golden": "golden/artifacts/ledger.jsonl"
        }
      ]
    },
    {
      "id": "s04",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "validate",
        "--rulebook",
        "rulebook/hand-authored.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--candidates",
        "work/candidates.json",
        "--out",
        "work/validation.json"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s04.stdout",
      "stderr_golden": "golden/steps/s04.stderr",
      "outputs": [
        {
          "path": "work/validation.json",
          "golden": "golden/artifacts/validation.json"
        }
      ]
    },
    {
      "id": "s05",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "compile",
        "--validation",
        "work/validation.json",
        "--out",
        "work/compiled.json"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s05.stdout",
      "stderr_golden": "golden/steps/s05.stderr",
      "outputs": [
        {
          "path": "work/compiled.json",
          "golden": "golden/artifacts/compiled.json"
        }
      ]
    },
    {
      "id": "s06",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "evaluate",
        "--compiled",
        "work/compiled.json",
        "--data",
        "data/canonical/data.json",
        "--questions",
        "data/canonical/questions.json",
        "--published",
        "data/canonical/published.json",
        "--source-model",
        "work/source-model.json",
        "--out",
        "work/results.json"
      ],
      "expect_exit": 4,
      "cwd": ".",
      "stdout_golden": "golden/steps/s06.stdout",
      "stderr_golden": "golden/steps/s06.stderr",
      "outputs": [
        {
          "path": "work/results.json",
          "golden": "golden/artifacts/results.json"
        }
      ]
    },
    {
      "id": "s06b",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "evaluate",
        "--compiled",
        "work/compiled.json",
        "--data",
        "data/canonical/data.json",
        "--questions",
        "data/canonical/questions.json",
        "--published",
        "data/canonical/published-seeded.json",
        "--source-model",
        "work/source-model.json",
        "--out",
        "work/results-seeded.json"
      ],
      "expect_exit": 4,
      "cwd": ".",
      "stdout_golden": "golden/steps/s06b.stdout",
      "stderr_golden": "golden/steps/s06b.stderr",
      "outputs": [
        {
          "path": "work/results-seeded.json",
          "golden": "golden/artifacts/results-seeded.json"
        }
      ]
    },
    {
      "id": "s07",
      "profile": [
        "hand"
      ],
      "argv": [
        "ruletrace",
        "evaluate",
        "--compiled",
        "work/compiled.json",
        "--data",
        "data/canonical/data.json",
        "--questions",
        "data/canonical/questions.json",
        "--published",
        "data/canonical/published.json",
        "--source-model",
        "work/source-model.json",
        "--out",
        "work/results.proof-run.json",
        "--ledger",
        "work/ledger.jsonl",
        "--rulebook",
        "rulebook/hand-authored.json",
        "--render",
        "--proof-report",
        "work/proof.txt",
        "--proof-report-json",
        "work/proof.json"
      ],
      "expect_exit": 4,
      "cwd": ".",
      "stdout_golden": "golden/steps/s07.stdout",
      "stderr_golden": "golden/steps/s07.stderr",
      "outputs": [
        {
          "path": "work/results.proof-run.json",
          "golden": "golden/artifacts/results.proof-run.json"
        },
        {
          "path": "work/proof.txt",
          "golden": "golden/artifacts/proof.txt"
        },
        {
          "path": "work/proof.json",
          "golden": "golden/artifacts/proof.json"
        }
      ]
    }
  ]
}
```
