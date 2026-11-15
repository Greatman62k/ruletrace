# Stand-in demonstration, `ai` profile

**This is a stand-in.** The text in `documents/stand-in-rules.md` is a short synthetic rulebook
written for the RuleTrace project. It is not the rules of any real competition, it is not part of the
Demo_Corpus, and nothing here claims to reproduce a real document. All participants and values in
`data/` are `synthetic`. Its first three clauses are the clauses of the `hand` demonstration
(`demo/stand-in-hand`); clauses 4 and 5 were added for this profile.

**The provider script is authored, not provider output.** `provider/authored-script.json` holds two
entries, both labelled `authored`: a person wrote them by hand for this demonstration. No AI provider
produced them, and nothing in this demonstration is a recording of a provider. Each entry is keyed by
the Request_Identifier that `ruletrace-ai propose --dry-run` prints for the stand-in. The mock
provider (`ai/mock-replay.toml`, provider `mock-replay`) replays them offline: no network, no
credential, no endpoint.

Reviewer `Demo Reviewer` and times `2026-01-15T09:00:00Z` (propose) and `2026-01-15T10:00:00Z`
(review) are fixed arguments. No clock is read. The offline flow replays the recorded decisions of
the demonstration Reviewer and does not make a new human decision. Every Approval_Record and the
rejection record were written by `ruletrace review`, never by hand.

Steps: s01 ingest. s02 `ruletrace-ai propose` sends clauses 1 to 3 in one request and clauses 4 and
5 in another (`max_clauses_per_request = 3`) and stores two batches and a Transmission_Log. s02b
`ruletrace review list` shows the five pending proposals before any decision: no ledger exists yet,
and `review list` then reads as over an empty ledger, writes nothing and creates no file. s03a to
s03f decide the five proposals: s03a approves `elig.entry` and creates the ledger, s03c approves
`score.weighted`, s03d approves `rank.standing`, s03e rejects `junior.flag` with a reason, s03f lists
what is still pending. There is no step s03b: that listing is now s02b. `rank.standing` reads the target `score` that `score.weighted`
declares, so it is approved after `score.weighted`: an earlier approval is refused with RT-APR-0205.
s04 validate, s05 compile, s06 evaluate against the equal published result, s06b evaluate against
the published result with one seeded difference (Warning RT-EXE-0011), s06c evaluate over
`data/canonical/data-complete.json`, s07 evaluate again with `--render` and the proof report, s07b
the same over the complete data.

Clause 4 (prize money) is outside the Rule_IR. Its proposal has the kind `unrecognized` and stays
pending with Unsupported RT-VAL-0003: **1 unsupported clause**. It reaches the Coverage_Statement
as `unsupported`. Clause 5 is proposed with a deliberate misreading (age at most 18, where the text
says under 18) and the Reviewer rejects it with a reason, so the Coverage_Statement lists it as
`proposal_rejected` with that reason. Clauses 1 to 3 are `rule_approved`.

Participant `p2` has no `age` in `data/canonical/data.json`, so its eligibility is Indeterminate by
design. Therefore `evaluate` exits 4 in s06, s06b and s07. That is the correct result of this
demonstration: exit code 4 means "at least one Outcome is not decided", and the tool has not failed.
`data/canonical/data-complete.json` supplies the age of `p2` (20), every Outcome is decided and
`evaluate` exits 0 in s06c and s07b. The values are those of the `hand` profile: p1, p2, p4 and p5
eligible and p3 not; scores 7.6, 7.8, 6, 7.8 and 6; standing p2, p4, p1, then the Tie_Group of p3 and
p5.

Platform note: where the platform cannot sync a directory (Windows), `propose` adds the Warning
RT-AI-0309 and a different Run_Summary. The files `golden/steps/s02.stdout.windows` and
`golden/steps/s02.stderr.windows` hold that output and are used on Windows only.

Golden files under `golden/` are produced by the real binaries and blessed with `RT_DEMO_BLESS=1`.

```ruletrace-runbook
{
  "format": "ruletrace-runbook",
  "version": 1,
  "profiles": {
    "ai": {
      "since_slice": 1
    }
  },
  "steps": [
    {
      "id": "s01",
      "profile": [
        "ai"
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
      "id": "s02",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace-ai",
        "propose",
        "--config",
        "ai/mock-replay.toml",
        "--rulebook",
        "rulebook/seed.json",
        "--source-model",
        "work/source-model.json",
        "--candidates",
        "work/candidates.json",
        "--store",
        "work/store",
        "--created-at",
        "2026-01-15T09:00:00Z",
        "--consent",
        "stand-in-rules.md"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s02.stdout",
      "stderr_golden": "golden/steps/s02.stderr",
      "outputs": [
        {
          "path": "work/store/batches/ab5e5c4a688908ce49628b8e0ab189dcd4ef095f97e745f13487e80a9de9f071.json",
          "golden": "golden/artifacts/proposal-store/batches/ab5e5c4a688908ce49628b8e0ab189dcd4ef095f97e745f13487e80a9de9f071.json"
        },
        {
          "path": "work/store/batches/be41f0e453a0518479e65a8da0bb7035989b86e5aa4727e7c851304c37c2d2b7.json",
          "golden": "golden/artifacts/proposal-store/batches/be41f0e453a0518479e65a8da0bb7035989b86e5aa4727e7c851304c37c2d2b7.json"
        },
        {
          "path": "work/transmission.log.jsonl",
          "golden": "golden/artifacts/transmission.log.jsonl"
        }
      ]
    },
    {
      "id": "s02b",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "list",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s02b.stdout",
      "stderr_golden": "golden/steps/s02b.stderr",
      "outputs": []
    },
    {
      "id": "s03a",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store",
        "--rule",
        "elig.entry",
        "--revision",
        "1",
        "--content-hash",
        "9edaa59e63831bf8d71ccba89eb6025bac65019cbd424ed1348981e784dc368c",
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
      "id": "s03c",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store",
        "--rule",
        "score.weighted",
        "--revision",
        "1",
        "--content-hash",
        "9fa6292ebbd6de150b1021e75b47a0ffca25db2da4c24c5ecd17f2c249136dac",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03c.stdout",
      "stderr_golden": "golden/steps/s03c.stderr",
      "outputs": []
    },
    {
      "id": "s03d",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store",
        "--rule",
        "rank.standing",
        "--revision",
        "1",
        "--content-hash",
        "0bc9dbf21d05814446b9ef0b9466e0f7de03c96b26bdebcb8916a8a328628224",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03d.stdout",
      "stderr_golden": "golden/steps/s03d.stderr",
      "outputs": []
    },
    {
      "id": "s03e",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "reject",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store",
        "--rule",
        "junior.flag",
        "--revision",
        "1",
        "--content-hash",
        "8c5f984c35ba64a1f5cff1220505cabf1e0f09a074007909d9e8a42d99df8282",
        "--reviewer",
        "Demo Reviewer",
        "--time",
        "2026-01-15T10:00:00Z",
        "--reason",
        "The clause says under 18; the proposal tests age at most 18, which makes an entrant aged exactly 18 a junior."
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03e.stdout",
      "stderr_golden": "golden/steps/s03e.stderr",
      "outputs": [
        {
          "path": "rulebook/seed.json",
          "golden": "golden/artifacts/rulebook.json"
        },
        {
          "path": "work/ledger.jsonl",
          "golden": "golden/artifacts/ledger.jsonl"
        }
      ]
    },
    {
      "id": "s03f",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "review",
        "list",
        "--rulebook",
        "rulebook/seed.json",
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--proposals",
        "work/store"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s03f.stdout",
      "stderr_golden": "golden/steps/s03f.stderr",
      "outputs": []
    },
    {
      "id": "s04",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "validate",
        "--rulebook",
        "rulebook/seed.json",
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
        "ai"
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
        "ai"
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
        "ai"
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
      "id": "s06c",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "evaluate",
        "--compiled",
        "work/compiled.json",
        "--data",
        "data/canonical/data-complete.json",
        "--questions",
        "data/canonical/questions.json",
        "--published",
        "data/canonical/published.json",
        "--source-model",
        "work/source-model.json",
        "--out",
        "work/results-complete.json"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s06c.stdout",
      "stderr_golden": "golden/steps/s06c.stderr",
      "outputs": [
        {
          "path": "work/results-complete.json",
          "golden": "golden/artifacts/results-complete.json"
        }
      ]
    },
    {
      "id": "s07",
      "profile": [
        "ai"
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
        "rulebook/seed.json",
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
    },
    {
      "id": "s07b",
      "profile": [
        "ai"
      ],
      "argv": [
        "ruletrace",
        "evaluate",
        "--compiled",
        "work/compiled.json",
        "--data",
        "data/canonical/data-complete.json",
        "--questions",
        "data/canonical/questions.json",
        "--published",
        "data/canonical/published.json",
        "--source-model",
        "work/source-model.json",
        "--out",
        "work/results-complete.proof-run.json",
        "--ledger",
        "work/ledger.jsonl",
        "--rulebook",
        "rulebook/seed.json",
        "--render",
        "--proof-report",
        "work/proof-complete.txt",
        "--proof-report-json",
        "work/proof-complete.json"
      ],
      "expect_exit": 0,
      "cwd": ".",
      "stdout_golden": "golden/steps/s07b.stdout",
      "stderr_golden": "golden/steps/s07b.stderr",
      "outputs": [
        {
          "path": "work/results-complete.proof-run.json",
          "golden": "golden/artifacts/results-complete.proof-run.json"
        },
        {
          "path": "work/proof-complete.txt",
          "golden": "golden/artifacts/proof-complete.txt"
        },
        {
          "path": "work/proof-complete.json",
          "golden": "golden/artifacts/proof-complete.json"
        }
      ]
    }
  ]
}
```
