# Rust Engineering and Testing

## Code
- Stable Rust, pinned via `rust-toolchain.toml`. Edition 2021 or later.
- `#![forbid(unsafe_code)]` in all crates unless a documented exception exists.
- No `unwrap`/`expect`/`panic!` in library code paths reachable from input. Return typed errors or `Diagnostic`s.
- Model states with enums and newtypes. Make invalid states unrepresentable.
- Public APIs are small and documented. Keep crate boundaries clean; no cyclic dependencies.
- Use `BTreeMap`/`BTreeSet` or sorted `Vec` where order can affect output.
- Exact arithmetic only (decimal or rational). Checked operations; overflow is a diagnostic, not a wrap.
- Core crates have no mutable globals, no `static mut`, no thread-local or lazily initialized global state, and no environment, clock, random, file or network access. Only the CLI binary, the AI and review tools, and the AI integration crates do I/O.
- Credentials are read only in an AI integration crate, from the environment variable whose name the explicit TOML configuration gives, and are redacted from every output. An inline credential in the configuration is an error.
- The retry decision is a pure function of attempt outcomes and elapsed monotonic time (no jitter, no randomness) so a simulated timer can test it. Only the integration crate uses a monotonic timer.

## Quality gates (run locally and in CI)
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- Dependency check (`cargo deny` or `cargo audit`) once dependencies exist.
- Dependency-graph check: acyclic, only allowed edges, no network, AI or auxiliary-crate dependency in any core crate.
- Core-only build and test with all auxiliary (AI) crates excluded.
- Resolved dependency tree of the `ruletrace` binary has no network client, AI provider or AI crate.

## Testing
- Unit tests for parsing, validation, compilation, execution, explanation, and each step of the AI proposal pipeline.
- Property-based tests (for example `proptest`) for invariants: determinism, serialization round-trip, exact arithmetic (differential test against an independent arbitrary-precision reference model), ranking totality and stability, trace completeness, rule content hash stability and invalidation on any content change.
- Negative tests for invalid, ambiguous, conflicting, and unsupported rulebooks. Assert the exact diagnostic code and source location.
- AI pipeline tests use the deterministic mock provider for every success and failure mode (malformed output, schema violation, bad span, quote mismatch, timeout, unavailable, rate limit, self-approval attempt, prompt injection). The real integration crate is tested against a loopback stub server. Live provider tests are opt-in and never required for CI to pass. A canary credential must never appear in any output.
- Approval tests: ledger round trip and hash-chain tamper detection, derived status from the ledger only, invalidation on any change of content, source binding or IR_Version, unresolved approval state blocks compile, no executable rule blocks compile, rejected revisions never block, review refusals (self-approval, stale hash, blocking diagnostic), and no code path outside `ruletrace review` can approve.
- Provider call tests with a simulated timer and the mock provider: each transient and non-transient failure, retry exhaustion versus deadline, Retry-After, late responses never stored, one stored batch after retries, repeated and concurrent runs idempotent.
- Configuration tests: inline secret and key-field rejection, insecure endpoint, missing consent, missing env var, redaction with a canary credential, no transmission on any rejection.
- Numeric tests run against both candidate representations while the numeric decision is pending.
- Dependency-graph and source-scan tests fail the build on cycles, forbidden edges, network or AI dependencies in core crates, mutable globals, and environment, clock, file or network access in core crates.
- Regression fixtures under `tests/fixtures/`. Include expected decisions and traces as golden files.
- Trace verification: every decision's trace must resolve to existing source spans through the provenance component.
- Isolation test: run two rulebooks in the same process (any order, interleaved) and assert neither affects the other's results.
- Core-only build test: build and run the core with all auxiliary crates excluded and assert byte-identical output.
- Each bug fix adds a regression test first.

## Workflow
- Small, reviewable changes. Update fixtures deliberately and explain why in the commit.
- Document each new dependency (purpose, alternatives, maintenance status) in the PR or commit message.
