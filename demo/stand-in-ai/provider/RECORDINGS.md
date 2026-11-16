# Recorded provider responses of the stand-in

`authored-script.json` is the script of the offline runbook. Its two entries
are authored by hand and are not provider output.

`recorded-meta-muse-spark-1.3-prompt-2.json` is a recording: one entry of
origin `recorded`, the accepted response of the first live call to the Meta
Model API, exported with

```sh
rt-xtask export-script --store <copy of the live store> \
  --provider openai-responses --provider-id meta --model muse-spark-1.3 \
  --captured "Meta Model API muse-spark-1.3, reasoning_effort low, captured 2026-11-16 by the user, synthetic stand-in document" \
  --out demo/stand-in-ai/provider/recorded-meta-muse-spark-1.3-prompt-2.json
```

- Request `1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2`,
  adapter version `responses-1/prompt-2`, response sha256 `9585ce1d...`.
- The source batch is kept as
  `crates/rt-ai-tool/tests/fixtures/live-run1-prompt-2/batch.json`.
- The script format (version 1) holds the provider identifier, the protocol,
  the model, the adapter version, the capture text, and per entry the request
  identifier, the completion, the payload and its hash. It has no member for
  the reported model, the response identifier, the token usage or the
  document hash; those stay in the source batch only.
- A recording of another prompt profile gets its own file, named with its
  profile. This file is never overwritten.
- This recording is not part of the runbook and of no golden file.

## The recordings

Both recordings have the same Request_Identifier, because the prompt profile
does not change it. They are separate files and are never merged: the export
refuses a store whose batches differ in adapter version, the script loader
refuses a repeated `request_id`, and a configuration names exactly one
script (`script` is a single string; a list or a second key is refused).

| | prompt-2 | prompt-3 |
|---|---|---|
| File | `recorded-meta-muse-spark-1.3-prompt-2.json` | `recorded-meta-muse-spark-1.3-prompt-3.json` |
| Capture text | `Meta Model API muse-spark-1.3, reasoning_effort low, captured 2026-11-16 by the user, synthetic stand-in document` | `Meta Model API muse-spark-1.3, reasoning_effort low, prompt-3, captured 2026-11-16 by the user, synthetic stand-in document` |
| Request id | `1c574c2d...` | `1c574c2d...` (same) |
| Response sha256 | `9585ce1dac804dc1...` | `7f3371ca503a4f61...` |
| Adapter version | `responses-1/prompt-2` | `responses-1/prompt-3` |
| Source batch | `crates/rt-ai-tool/tests/fixtures/live-run1-prompt-2/batch.json` | `crates/rt-ai-tool/tests/fixtures/live-run2-prompt-3/batch.json` |
| Replay configuration | `ai/meta-replay.toml` | `ai/meta-replay-prompt-3.toml` |
| File sha256 | `628dc20dd933c69a...` | `f0a1a377b9e2f476...` |

The prompt-3 file was exported from a read-only copy of the live store with
the command above, the prompt-3 capture text and `--out` set to its path.
The store was the user's `~/ruletrace-live/run2/work/store`.

## What the recording keeps

| Held in the recording script (version 1) | Only in the original batch |
|---|---|
| provider identifier, protocol, model | reported model |
| adapter version | response identifier |
| capture text | token usage |
| request identifier | `created_at` |
| completion, payload, payload hash | `ai_identities` |
| | proposals as stored, with their validation outcomes |
| | document hash and the Transmission_Log |

A replay writes only what the script holds, plus what the replay run itself
computes (its own proposals, `created_at` from `--created-at`). It does not
fill a member that the script does not hold: the reported model, response
identifier and usage of a replayed batch are absent, and its provider and
protocol are `mock-replay`. Nothing omitted is fabricated.

The adapter version is kept: the prompt-3 replay stores `responses-1/prompt-3`
and the prompt-2 replay stores `responses-1/prompt-2`, though the hosted
adapter of this build reports prompt-3. The test
`replay_live_recording.rs` proves, per recording and against the live batch
fixture, that the replayed payload, response hash, proposals, request id,
model, `created_at` and adapter version equal the live ones, and that the two
replays differ from each other in payload hash and adapter version.

## Offline replay

Run from a scratch copy of `demo/stand-in-ai` (the configuration paths are
relative to the working directory). No credential, no endpoint, no network:

```sh
ruletrace ingest --document documents/stand-in-rules.md --document-id stand-in-rules.md \
  --out work/replay/source-model.json --candidates work/replay/candidates.json
ruletrace-ai propose --config ai/meta-replay.toml --rulebook rulebook/seed.json \
  --source-model work/replay/source-model.json --candidates work/replay/candidates.json \
  --store work/replay/store --created-at 2026-11-16T05:13:25Z --consent stand-in-rules.md
```

For prompt-3, use `ai/meta-replay-prompt-3.toml` and `--created-at
2026-11-16T09:08:32Z`. Each run stores `work/replay/store/batches/1c574c2d....json` whose
`response`, `response_sha256`, `proposals` and `adapter_version` equal those
of the live batch. Its `provider` and `protocol` are `mock-replay`, and it
has no reported model, response identifier or usage, because the replay
invents none. The test `crates/rt-tests-ai/tests/replay_live_recording.rs`
checks this.

## The proposer is a model claim

The `proposer` of a stored proposal is text that the model wrote. It is not
an identity that RuleTrace verified, and it varies between runs: the
prompt-2 recording (run1) has `"model"` for all five proposals, the prompt-3
recording (run2) has `"muse-spark-1.3"`. Provider and model, by contrast, come
from the configuration. `ruletrace review list` renders the line as
`origin: AI proposal '<id>' (proposer claimed by the model: '<p>', provider
'<prov>', model '<m>')`. The stored batches and recordings are unchanged, and
the self-approval refusal still includes the claimed text.
