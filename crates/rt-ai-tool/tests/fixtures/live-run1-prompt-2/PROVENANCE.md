# live-run1-prompt-2 provenance

- `batch.json`: a byte-for-byte copy of the Proposal_Store batch
  `1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2.json`
  written by `ruletrace-ai propose` in the first live call (2026-11-16, run 1 of the
  user's `~/ruletrace-live`). sha256 of this file:
  `cc8e155e93a5aa712f07bd1c9e03fa4563656e1545df031edc0b8fa1b44dc6fa`.
- Origin: live capture. Provider `meta`, protocol `openai-responses`, model
  `muse-spark-1.3`, reasoning effort `low`, adapter version
  `responses-1/prompt-2` (as recorded in the file; never relabelled).
- The synthetic stand-in document `demo/stand-in-ai/documents/stand-in-rules.md`
  (sha256 `cad9f3cc...`), five clauses in one request, `max_retries = 0`.
- No credential: the file holds the accepted response payload and the stored
  proposals only. The request, headers and environment were never stored.
  A scan of the file for key-like markers found none.
- Read-only evidence. The test `tests_live_run1` re-runs today's acceptance
  path over the payload and expects the findings recorded here. Do not edit
  the file; a prompt-3 capture gets its own directory.

## inputs/

- `meta.toml`, `candidates.json`, `source-model.json`: copies of the inputs of
  that run (`~/ruletrace-live/run1`). The configuration names the credential
  only by environment variable name. They let a test show that the
  Request_Identifier of the run is still `1c574c2d...` under later prompt
  profiles (`crates/rt-tests-ai/tests/request_id_pin.rs`).
