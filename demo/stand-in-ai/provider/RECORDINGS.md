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
  profile (for example `...-prompt-3.json`). This file is never overwritten.
- This recording is not part of the runbook and of no golden file.

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

The run stores `work/replay/store/batches/1c574c2d....json` whose
`response`, `response_sha256`, `proposals` and `adapter_version` equal those
of the live batch. Its `provider` and `protocol` are `mock-replay`, and it
has no reported model, response identifier or usage, because the replay
invents none. The test `crates/rt-tests-ai/tests/replay_live_recording.rs`
checks this.
