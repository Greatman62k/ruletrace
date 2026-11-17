# Kiro engineering tooling

How RuleTrace uses Kiro custom agents, hooks and skills for its own
development. None of it is needed to build, test or run RuleTrace. Every
statement below was observed on Kiro CLI 3.3.0 unless it is marked as not
verified.

## Phase A: custom agents

Files:

- `.kiro/agents/ruletrace-implementer.json`, prompt `.kiro/agents/prompts/implementer.md`
- `.kiro/agents/ruletrace-reviewer.json`, prompt `.kiro/agents/prompts/reviewer.md`

Both load the five steering documents through `resources`. The prompts add
only the task protocol and the report format; they do not restate steering.

Use them with `kiro-cli chat --v3 --agent ruletrace-reviewer ...` (the
headless default engine is v2, which cannot load v3-format agents), or by
name as a sub-agent.

### Verified behaviour

| Check | Result |
| --- | --- |
| `kiro-cli agent validate --path <file>` | Exit 0 for both |
| `kiro-cli agent list` | Both listed as Workspace agents |
| Sub-agent discovery | Both are callable by name as sub-agents and their rules apply there (denial text says `Source: subagent-profile`) |
| `deny` rules | Enforced in the CLI and in sub-agents. Tested: `git push`, `git commit`, `gh run rerun`, `cargo test` (bare), `fs_write` outside `docs/reviews/*.md` for the reviewer |
| `allow` rules | Enforced for shell prefixes and for workspace paths (`docs/reviews/*.md`) |
| `ask` rules, interactive CLI | Prompt the user. In `--no-interactive` mode an unanswered prompt fails the call, so the command does not run |
| `ask` rules, sub-agent | **Not enforced.** A sub-agent ran an `ask`-listed fixture write and an `ask`-listed shell command without a prompt |
| Unlisted shell commands, sub-agent | **Not blocked.** `echo x > file` ran in the main worktree |
| Write paths outside the workspace | Cannot be pre-allowed: an `allow` rule for `~/review-wt-*/**` or its absolute form had no effect, so such a write asks every time |
| `**` deny with `exclude` | Works: the root file was denied and `docs/reviews/*.md` was allowed |
| `*` and `**/*` patterns | Did not match a root-level file in the probe. Use `**` |

### What this means

Only `deny` is a reliable boundary in every mode. The reviewer's independence
therefore rests on three layers:

1. `deny` rules for commits, pushes, resets, checkouts, branch changes,
   GitHub write operations and bare `cargo test`/`cargo run`. A shell command
   that mentions the main worktree through `git -C <main>` is also denied.
2. The prompt, which forbids edits to the main worktree.
3. The integrity check of Phase B, which compares the main worktree and `HEAD`
   before and after a review. Shell redirection, `sed -i` or `python3` could
   still write to the main worktree inside a sub-agent. The check detects that
   afterwards. It does not prevent it.

Not enforced by the agent configuration alone:

- A review run as a sub-agent can write to the main worktree through a shell
  redirect. Only the Phase B check reveals it.
- `CARGO_TARGET_DIR=... cargo test` (a leading variable assignment) is not
  matched by the bare `cargo test*` deny. The Phase B shell hook parses the
  command and closes this.
- The golden/fixture `ask` rules protect an interactive session only. The
  Phase B hook is the enforcement.

### Report formats

Implementer: ten headings (requirements, files, tests, red/green, mutants,
commands with exit codes, changed expectations, deviations, open decisions,
not verified). Reviewer: nine headings (verdict, scope, blocking findings,
non-blocking findings, what held up, commands, mutants, not verified,
integrity). Both are in the prompt files.
