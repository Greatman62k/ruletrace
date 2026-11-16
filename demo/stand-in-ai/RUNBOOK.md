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

## Live steps and document transmission

This section is about the optional live flow (design §8.11.6, Req 33.10). Nothing in it runs in the
offline acceptance test, in CI or by default. The harness never executes a fenced block marked
`live`, and the `ai` profile above stays offline and byte-identical.

**What is transmitted.** A live `ruletrace-ai propose` sends the text of the documents named by
`--consent`, one `--consent <document id>` per document, and nothing else. For this stand-in the only
document is `stand-in-rules.md`: a synthetic text written for RuleTrace. Each send is logged in the
Transmission_Log without document text and without credentials. Without `--consent` naming a document,
`ruletrace-ai` and the harness both refuse.

**Permission.** The harness runs a live step only when `manifest.json` has an entry for every
document the step names with `--consent` and that entry's `transmission_permitted` is exactly the
boolean `true`. A document that is missing from the manifest, has `false`, or has any other value is
refused before `ruletrace-ai` is started, and the refusal names the document and the field. A live
step that names no `--consent` is refused. The harness treats every `ruletrace-ai` step as live unless
its configuration is the `mock-replay` provider; a configuration it cannot read, or one with an
unknown protocol, counts as live. The real Kiroween and ICC documents are not in this repository.
They get manifest entries with task 11.1, and they are not transmitted before an entry with
`transmission_permitted = true` exists.

**Where a live run writes.** The live flow writes its store, its `--out` files and its
Transmission_Log only outside `golden/` and outside `provider/`. The harness refuses a live step whose
`--store`, `--out` or configured `transmission_log` is, or resolves (through `..` or a symbolic link)
to, a location at or under `golden/` or `provider/` of the demonstration. A live run never replaces a
golden file or a Recorded_Provider_Script. To keep a live response, the user inspects the batches and
exports a candidate script with `rt-xtask export-script` in a deliberate commit (task 10.8).

**Configurations.** `ai/meta.example.toml` (credential variable `MODEL_API_KEY`) and
`ai/openai.example.toml` (credential variable `OPENAI_API_KEY`) are examples for local use. Copy one
outside the repository, set the mode to 600 and edit it there. The OpenAI example holds the placeholder
`REPLACE_WITH_YOUR_MODEL_ID` as its model; you must put your own model identifier in your copy. The
files name the credential variable only. Never write a credential into a file.

**Running a live step by hand today.** Export the credential variable in your own shell first. Do not
paste a credential into a file or into this document. The blocks below are marked `live`, so the
harness parser accepts them and never runs them. `REPO` is your checkout. Run all blocks in one shell,
where the first block puts `target/debug` on `PATH`.

```sh live
export REPO="$HOME/Desktop/ruletrace"
export PATH="$REPO/target/debug:$PATH"
cargo build --manifest-path "$REPO/Cargo.toml" -p rt-cli -p rt-ai-tool
mkdir -p ~/ruletrace-live/documents ~/ruletrace-live/rulebook ~/ruletrace-live/work
cd ~/ruletrace-live
cp "$REPO/demo/stand-in-ai/documents/stand-in-rules.md" documents/
cp "$REPO/demo/stand-in-ai/rulebook/seed.json" rulebook/
cp "$REPO/demo/stand-in-ai/ai/meta.example.toml" meta.toml
chmod 600 meta.toml
```

Ingest the document and print the Request_Identifier. The dry run sends nothing and reads no
credential:

```sh live
cd ~/ruletrace-live
ruletrace ingest --document documents/stand-in-rules.md \
  --document-id stand-in-rules.md --out work/source-model.json --candidates work/candidates.json
ruletrace-ai propose --dry-run \
  --config meta.toml --rulebook rulebook/seed.json \
  --source-model work/source-model.json --candidates work/candidates.json \
  --store work/store --created-at 2026-11-16T00:00:00Z \
  --consent stand-in-rules.md
```

The live propose. It sends the text of `stand-in-rules.md` to the provider named in `meta.toml`.
It needs the credential variable exported in the same shell. Start from a fresh `work/store`:

```sh live
cd ~/ruletrace-live
ruletrace-ai propose \
  --config meta.toml --rulebook rulebook/seed.json \
  --source-model work/source-model.json --candidates work/candidates.json \
  --store work/store --created-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --consent stand-in-rules.md
```

**Machine-readable live steps.** A step of the `ruletrace-runbook` block may carry `"live": true`. It
must be a `ruletrace-ai` step with no golden `outputs`. `steps_for` leaves it out, so the offline
runner never sees it, and only `run_live_step` runs it, with the checks above. No such step is
committed today.
