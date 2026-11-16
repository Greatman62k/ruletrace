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
| Gateway timeout | A non-streaming request that runs longer than a server-side time limit returns HTTP 504 (`gateway_timeout`). Streaming requests are exempt. RuleTrace does not stream, so a long reasoning generation can end in 504. A 504 is a final failure, never retried (Req 29.2; OD-37-1 resolved by the user on 2026-11-16). | https://dev.meta.ai/docs/protocols/responses | 2026-11-16 |

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
(`INPUT_PREAMBLE`, eleven lines, each a rule that the tool enforces and that a
test in `rt-ai-tool` asserts) followed by the canonical request bytes that
passed the credential scan. The schema carries `description` members under the
same rule. The wording is the prompt profile `N` of the adapter version
`responses-1/prompt-N` (Req 35.23), which a stored batch records; it is not
part of the Request_Identifier. The credential travels only in the
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
