# Provider profiles

The `rt-ai-tool` module `profile` holds one Provider_Profile per provider
identifier as constants. A profile has no network, environment or credential
code and selects no model (Req 35.5, UD-37). This page lists the source and
the day each fact was read. A test checks that every URL in the profile data
appears here.

Facts that no source read for this version states are `Unknown` in the code
and are not relied on. A configuration may restate a capability or lower it
from `yes` to `no`. It cannot raise one.

## Common to `meta` and `openai`

| Item | Value |
| --- | --- |
| Protocol | `openai-responses` |
| Request path | `/v1/responses`, appended to the endpoint |
| Model catalogue path | `/v1/models`, appended to the endpoint |
| `max_output_tokens` | Provisional 8192, kept by task 10.1 and not measured. Reasoning tokens count toward it, so a long reasoning phase can leave no room for the answer, which then ends as `incomplete` (RT-AI-0205). Task 10.8 revises it after the live run. The minimum is 16. The value lives in the profile and reaches `rt-ai-responses` through `Settings`, because the crate must not depend on `rt-ai-tool`. No price or cost is estimated. |
| Context window, output token limit | Unknown. No source read for this version states them. |

## `meta`

Official host: `api.meta.ai`. Only this host (after ASCII lower-casing) or a
loopback address (for the stub tests) receives the credential.

| Fact | Value in the profile | Source | Read on |
| --- | --- | --- | --- |
| Structured output | JSON Schema, `strict: true` (`yes`) | https://dev.meta.ai/docs/structured-output | 2026-11-16 |
| Recursive schema | not accepted (`no`) | https://dev.meta.ai/docs/structured-output | 2026-11-16 |
| Maximum nesting depth | 10 | https://dev.meta.ai/docs/structured-output | 2026-11-16 |
| Maximum properties | 5,000 | https://dev.meta.ai/docs/structured-output | 2026-11-16 |
| Reasoning effort control (`reasoning.effort` on the Responses API) | `yes` | https://dev.meta.ai/docs/reasoning | 2026-11-16 |
| Accepted effort values | `minimal`, `low`, `medium`, `high`, `xhigh`, `max`. `none` is refused by Muse Wonder, Spark and Ember with HTTP 400. | https://dev.meta.ai/docs/reasoning | 2026-11-16 |
| `max` per model | Accepted only with `muse-spark-1.3`, `muse-ember-1.0` and `muse-wonder-1.0`, the three models that both pages name. Any other model identifier, including `muse-spark-1.1`, `muse-spark-1.2` and `muse-spark-1.3-contributor`, is refused with RT-AI-0002 at the effort field, so a model that a page does not list fails closed. The other values are not limited by model. | https://dev.meta.ai/docs/reasoning, https://dev.meta.ai/docs/protocols/responses | 2026-11-16 |
| Store control (`store: false`) | `yes`. The Responses page documents `store` (default `true`) and says that `store: false` with stateless replay keeps no server copy. The request always sends `store: false`. No run has observed the behaviour yet. | https://dev.meta.ai/docs/protocols/responses | 2026-11-16 |
| Model discovery (`GET /v1/models`) | `yes`. An unauthenticated request answers HTTP 401, so the path exists. | UD-37 section 2 | 2026-11 |
| Base URL for OpenAI-compatible clients | `https://api.meta.ai/v1` | https://dev.meta.ai/docs/authentication | 2026-11 (UD-37, search snippet) |
| Usage reporting | The capability stays `Unknown` until a live run observes it. The Responses page documents `usage` with `input_tokens`, `output_tokens` and `total_tokens`, and the adapter reads exactly those three members, each only when it is a non-negative integer. A retrieved response also carries `output_tokens_details.reasoning_tokens`, which is not read. | https://dev.meta.ai/docs/protocols/responses | 2026-11-16 |
| Gateway timeout | A non-streaming request that runs longer than a server-side time limit returns HTTP 504 (`gateway_timeout`). Streaming requests are exempt. RuleTrace does not stream, so a long reasoning generation can end in 504. A 504 is a final failure, never retried (Req 29.2; OD-37-1 resolved by the user on 2026-11-16). The error page names `background: true` on the Responses API as the most reliable remedy, because background requests are subject to neither time limit; the request would then be polled or retrieved. RuleTrace does not use background mode, and a design decision would be needed before it did. | https://dev.meta.ai/docs/protocols/responses | 2026-11-16 |

## `openai`

Official host: `api.openai.com`.

| Fact | Value in the profile | Source | Read on |
| --- | --- | --- | --- |
| Structured output | JSON Schema, `strict: true` (`yes`) | https://developers.openai.com/api/docs/guides/structured-outputs | 2026-11-16 |
| Recursive schema | accepted (`yes`). The generated Proposal schema is flat and never recursive. | https://developers.openai.com/api/docs/guides/structured-outputs | 2026-11-16 |
| Maximum nesting depth | 10 ("up to 10 levels of nesting") | https://developers.openai.com/api/docs/guides/structured-outputs | 2026-11-16 |
| Maximum properties | 5,000 ("up to 5000 object properties total") | https://developers.openai.com/api/docs/guides/structured-outputs | 2026-11-16 |
| Reasoning effort control | Unknown. A run that configures an effort fails closed with RT-AI-0011. | UD-37 section 3 | not verified |
| Accepted effort values | none accepted until the capability is verified | UD-37 section 3 | not verified |
| Usage reporting | Unknown | UD-37 section 3 | not verified |
| Store control, model discovery | Unknown. No page was read for this version, and neither is used by a real run yet. | none read | not verified |

The re-check of the OpenAI structured-output limits that task 10.12 requires
found the same two values that design section 3.7.3 states (5,000 properties,
10 levels). The page was read on 2026-11-16. The other limits on that page
(120,000 characters of names and enum values, 1,000 enum values) were not
copied into the profile.

## `mock-replay`

No host, no documentation. The offline replay of a recorded script sends
nothing. Its capabilities equal those of `ReplayProvider`: structured output
`yes`, reasoning effort `no`, usage reporting `no`.

## Request and transport (`rt-ai-responses`, tasks 10.1 and 10.2)

`POST {endpoint}/v1/responses` with exactly these members, in this order:
`model`, `input`, `text.format` (`json_schema`, `name`, `schema`, `strict:
true`), `store: false`, `max_output_tokens`, and `reasoning.effort` only when
configured. There is no `tools` member. `input` is a constant text
(`INPUT_PREAMBLE`, eighteen lines in prompt profile 3, each a rule that the
tool enforces and that a test in `rt-ai-tool` asserts) followed by the canonical request bytes that
passed the credential scan. The schema carries `description` members under the
same rule. The wording is the prompt profile `N` of the adapter version
`responses-1/prompt-N` (Req 35.23), which a stored batch records; it is not
part of the Request_Identifier.

Prompt profiles: `prompt-2` (eleven lines, the first live call of 2026-11-16)
and `prompt-3` (seven added lines and seven added description sentences on
the rule `type` and the node `t`, after that call declared a ranking rule as
`number` and a condition as kind `derived`). Stored prompt-2 batches keep
their recorded adapter version. The credential travels only in the
`Authorization: Bearer` header to the configured host. Redirects are not
followed, proxy variables are ignored, and the embedded `webpki-roots` store
verifies the server. Error response bodies are never read.

An envelope that is not valid JSON or has another shape (a tool call, a
second message, a second `output_text`, no message) yields an empty payload
with `Completion::Complete` and a fixed reason in the attempt detail. The
caller then reports RT-AI-0201 because an empty payload is not JSON. A body
larger than the response limit is returned as its first `limit + 1` bytes, so
the caller reports RT-AI-0203. HTTP 504 maps to `GatewayTimeout`, a final
failure under Req 29.2 (OD-37-1, resolved by the user on 2026-11-16). Every
other 5xx maps to `Unavailable` (transient).

## Error bodies are never read (task 10.15, user decision of 2026-11-16)

The mutant "branch on the error body `type`" cannot exist, so it is accepted
as structurally impossible and no error-body parser is added. The mapping is
`status::failure_for(status, retry_after)` in
`crates/rt-ai-responses/src/status.rs`, which takes a status code and the
`Retry-After` header values only. In `crates/rt-ai-responses/src/transport.rs`
the function `read` returns for every non-2xx status before it touches the
body reader, and drops the response. Tests in
`crates/rt-ai-responses/tests/stub_http.rs`:

- `a_stalled_error_body_is_not_waited_for_in_any_non_2xx_status`: for each of
  400, 401, 402, 403, 429, 500, 503 and 504 the head declares a 1 MiB body, the
  stub sends a few bytes and stalls for 1.5 s, and the adapter returns the
  status's failure in under 500 ms. A transport that read the body would wait
  for the stall. A mutant that drains the body of any one of these statuses
  fails the test.
- `a_401_or_403_is_auth_failed_with_one_attempt_for_every_body_shape`: changed
  `type`, a `type` that names a rate limit, empty body with and without a
  content type, non-JSON text, invalid UTF-8, truncated JSON, HTML, JSON under
  another content type, no content type, and the credential echoed in the
  body. Each gives `AuthFailed`, one request, `NonTransient` stop in the retry
  decision, and no canary in the report.
- `an_oversize_error_body_gives_the_same_failure_as_an_empty_one` and
  `a_401_is_not_retried_even_when_a_success_is_waiting`.

## Host refusal before any connection (task 10.13, cited by 10.15)

The stub of the end-to-end tests cannot count a connection to a foreign host,
because the foreign host is not the stub. The older test
`tests_e2e_responses.rs::a_foreign_host_is_rt_ai_0013_before_any_connection`
asserts `stub.connections() == 0`, which would also hold if the run connected
to the foreign host. It does fail in practice, since the runtime guard in
`hosted::responses_adapter` panics for a non-loopback endpoint in a test, but
that is an indirect proof. The direct proof is
`tests_host_before_connection.rs`: a `Providers` implementation counts every
request for an adapter or a catalogue and builds none. For six foreign
spellings the run ends with the exact code (RT-AI-0013, RT-AI-0004 or
RT-AI-0003) and exit 2, with zero adapter and catalogue requests, no
Transmission_Log entry and no canary, in `propose` and in `models`. A control
test shows that the official origin does reach the provider factory. No
adapter means no transport, so no connection can have been made.

## Re-read of the Meta documentation, 2026-11-16

Read on 2026-11-16 (`date -u`), the same day as the earlier reads, so a
change since those reads is unlikely and none was found. "Same" means the fact
in the profile matches the page. Pages are summarised, not quoted.

| Fact in the profile | Page (full URL) | Result |
| --- | --- | --- |
| Strict JSON Schema through `text.format` on Responses | https://dev.meta.ai/docs/structured-output, https://dev.meta.ai/docs/protocols/responses | Same. The page says `strict` defaults to false, and the request sets it to true explicitly. |
| Nesting depth 10 | https://dev.meta.ai/docs/structured-output | Same. |
| Total properties 5,000 | https://dev.meta.ai/docs/structured-output | Same. |
| Recursive (`$ref` cycle) schema rejected | https://dev.meta.ai/docs/structured-output | Same. |
| Other schema limits: 120,000 characters of names and enum values, 1,000 enum values, a single enum over 250 values capped at 15,000 characters, 200,000 nodes after inlining | https://dev.meta.ai/docs/structured-output | Documented, not in the profile constants. `tests_proposal_schema.rs` already measures the generated schema against the first two (`MAX_NAME_CHARS`, `MAX_ENUM_VALUES`); the largest enum is 6 values and the schema has no `$ref`, so the other two cannot be reached. |
| Strict subset: plain object root, no `allOf` or `oneOf`, `anyOf` below the root only, `additionalProperties: false` on every object, `required` lists every property | https://dev.meta.ai/docs/structured-output | Same as the rules the schema tests check (`forbidden_combinators`, `open_objects`, `required_mismatches`, `nested_root_any_of`). The page also forbids a root `enum` and a root `not`, and validates the subset inside `$defs` and `definitions` as well. The schema tests do not check a root `enum` or `not` (the root test accepts any root with `type: object`), and they reject every `$ref` and `$defs`, so the `$defs` rule cannot be reached. The generated schema is a plain object with neither. A violation returns HTTP 400 only with `strict: true`. |
| Effort values `minimal`, `low`, `medium`, `high`, `xhigh`, `max`; `none` refused with 400 | https://dev.meta.ai/docs/reasoning, https://dev.meta.ai/docs/protocols/responses | Same. Both pages list `none` among the accepted names and then refuse it for the three reasoning models. |
| `max` only for `muse-spark-1.3`, `muse-ember-1.0`, `muse-wonder-1.0` | https://dev.meta.ai/docs/reasoning, https://dev.meta.ai/docs/protocols/responses | Same. Not available with `muse-spark-1.3-contributor`. |
| Default effort when omitted | https://dev.meta.ai/docs/reasoning | New, not in the profile: `max` on Wonder and Ember, `high` on every Spark version. RuleTrace sends an effort only when configured. |
| `store` default and `store: false` | https://dev.meta.ai/docs/protocols/responses | Same. The default is `true`; `store: false` with stateless replay keeps no server copy. |
| Usage members `input_tokens`, `output_tokens`, `total_tokens` | https://dev.meta.ai/docs/protocols/responses | Same. `output_tokens_details.reasoning_tokens` appears on retrieved responses only. |
| Non-streaming time limit, HTTP 504 `gateway_timeout`, final | https://dev.meta.ai/docs/protocols/responses, https://dev.meta.ai/docs/error-handling | Same. The error page also says not to retry a 504 unchanged, which agrees with Req 29.2 as decided on OD-37-1. |
| Status mapping 400, 402, 404 final; 401, 403 authentication; 429 rate limit; 500 and 503 retryable | https://dev.meta.ai/docs/error-handling | Same. The page says not to retry 400, 401, 402, 403, 404 and 413. |
| 503 `service_overloaded` and `backend_unavailable` carry `Retry-After: 60` | https://dev.meta.ai/docs/error-handling | Same. `server_shutting_down` carries a shorter window of a few seconds. The page asks clients to honour the header. Req 29.3 treats it as absent and keeps the 2 s and 8 s schedule, a deliberate design decision that retries earlier than the page asks. |
| 429 carries `Retry-After` | https://dev.meta.ai/docs/error-handling | Same. |
| Error body shape | https://dev.meta.ai/docs/error-handling | The page puts the category in `type` (`authentication_error` for 401, `rate_limit_error` for 429, `billing_error` for 402, `server_error` for 5xx) and the machine code in `code` (`invalid_api_key`, `rate_limit_exceeded`, `service_overloaded`, `backend_unavailable`, `gateway_timeout`). The older stub helper in `stub_http.rs::meta_error_shapes_map_by_status` writes the code into `type`. The bodies are never read, so no result depends on it, and the new tests add the documented shape. The page also says to branch on status and `type`; RuleTrace branches on status only, which is stricter. |
| Base URL `https://api.meta.ai/v1`, Bearer authentication | https://dev.meta.ai/docs/structured-output, https://dev.meta.ai/docs/protocols/responses, https://dev.meta.ai/docs/authentication | Same. Seen in the code samples of three pages, which replaces the earlier search snippet. The profile constant keeps its old `read_on` text. The key prefix `LLM_` is documented and not used by any check. |
| Model listing `GET /v1/models` | https://dev.meta.ai/docs/models | Now documented (the profile relied on an HTTP 401 probe). The page says the list is newest first with a `created` member. `models` sorts by identifier bytes on purpose (Req 35.18). |
| Context window | https://dev.meta.ai/docs/models | Changed from unknown for two models: 1,048,576 tokens for `muse-wonder-1.0` and the Spark versions. The Ember row has no value. The output limit is not stated as a number. The profile keeps `Limit::Unknown` because it holds one value for every model, and no code reads it. |
| `max_output_tokens` minimum 16, shared budget with input | https://dev.meta.ai/docs/protocols/responses | Same. Reasoning tokens count toward it. |
| Rate limits | https://dev.meta.ai/docs/pricing-rate-limits | New, not in the profile: 3,000 requests and 4,000,000 tokens per minute per team for the Standard models, 150 requests per minute for the Contributor variants. Not used. |

Findings: no profile constant is wrong, and no constant, source or test
expectation was changed by this re-read. The two proposals, neither applied:
update the `read_on` text of the base-URL fact in `profile.rs`, and record
the 1,048,576-token context window per model if a later task needs it.
