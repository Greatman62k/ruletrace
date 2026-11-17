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

## Phase B: hooks

Wiring: `.kiro/hooks/ruletrace.json` (CLI hook format `{"version":"v1","hooks":[...]}`).
Scripts and their tests: `tools/hooks/` (Python 3 standard library only; see
`tools/hooks/README.md`). Test suite: `python3 -B -m unittest discover -s tools/hooks/tests`
(108 tests) and the mutant runner `python3 -B tools/hooks/tests/mutants.py` (88 mutants).

| Hook | Event and matcher | Effect |
| --- | --- | --- |
| `guard-golden` | PreToolUse, write tools | Blocks writes to goldens, fixtures and recordings. The user approves the exact change (path, old hash, new hash, expiry, single use) with `tools/hooks/golden_approve.py <id>` on a TTY |
| `review-integrity` | PreToolUse, write tools | A report under `docs/reviews/*.md` can be written only while the main worktree and `HEAD` match the baseline recorded by `tools/hooks/review_integrity.py begin <id>` |
| `guard-shell` | PreToolUse, `execute_bash` | Blocks `git push` and destructive git, GitHub write verbs, credential exposure, direct provider calls, bless commands, and `cargo test|run|nextest|bench|mutants|llvm-cov` outside the exact wrapper `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh <cmd>` |
| `fmt-check` | PostToolUse, write tools | Runs `rustfmt --edition 2021 --check` on a written `.rs` file and records the result in `.git/ruletrace-hooks/fmt-warnings.json`. Never rewrites |
| `stop-report` | Stop | Prints the working-tree state, flags goldens, catalogue, architecture-edge and decision changes, runs the relevant `rt-xtask` checks and a read-only FORMAT check, and writes `.git/ruletrace-hooks/last-stop-report.txt` |

### Verified on Kiro CLI 3.3.0

| Check | Result |
| --- | --- |
| Hook payloads | `session_id`, `hook_event_name`, `cwd`, `tool_name`, `tool_input` (`fs_write`: `path` (absolute), `text`; `str_replace`: `path`, `oldStr`, `newStr`, `replace_all`; `execute_bash`: `command`, `cwd`, `description`, `timeout`, `run_in_background`); PostToolUse adds `tool_response`. The payload has no agent name |
| Working directory of a hook | The project root |
| PreToolUse exit 2 | Blocks the call. The hook's stderr text is shown to the agent. Verified in the CLI, for a sub-agent, and in a fresh session in this workspace |
| Wired hooks, fresh session | Protected fixture write blocked with a pending id; `echo $MODEL_API_KEY` blocked; `git status` allowed |
| Sub-agents | Workspace hooks apply to sub-agents (probe). A Stop event was not observed for sub-agent runs |
| PostToolUse and Stop output | Never reached the agent in headless mode, for exit codes 0, 1 and 2. The interactive TUI display is not verified. The scripts therefore also write their results under `.git/ruletrace-hooks/` |
| Hot loading | Not reliable. After `.kiro/hooks/` was deleted and recreated, a running session did not pick up the new file. Start a new session |
| `fs_append`, `delete_file` | Not present in the tool set that was probed; the scripts fail closed for protected paths if they appear |
| Approved wrapper | `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh cargo test -p rt-ai-responses --test stub_http` passes (21 tests). Without `ip link set lo up` 18 of 21 fail, so the exact wrapper is required |

### Limits

- Shell write detection (golden and bless rules) is pattern based. A
  `python3 -c` or `perl` one-liner that writes a golden file is not detected.
- The review integrity check is tamper evidence at the moment the report is
  written. It does not stop a write to the main worktree during the review.
- Nothing here is a security boundary against a determined agent. It makes
  accidents and shortcuts visible and, for the listed patterns, blocked.

### Incident, 2026-11-17

While testing, the orchestrator ran `git push origin HEAD` in a running session
to confirm that `guard-shell` blocked it. That session had not loaded the newly
created hook file (hot loading is unreliable), so the command executed and
pushed the local branch `m3` (commit `5eb7d1a`, which includes work that had
not been independently reviewed) to origin. `main` was not touched and no CI
run started (the workflow runs on pushes to `main` and on pull requests). The
user's rule was that nothing is pushed. A blocking test must never be a command
with an irreversible effect: use a harmless stand-in (a write to a protected
path in a throwaway clone) instead.

User decision (2026-11-17): leave the remote branch `m3` on origin for now; it is
cleaned up later. It is not a release or review point.

## Phase C: Context7 MCP experiment (2026-11-17)

Setup checked first: the hosted server `https://mcp.context7.com/mcp` answered an
unauthenticated MCP `initialize` (HTTP 200, server version 4.7.0), so no key or
paid account was needed. It was configured only in a throwaway clone
(`.kiro/settings/mcp.json` with `url` only, no headers) and started in a v3
session (`--require-mcp-startup`, tools `context7::resolve-library-id` and
`context7::query-docs`). Only public library names and API questions were sent.

Two real questions about RuleTrace dependencies were asked in fresh headless
sessions, once restricted to the Context7 tools and once with the built-in web
tools only:

| Question | With Context7 | Web tools only | Checked against |
| --- | --- | --- | --- |
| ureq 3.5.x `ConfigBuilder`: no redirects, status not an error, no proxy from the environment, one overall timeout | Correct method names and types (`max_redirects(u32)`, `http_status_as_error(bool)`, `proxy(Option<Proxy>)`, `timeout_global(Option<Duration>)`). Context7 served docs.rs `latest` only and listed no 3.5.x version | Same four answers, read from the docs.rs 3.5.0 page and source; gave the version | The ureq 3.5.2 source in the local cargo registry and `crates/rt-ai-responses/src/transport.rs`: both correct; neither mentioned `max_redirects_will_error`, which the transport also sets |
| proptest 1.x: `cases` field and default, per-block 512, default failure persistence path, `PROPTEST_CASES` | Correct, with one extra caveat (an older book page conflicts) | Correct | The repository's `crates/rt-ai-tool/proptest-regressions/*.txt` layout matches both |

Session sizes (events): ureq 163 with Context7 against 193 without; proptest 258
against 160.

Result: no demonstrable benefit. Both routes gave correct answers to these
questions; the web route was better at pinning the exact version the project
uses, which matters for a pinned lockfile. Context7 is not retained and no MCP
server is configured for the project. The experiment can be repeated if a
dependency appears whose documentation is hard to reach through docs.rs.

## Phase D: workspace skills (2026-11-17)

Location verified against the Kiro docs (`kiro.dev/docs/skills.md`): workspace
skills live in `.kiro/skills/<name>/SKILL.md` (Agent Skills format, YAML front
matter with `name` and `description`), are discovered at session start, and
are loaded in full only when a request matches the description or the skill is
invoked as `/<name>`. Custom agents do not load skills unless they list them;
both RuleTrace agents now have `skill://.kiro/skills/*/SKILL.md` in
`resources`.

| Skill | Purpose |
| --- | --- |
| `ruletrace-verify-gates` | The approved wrapper, scoped checks and the full gate list in CI order |
| `ruletrace-catalogue-change` | Catalogue entry, `gen-catalogue`, requirement and design tables, `check-catalogue`, test |
| `ruletrace-golden-change` | Reason, old and new bytes, the `golden_approve.py` flow, recording rule |
| `ruletrace-independent-review` | Baseline with `review_integrity.py begin`, reviewer brief without the implementer's conclusions, after-review checks |
| `ruletrace-ci-inspection` | Read-only `gh` commands and the cancellation diagnosis steps |

The skills hold procedures only; the rules stay in the steering documents.

Verified: in a fresh v3 session of a clone, the default agent and the
`ruletrace-reviewer` agent both list the five skills; the reviewer also has the
steering documents and its prompt in context. Not yet verified: a skill
activating automatically from a matching request during real work. Use is to
be recorded in the usage log when it happens.
