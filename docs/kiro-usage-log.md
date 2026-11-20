# Kiro usage log

A factual record of how Kiro was used on RuleTrace. Entries state what was used,
what it supported, and where the evidence is. Nothing here is estimated or
embellished. Items marked "to confirm" are unknown to the project agent and
need the user's statement.

The final Kiro usage narrative is written after M4 from this log.

## Surfaces

| Surface | Used | Evidence |
| --- | --- | --- |
| Kiro CLI | Yes. The development sessions ran in Kiro CLI | Session records; git history from 2026-11-14 |
| Kiro IDE, Web, Crew | To confirm with the user | None known to the agent |

## Capabilities actually used

| Capability | What it supported | Evidence |
| --- | --- | --- |
| Spec-driven development | Requirements, design, task list and traceability for the compiler, written before implementation and amended by recorded decisions | `.kiro/specs/ruletrace-compiler/{requirements,design,tasks,traceability}.md`, `tasks.meta.json` |
| Steering documents | Five workspace rules applied to every task: product principles, technical architecture, Rust engineering and testing, Rust formatting and comments, testing integrity and anti-gaming | `.kiro/steering/*.md` |
| Decision records | User decisions captured as numbered records (for example UD-33, UD-34, UD-35, UD-37) and linked from the spec | `tools/decisions/*.md` |
| Sub-agents | Implementation work and independent reviews delegated to separate agents with fresh context. Reviewers ran in separate git worktrees and wrote review reports | `docs/reviews/*.md` |
| Property-based testing | Invariants checked with `proptest` (determinism, round trips, ordering, configuration checks, endpoint agreement) with at least 256 cases where generation is real | Property tests under `crates/*/src/tests_*.rs` and `crates/*/tests/` |
| Mutation checks by hand | Controlled mutants applied and reverted to show that new tests detect the behaviour they claim | Commit messages and review reports |
| Custom agents (Kiro CLI 3.3.0, v3 format) | `ruletrace-implementer` and `ruletrace-reviewer`, defined in `.kiro/agents/`. Validated with `kiro-cli agent validate`, listed by `kiro-cli agent list`, called by name as sub-agents. Permission boundaries probed in the CLI and as sub-agents: `deny` rules hold in both; `ask` rules and unlisted shell commands are not enforced for sub-agents | `.kiro/agents/`, `docs/kiro-tooling.md` (table of probes) |
| Agent hooks (Kiro CLI 3.3.0, `.kiro/hooks/ruletrace.json`) | Five PreToolUse, PostToolUse and Stop hooks guard goldens, shell commands, review-report integrity, formatting and end-of-session reporting. Real-CLI probes established the payloads and the blocking behaviour; PostToolUse and Stop output does not reach the agent in headless mode | `.kiro/hooks/`, `tools/hooks/` (108 tests, 88 mutants), `docs/kiro-tooling.md` |
| Workspace skills (`.kiro/skills/`) | Five procedure skills for gates, catalogue changes, golden changes, independent review and CI inspection. Discovery verified in the default and the reviewer agent | `.kiro/skills/*/SKILL.md`, `docs/kiro-tooling.md` (Phase D). Activation in real work: not yet recorded |
| Task orchestration | Tasks moved through `[ ]`, `[-]`, `[x]` with evidence notes | `tasks.md` |

## Not used (so far)

- MCP servers: none configured for the project. Context7 was trialled on 2026-11-17 in a throwaway clone and not retained (no demonstrable benefit; see `docs/kiro-tooling.md`, Phase C).
- Kiro powers: none. Packaging the skills as a Power is deferred until the skills show repeated value (user decision, 2026-11-17).

Adding any of these happens only where it gives a demonstrable engineering
benefit, with the user's approval.

## Impact evidence to collect as work proceeds

Record, with a pointer, each case where a Kiro capability changed an outcome.
Examples already in the repository (verify before quoting):

- Independent review findings that were real defects and were fixed:
  `docs/reviews/` (several reports returned NEEDS_CHANGES with a blocking
  finding that the implementing agent had not found).
- Property tests and mutants that exposed a surviving mutant or a gap:
  review reports for waves 6 to 11.
- Steering rules that blocked a shortcut (for example golden files changed
  only by explicit, reviewed edit): commit messages on golden changes.

## Log

| Date | Entry |
| --- | --- |
| 2026-11-16 | Log created. Sequencing decision recorded in `tasks.md`. |
| 2026-11-20 | M2 closure follow-up: the `ruletrace-reviewer` review of `5644e52` found the build guard untested; the orchestrator added a counting test with six killed mutants (`c6da862`), reviewed by `ruletrace-reviewer`. The user ran the cherry-pick, push of `ci/once-guard` and dispatch; the orchestrator inspected run `44711229145` read-only. The first log search returned nothing because `gh` refused output with terminal escape sequences; it was repeated with `--allow-escape-sequences` before any result was reported. |
| 2026-11-18 | 9.8 APP-API: `ruletrace-implementer` built the rule, `ruletrace-reviewer` ran three reviews (main, focused F1, re-review). The focused review found a gap the orchestrator's own F1 fix missed (keyword before a leading `::`). Integrity `verify` was run and each report committed before the next code change, as the 9.11 incident note requires. |
| 2026-11-18 | Process error: the orchestrator ran the review-integrity `verify` for review 9.11 in parallel with the implementer that fixed that review's findings. `verify` reported the implementer's two test files as changed, and the check did not hold as evidence. Recorded in the 9.11 task note; the fix commit gets its own review. Rule for later work: `verify` and the report commit come before any implementer starts on the same worktree. |
| 2026-11-18 | UD-39 decisions applied to the specification and reviewed (`2026-11-18-review-doc-66ef6ec.md`, `...-ud39-6c85779.md`, both APPROVED WITH FINDINGS). Tasks 9.5 and 9.6 implemented by the `ruletrace-implementer` agent and reviewed by `ruletrace-reviewer` (`...-9.5-bfa1120.md`, `...-9.5-followup-8ef8e8d.md`, `...-9.6-cf9c12b.md`, all APPROVED WITH FINDINGS). The 9.5 review found duplicated exit-class logic between `rt-cli` and `prize-check`, fixed by sharing one function; the 9.6 review found duplicated glue between the two Workflow_Modules, now RN-52 with a drift guard. A permission rule blocked the agent from deleting build caches when the disk reached 97%. |
| 2026-11-18 | Laboratory design phase (UD-38). Three `context-gatherer` sub-agents collected read-only evidence in parallel (the 34.9/34.10 source review, the credential and review-lock code paths, fixture sizes). The `ruletrace-reviewer` agent reviewed the L1 threat model and design §11 three times: `2026-11-18-review-L1-threat-model-d0c821c.md` returned CHANGES REQUIRED with three blocking findings (an over-claimed OS-level credential isolation for mechanism M5, a session-end request blocked by the step mutex, `rt-lab` edges outside Req 24.13 with no requirements note); all were fixed and re-reviewed (`...-L1-fix-6f7e851.md`, APPROVED WITH FINDINGS). The task plan was reviewed separately (`...-tasks-a4628bd.md`, APPROVED WITH FINDINGS). The follow-up fix commit `66ef6ec` was reviewed later in `...-doc-66ef6ec.md` (APPROVED WITH FINDINGS). The reviewer ran no cargo command for these document reviews; the repository hook blocked one command that would have read a process environment. |
| 2026-11-17 | First real review by the `ruletrace-reviewer` custom agent (task 9.2, `docs/reviews/2026-11-17-review-9.2-4a3bc77.md`): APPROVED with three surviving mutants that showed test gaps. The `review_integrity.py` baseline was recorded before the review and verified after it (main worktree and HEAD unchanged; scratch worktree removed). The agent's `git tag` deny rule stopped it from listing tags, which it reported as a gap. The Stop hook wrote `.git/ruletrace-hooks/last-stop-report.txt`, confirming the hook fires in this workspace. |
| 2026-11-17 | Phase D: five workspace skills created and loaded by both custom agents (discovery verified). |
| 2026-11-17 | Phase C: Context7 MCP trialled with two real dependency questions against the built-in web tools; equal accuracy, weaker version pinning; not retained. |
| 2026-11-17 | Phase B: hook scripts built by the `ruletrace-implementer` agent (first real use of the custom agent) and wired. Hooks verified in a fresh Kiro CLI session in this workspace. During verification an unguarded `git push origin HEAD` was run by the orchestrator in a session where the hooks were not yet loaded: it published the local branch `m3` (5eb7d1a) to origin. See the incident note in `docs/kiro-tooling.md`. |
| 2026-11-17 | Phase A of the Kiro tooling: two custom agents created, validated and probed. Findings and limits in `docs/kiro-tooling.md`. The reviewer has not yet been used for a real review. |
