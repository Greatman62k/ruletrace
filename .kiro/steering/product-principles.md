# RuleTrace — Product Principles

## 1. Product Identity

RuleTrace is a competition rulebook compiler with optional AI-assisted rule interpretation.

The product transforms competition regulations into structured, typed, versioned and executable rules. It evaluates competition data against approved rules and produces reproducible decisions with inspectable explanations and references to original source clauses.

The compiler and deterministic execution engine are the foundation of the product. AI assists with understanding and authoring rules; it does not replace the compiler.

RuleTrace must be useful without AI and more convenient to use with AI.

We are not building a generic chatbot, document summarizer, dashboard product or hosted AI wrapper.

## 2. Core Product Principles

### 2.1 Deterministic foundation

- The core compiler and Rules_Engine must work independently of AI.
- Identical approved rules and identical input data must always produce identical outcomes and decision traces.
- AI output must never participate directly in authoritative rule execution.
- Compilation and execution must not depend on network access, wall-clock time, random values or environment state.

### 2.2 Provenance and traceability

- Every executable rule must retain provenance to its original source document and source span.
- Every decision must be traceable to the rules, input values and calculations that produced it.
- Source text must be verifiable against the original document and its content hash.
- No source location may be fabricated or presented as verified when it cannot be resolved.

### 2.3 Explicit uncertainty

- Ambiguous, incomplete, unsupported and conflicting rules must be represented explicitly.
- RuleTrace must never guess an interpretation to make compilation succeed.
- Missing information must never silently become false, zero, an empty value or success.
- When missing information prevents a definitive decision, the result must be Indeterminate.
- Boolean expressions must follow documented three-valued logic.
- A conflict may only be resolved by an explicit, approved precedence declaration. Input order and implementation convenience must never resolve conflicts.

### 2.4 Exact computation

- Money and score calculations must use exact decimal or rational arithmetic.
- Floating-point arithmetic is prohibited in Rule_IR, compilation and execution paths.
- Rounding must be explicitly declared by the applicable rule.
- The numeric representation is chosen by comparing exact decimal, rational and hybrid options on exactness, range and scale, rounding, overflow protection, canonical serialization, and dependency and performance cost, not for convenience. The choice (bounded decimal or bounded rational with decimal literals) is pending user confirmation; requirements are written to hold under both.
- Overflow, division by zero and unsupported numeric operations must be reported rather than silently approximated.

### 2.5 Security

- All documents, Rule_IR files and competition data are untrusted input.
- RuleTrace must never execute scripts, shell commands or arbitrary code extracted from documents.
- The rule language must remain constrained, typed and declarative.
- The compiler must reject unsupported constructs explicitly.

### 2.6 Approval gate

- Approval is derived from a separate, append-only, hash-chained Approval_Ledger. A rule's Approval_Status is never stored in the Rulebook.
- An approval binds the exact rule content, the IR_Version and the hashes of every referenced source document. Any change invalidates it, and the revision returns to Proposed.
- Warnings never change compilability. Errors, ambiguities and unsupported constructs are blocking. A rule revision awaiting a decision (unresolved approval state) is a blocking Error with its own code.
- A rulebook with no executable approved rule does not compile. Rejected revisions stay auditable and never block.

## 3. AI Is Optional but First-Class

AI is part of the initial architecture and development plan. It is not a future architectural afterthought.

The AI subsystem must be independently deployable, replaceable and optional.

When enabled, it may assist with:

- Identifying meaningful clauses in competition regulations.
- Interpreting natural-language rule statements.
- Suggesting structured Rule_IR representations.
- Identifying relationships and dependencies between clauses.
- Highlighting possible rule conflicts.
- Identifying ambiguous or incomplete interpretations.
- Generating candidate test scenarios.

### 3.0 Two executables

- `ruletrace` is deterministic. It has the four pipeline commands (ingest, validate, compile, evaluate) and the `review` command group (list, approve, reject). It never touches the network or a provider library.
- `ruletrace-ai` is separate and optional. It writes only the Proposal_Store and a transmission log. It can never write the Approval_Ledger or call an approval function, enforced by crate dependency.
- Human approval is an explicit operation of the main workflow (`ruletrace review`).

### 3.1 AI proposal pipeline

The intended flow is:

1. Ingest and structurally preserve the source document.
2. Identify candidate clauses and their original source locations.
3. Prepare relevant source context for an AI task.
4. Send the task through an optional, provider-independent AI interface.
5. Receive structured candidate Proposals.
6. Parse and validate the proposed output.
7. Verify that every proposed rule and interpretation refers to real source spans.
8. Pass proposals through deterministic semantic validation.
9. Present proposals for human review.
10. Record explicit human approval or rejection through `ruletrace review` in the Approval_Ledger.
11. Allow only approved, validated rules to enter compilation.

AI-generated text is never directly executable. AI never compiles or executes rules.

Manual authoring and AI-assisted interpretation feed the same Rule_IR and the same validation pipeline. A rule's origin (manual or AI proposal) is provenance metadata and never changes its semantics or its compiled form.

### 3.2 AI trust boundary

- AI-generated Proposals are untrusted.
- AI is disabled by default and enabled only by explicit configuration.
- A Proposal must never automatically become an Approved Rule.
- AI must not approve its own suggestions. An AI identity can never be a Reviewer, and no AI crate can call the approval function.
- Approval is an explicit human act recorded in an Approval_Record bound to the hash of the exact rule content.
- AI must not modify an Approved Rule in place.
- Changes to approved rules must create new proposed revisions, and any content change invalidates earlier approval records.
- Rejected is terminal for a revision.
- Protocol failures (transport, schema, provenance) reject the complete batch with a specific diagnostic code. Well-formed proposals that the validator finds ambiguous, unsupported or invalid are stored as Proposed, non-executable revisions carrying those diagnostics.
- An AI provider failure, timeout, rate limit or outage must not compromise or disable the deterministic compiler.
- Model-generated explanations are not substitutes for verifiable compiler diagnostics or decision traces.

### 3.3 Provider independence

- The AI subsystem must expose a provider-independent interface.
- Provider-specific implementations must live outside the core compiler crates, in separate integration crates.
- AI functionality must be explicitly enabled and configured, and is disabled by default.
- The core must never require AI credentials, network access or provider libraries.
- A hosted AI provider may require credentials only when that provider is explicitly configured. Credentials never appear in source, logs or stored artifacts.
- No document text is sent without an explicit TOML configuration and a per-invocation consent that names the documents sent. Each transmission is recorded in a log without document text or credentials.
- Credentials are referenced only by environment variable name. A configuration with an inline credential is rejected. Endpoints use https, except loopback in tests.
- Provider calls use a fixed 60-second total deadline, 20-second attempt timeout and at most two retries on transient failures only, with a deterministic backoff and idempotent, atomic batch storage.
- Provider, model and proposal-generation metadata should be recorded for auditability without contaminating deterministic execution artifacts.

The first version contains a real, testable AI proposal workflow: a provider-independent adapter, a real integration for the OpenAI Responses API in its own crate (structured JSON Schema outputs, provider and model set by configuration), and a deterministic mock provider for automated tests. The initial demonstration uses public competition documents.

## 4. One Compiler, Multiple Applications

Prize Check and Bracket Check are separate future applications powered by one shared Rules_Engine.

Prize Check will focus on competition eligibility, scoring, rankings and tie-breaking.

Bracket Check will focus on standings, qualification, rankings and tie-breaking, with more advanced progression features introduced only when supported.

Application-specific workflow, input mapping and presentation must not be duplicated inside the shared engine.

A capability not supported by the Rule_IR must be rejected rather than implemented as a separate hidden application-specific rules engine.

## 5. Architecture Principles

- Use Rust for the compiler, typed Rule_IR, validation and deterministic execution.
- Maintain clear boundaries between ingestion, extraction, rule representation, validation, compilation, execution, provenance and AI assistance.
- Use typed inputs and outputs for public pipeline interfaces. Each stage declares all of its typed inputs and outputs, with no undocumented shared state, mutable globals or implicit environmental inputs.
- Keep the crate dependency graph acyclic and verified automatically.
- Keep source resolution, hash verification and text rendering in the provenance component, separate from the execution engine, which does no file or network access.
- Treat the Rule_IR as a versioned language with a precisely documented support boundary.
- Keep compilation atomic: a Rulebook with blocking diagnostics must not produce a partially executable Compiled_Rulebook.
- Use immutable, independently owned compiled artifacts.
- Design for reproducible builds, deterministic serialization and regression testing.
- Avoid unnecessary microservices, databases, background services and infrastructure.
- Keep provider-specific AI dependencies outside core crates.

## 6. Scope Discipline

- Do not claim universal rulebook support.
- Support only constructs explicitly documented in the current version of the Rule_IR.
- Reject unsupported functionality with useful diagnostics.
- Keep unresolved technical decisions documented rather than silently assuming answers.
- Do not introduce features solely to make the product appear more sophisticated.
- Prefer a small, correct and auditable implementation over broad but unreliable support.
- Do not build dashboards, chatbots or hosted services as substitutes for the compiler.

## 7. Definition of a Successful RuleTrace Decision

A successful evaluation must allow an independent reviewer to establish:

1. Which approved rules were applied.
2. Which source clauses justify those rules.
3. Which input values were read.
4. Which calculations were performed.
5. Which tie-breaking or precedence policies were applied.
6. Whether information was missing or a decision was indeterminate.
7. Why the final result was produced.

The reviewer must be able to inspect the decision without trusting an AI-generated assertion.

## 8. Product Goal

RuleTrace aims to make competition decisions explainable, reproducible and independently verifiable.

Its defining principle is:

**AI helps understand the rules. RuleTrace verifies and executes them.**

Every decision must be backed by approved rules, exact computation and traceable evidence.