# RuleTrace Technical Architecture

## Pipeline (separate responsibilities, typed contracts)
1. Ingestion: preserve source structure and spans.
2. Extraction: candidate clauses with source locations.
3. Rule IR: typed, versioned representation of rule revisions. Approval status (Proposed, Approved, Rejected) is derived from a separate Approval_Ledger and never stored in the Rulebook.
4. Semantic validation: types, missing values, conflicts, unsupported operations, incomplete interpretations.
5. Compilation: validated, approved IR to a deterministic executable representation.
6. Execution: compiled rules over competition data.
7. Explanation (provenance component): resolves, verifies and renders decision traces against verified source documents.

Each stage declares all of its typed inputs and typed outputs. A stage may consume any typed value its contract lists, not only the previous stage's output. Examples:
- Execution consumes a compiled rulebook, competition data, questions and an optional published result.
- Provenance resolution consumes a decision trace and pre-loaded, hash-verified source models.
- AI proposal generation consumes candidate clauses, source models, rulebook type declarations and an AI configuration.

No stage may depend on undocumented shared state, mutable globals, or implicit environmental inputs (environment variables, working directory, locale, clock, randomness, network). Data crosses stages only as documented typed values.

## Design rules
- Rule language is constrained and declarative, serialized as a versioned data format. No embedded scripting, no `eval`, no general-purpose language.
- IR nodes carry a `Location` (document id, span). Rules require at least one real source span; diagnostics may use document, input-file or CLI locations. Never fabricate a span. Compilation and execution must preserve locations.
- The engine does no file or network access and receives no source text. Source resolution, source hash verification and source text rendering live in the separate provenance component (explainer), which works only on pre-loaded source models. If a document or span cannot be resolved, the trace is marked not verified, a diagnostic is emitted, and no text is invented.
- Errors, warnings, ambiguities, and unsupported constructs are all `Diagnostic` values with a code, severity, message, and source location. Validity flags (`has_errors`, `no_errors`, validation status) are derived from the single diagnostic list and are never stored or accepted as independent fields.
- Numbers use an exact numeric type. Floats are banned from the IR, compiler, and engine. The representation (bounded decimal or bounded rational with decimal literals; user prefers rational) is chosen by comparing exactness, range and scale, rounding, overflow protection, canonical serialization, and dependency and performance cost. The decision is pending user confirmation and must be recorded and justified before the IR is finalized.
- Execution is a pure function of (compiled rulebook, input data). No clock, randomness, environment, network, or iteration-order dependence. Use ordered collections.
- A compiled rulebook owns all its state. Two rulebooks must not share mutable or global state.
- Rules have revisions. Approval records live in a separate append-only, canonical-serialized, hash-chained Approval_Ledger supplied to validation as an explicit typed input. Each record binds the rule content hash, the IR_Version and the hashes of all referenced source documents. Any change to content, source binding or IR_Version makes the record non-matching, and the revision is Proposed again. Manual and AI-origin rules use the same IR and the same validator. Origin is provenance metadata and never affects semantics or compiled output.
- Diagnostics: Warning never changes compilability. Error, Ambiguity and Unsupported are blocking. An unresolved approval state (Proposed revision awaiting a decision) is a Severity Error with its own code and blocks compilation. A rulebook with no executable approved rule produces an Error and no compiled rulebook. Compilation is atomic. Rejected revisions are retained and never block.
- Diagnostic codes have catalogue identifiers RT-DGN, RT-PRV, RT-APR and RT-AI with numbers, and every failure mode has exactly one code.
- Shared engine crate is application-neutral. Prize Check and Bracket Check are thin workflow layers over it.

## AI (optional, first-class, outside the deterministic core)
- Disabled by default. Enabled only by an explicit TOML configuration file named on the command line of the separate `ruletrace-ai` executable, with no auto-detection from the environment. No credentials, network or provider libraries are needed to build or run the core.
- Configuration names the credential only by environment variable name. Inline credentials are rejected. Endpoint scheme is https except loopback in tests. Document text is sent only with a per-invocation consent naming the documents, and every send is logged without text or credentials.
- Call policy: 60 s total deadline, 20 s per attempt, at most 2 retries, 1 MiB response limit. Retry only transient failures (timeout, connection failure, HTTP 429 with Retry-After in seconds, HTTP 5xx) on a fixed 2 s and 8 s schedule without jitter. The request identifier is the SHA-256 of the request bytes, one response from one attempt is parsed, proposal identifiers derive from request hash and index, and batch storage is atomic and idempotent.
- First real provider: OpenAI Responses API with structured JSON Schema output, in its own integration crate. The response is still untrusted.
- A provider-independent adapter trait lives in an auxiliary crate outside the core. Real provider integrations live in separate integration crates. A deterministic mock provider is used for automated tests.
- The adapter returns untrusted bytes. They are parsed against a strict proposal schema, every source span and quote is verified against the ingested documents, and each proposed rule goes through the same validator as manual rules.
- A human reviewer approves or rejects through the `review` command group of `ruletrace`, which is the only code path that appends to the ledger. AI never approves its own suggestions, never compiles, and never executes. No AI crate may depend on the compiler, the engine, the approval-transition crate or the ledger-append crate.
- Provider failures, timeouts, rate limits, retry exhaustion, unavailability and malformed output become diagnostics with codes. Protocol failures (transport, schema, provenance) reject the whole batch. Semantic outcomes of well-formed proposals are stored as Proposed, non-executable revisions with their diagnostics. A failed `ruletrace-ai` run leaves the rulebook, ledger and proposal store unchanged. The core keeps working.

## Structure
- Cargo workspace of small crates with explicit boundaries (for example: source, ir, diagnostics, numeric, ledger, validate, compile, engine, trace, explain, review, cli in the core; ai-adapter, ai-mock, ai-integration-*, ai-tool outside it).
- Two executables. `ruletrace`: ingest, validate, compile, evaluate and `review` (list, approve, reject). `ruletrace-ai`: propose. The `ruletrace` dependency tree contains no network or provider crate, and a test enforces it.
- Exit codes: `ruletrace` 0, 2, 3, 4, 70. `ruletrace-ai` 0, 2, 3, 5, 6, 70. The earliest failing stage decides the code (config load, credential and consent, input load, request, provider call, response parse, schema, provenance, store).
- The crate dependency graph is acyclic, documented, and checked in CI against a stored list of allowed edges. Core crates never depend on auxiliary crates, network clients or AI libraries.
- No microservices, databases, or background infrastructure.
- Add a dependency only with a written justification. Prefer the standard library.
