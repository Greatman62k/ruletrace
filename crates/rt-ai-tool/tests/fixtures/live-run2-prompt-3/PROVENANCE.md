# live-run2-prompt-3 provenance

- `batch.json`: a byte-for-byte copy of the Proposal_Store batch
  `1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2.json`
  written by `ruletrace-ai propose` in the live call with the prompt-3
  profile (2026-11-16, run 2 of the user's `~/ruletrace-live`). sha256 of
  this file: `9230882b8ca93ca4d1efe96ffa7374941efb2785f8b1884f8c70b1a4ded5b156`.
- Origin: live capture. Provider `meta`, protocol `openai-responses`, model
  `muse-spark-1.3`, reasoning effort `low`, adapter version
  `responses-1/prompt-3` (as recorded in the file; never relabelled),
  `created_at` `2026-11-16T09:08:32Z`, response sha256 `7f3371ca...`.
- The same synthetic stand-in document as run 1
  (`demo/stand-in-ai/documents/stand-in-rules.md`, sha256 `cad9f3cc...`),
  five clauses in one request, `max_retries = 0`. The Request_Identifier is
  the same as that of run 1 (`1c574c2d...`): it is derived from the request
  inputs, which the prompt profile does not change. The two batches are
  different files in different stores, and their payloads differ.
- No credential: the file holds the accepted response payload and the stored
  proposals only. The request, headers and environment were never stored.
  A scan of the file for key-like markers found none.
- Every value in the recording script exported from this file was read from
  this batch. Nothing was invented. The reported model, the response
  identifier, the token usage and `created_at` live only in this batch; the
  script format has no member for them.
- Read-only evidence. The test `replay_live_recording` and the test
  `export_script` compare against this file. Do not edit it.
