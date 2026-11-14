---
decision: UD-2 and UD-9
title: Serialization format and size limits
status: Confirmed
confirmed_by: user
confirmed_on: 2026-11-14
chosen_format: RT-JSON (strict JSON subset, hand-written positioned parser)
chosen_limits: defaults of design section 4.1.5, unchanged
requirements: 4.2, 4.4, 19.3, 19.4, 19.9, 19.6
design: 4.1.1, 4.1.5, 9.2
---

# UD-2 and UD-9: serialization format and size limits

## Decision

The user accepted the recommendation of design section 4.1.1 and the default limits of section 4.1.5.

- **Format (UD-2): RT-JSON.** A strict subset of JSON (RFC 8259) read by a hand-written parser that records byte, line and column for every key and value. `serde_json`, TOML, RON and a custom text format are not chosen. TOML stays for the AI_Configuration only (UD-18).
- **Limits (UD-9): the defaults of section 4.1.5, with no change.**

## Format comparison (design section 4.1.1)

| Criterion | RT-JSON (strict JSON subset, hand-written parser) | serde_json | TOML | RON | Custom text |
| --- | --- | --- | --- | --- | --- |
| Positions (byte, line, column) for every value and key | Yes, by construction | Errors only | Via toml_edit | Errors only | Yes |
| Duplicate-key and unknown-field control | Yes | Duplicate keys need extra work | Partial | Partial | Yes |
| Depth limit before building values | Yes (counter in parser) | `recursion_limit` yes | Partial | No | Yes |
| No float coercion of numbers | Yes: numbers are strings | Risk with `Number` | Native floats | Native floats | Yes |
| Canonical output | One writer, fixed key order | Needs custom serializer | Hard | Hard | Yes |
| Tooling and diff friendliness | Good | Good | Good | Poor | Poor |
| Dependency cost | None, about 600 lines | serde and serde_json (about 6 crates, proc macros) | toml_edit | ron and serde | None, new grammar to design |
| Third-party review surface | Own code, differential-tested against `serde_json` as a dev-only oracle | Large | Medium | Medium | Own code |

### Reasons for RT-JSON

The requirements need exact locations for syntax errors, unknown names and literals (Req 4.2 to 4.4), duplicate and depth control (19.4, 23.5), and byte-identical output (4.5). RT-JSON meets all of them with no third-party dependency.

### Accepted cost

The parser and the typed decoders are written and tested in-house (about 600 lines plus decoders). The parser is differential-tested against `serde_json`, which is a dev-only dependency and never part of a shipped crate.

### RT-JSON dialect (design section 4.1.1)

- Standard JSON (RFC 8259) with these restrictions: UTF-8 only, no BOM, no comments, no duplicate keys, nesting depth at most the effective limit.
- Numbers appear only as non-negative integers up to 9,007,199,254,740,991 in integer fields (offsets, counts, revisions, versions).
- Strings have valid escapes and no unpaired surrogates. No unescaped control characters.
- A bare JSON number at a position that expects an Exact_Number is an error (RT-IR-0004, the literal text is reported). Exact_Number values are JSON strings (`"0.6"`, `"16"`), parsed from the lexeme and never through a float.
- Canonical forms: compact (no whitespace, used for hashes) and pretty (two-space indent, trailing LF, used for files). Both use the same fixed per-object key order and the same string escaping.

## Limits (design section 4.1.5)

| Limit | Default | Where checked |
| --- | --- | --- |
| Source_Document size | 16,777,216 bytes (Req 1.9) | ingest |
| Source_Model, Rulebook, ledger file | 64 MiB, 8 MiB, 8 MiB | parse, before parsing |
| Competition_Data, Proposal batch file | 32 MiB, 16 MiB | parse |
| AI_Configuration, Mock script | 64 KiB, 4 MiB | load |
| JSON container nesting depth | 64 for Rulebook, Source_Model, data; 32 for provider responses | parser |
| Expression node depth | 16 | validator (`RT-LIM-0003`) |
| Rules plus revisions per Rulebook | 10,000 rules, 100,000 revisions | parser (`RT-LIM-0004`) |
| Spans per rule | 64 | parser |
| Participants, collection elements | 100,000; 100,000 per participant | engine input (`RT-LIM-0004`) |
| `--max-bytes` and `--max-depth` | override defaults in either direction | CLI |

A value equal to the limit is accepted. Only a strictly greater value is rejected (Req 19.9). Container depth 64 allows expression depth 16, because each node contributes at most 2 containers, plus rule and document wrapper depth below 16.

## Consequences

- `rt-json` is registered against UD-2 in task 2.1, and task 2.7 proceeds as written. No revision of task 2.7 is needed.
- Every parser of a serialized artifact uses RT-JSON and these limits.
- The design register (section 9.2) still lists UD-2 and UD-9 as Recommended-pending. This task does not edit the design. Updating those two rows to Resolved-by-user is a follow-up for the orchestrator or the next design review.
