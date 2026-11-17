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
| Task orchestration | Tasks moved through `[ ]`, `[-]`, `[x]` with evidence notes | `tasks.md` |

## Not used (so far)

- Agent hooks: none committed yet (`.kiro/hooks` does not exist).
- MCP servers: none configured for the project.
- Kiro powers: none.

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
| 2026-11-17 | Phase A of the Kiro tooling: two custom agents created, validated and probed. Findings and limits in `docs/kiro-tooling.md`. The reviewer has not yet been used for a real review. |
