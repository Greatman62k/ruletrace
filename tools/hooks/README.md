# Kiro hook scripts

Development tooling for Kiro CLI. Nothing here is needed to build, test or
run RuleTrace. Python 3 standard library only. The scripts read one JSON
object from stdin, run with the project root as working directory, and keep
their only state under the repository's `.git/` directory. Wiring them as
hooks is a separate step; this directory contains no hook configuration.

## Hook semantics (observed on Kiro CLI 3.3.0)

| Event | Exit 0 | Exit 2 | Other non-zero |
| --- | --- | --- | --- |
| PreToolUse | allow | block; stderr is shown to the agent as the reason | warning only |
| PostToolUse | ok | warning only (the tool already ran) | warning only |
| Stop | ok | not used: a non-zero Stop exit would start another turn | not used |

Workspace hooks apply to the top-level agent and to sub-agents. No Stop
event was observed for sub-agent runs. The field names of `str_replace`
(`oldStr`/`newStr`/`replace_all`) and `fs_append` were not probed. The
scripts accept `oldStr`/`old_str`, `newStr`/`new_str` and
`replace_all`/`replaceAll`, and block any protected-path call whose shape
they cannot interpret.

## Scripts

| Script | Event | Purpose |
| --- | --- | --- |
| `guard_golden.py` | PreToolUse, `fs_write\|str_replace\|fs_append\|delete_file` | Blocks writes to goldens, fixtures and recordings unless the user approved this exact change |
| `guard_shell.py` | PreToolUse, `execute_bash` | Blocks destructive git, GitHub writes, credential exposure, provider calls, code execution outside the isolation wrapper, and shell writes to protected paths |
| `fmt_check.py` | PostToolUse, write tools | Warns (exit 1) when a written `.rs` file fails `rustfmt --edition 2021 --check`. Never edits files |
| `stop_report.py` | Stop | Prints a working-tree report, notices, relevant `rt-xtask` checks and the review-baseline result. Always exits 0 |
| `guard_review_report.py` | PreToolUse, write tools | Blocks writing `docs/reviews/*.md` without an intact, active review baseline |
| `golden_approve.py` | run by the user | Approves a pending write or an exact shell command |
| `review_integrity.py` | run by the orchestrator | `begin`, `verify`, `status` for the review baseline |
| `hooklib.py` | library | Shared helpers |

Protected paths: `demo/*/golden/**`, `tests/fixtures/**`,
`crates/*/tests/fixtures/**`, `crates/*/tests/golden/**`,
`demo/*/provider/recorded-*.json`, `demo/*/provider/authored-script.json`,
`catalogue/baseline*.json`. `docs/reviews/**` is not protected by
`guard_golden.py`. Paths are normalised with `..` and symlink resolution;
paths outside the repository are not protected.

## Golden approval flow

1. The agent's write is blocked. The message names the pending id, the diff
   file `.git/ruletrace-golden/pending/<id>.diff` and a 30-line preview.
2. The user reviews the diff and runs, in a terminal:
   `tools/hooks/golden_approve.py <id> [--minutes N]`
   (default 30, maximum 120). The script prints the diff and needs the
   literal answer `yes` on an interactive terminal. It refuses when stdin is
   not a TTY.
3. The agent repeats the same write once. The approval binds the path, the
   current file hash and the proposed resulting hash, expires, and is
   consumed by the first matching write. A changed file, different content or
   an expired entry fails by construction.

`golden_approve.py --list` shows pending requests and approvals.
`golden_approve.py --revoke <id>` removes approvals with that id.
For a blocked shell command (rule f) the user runs
`golden_approve.py --command "<exact command>"`; the block message prints it.
Only rule (f) is approvable. The approval is for that exact string.

An `fs_append` hash is computed as old content plus the `text` field. If
Kiro's tool adds anything else (for example a newline), the approved hash
cannot match the real result of the append and the write stays blocked.
This was not probed.

## Review baseline flow

1. Before a review, the orchestrator runs
   `tools/hooks/review_integrity.py begin <review-id> [--exclude <glob> ...]`.
   `--exclude` covers paths a concurrent implementer owns. `docs/reviews/*.md`
   and `target/` are always ignored. A young (under 12 hours) baseline with
   the same id is not overwritten without `--force`.
2. After the review, `review_integrity.py verify [<review-id>]` exits 1 and
   lists added, removed and changed paths and a moved HEAD or branch, or
   exits 0 when identical. Exit 2 means no usable baseline.
3. `guard_review_report.py` runs the same verification when a
   `docs/reviews/*.md` write is attempted and blocks when there is no baseline
   younger than 12 hours or the main tree or HEAD changed.

"added" means a path that was clean at baseline time and is now modified,
staged or untracked. This is a tamper-evidence control. It reveals changes
made during a review. It does not prevent writes while the review runs.

## Network-isolation wrapper (validated before enforcing)

The only accepted form is
`unshare -rn sh -c 'ip link set lo up && exec "$@"' sh <command...>`.

| Command | Result |
| --- | --- |
| wrapper + `cargo test -p rt-ai-responses --test stub_http` | exit 0, 21 passed |
| wrapper + `cargo run -q -p rt-xtask -- check-graph` | exit 0, `check-graph: ok (26 workspace crates)` |
| `unshare -rn cargo test -p rt-ai-responses --test stub_http` (no `lo up`) | exit 101, 3 passed, 18 failed |

The loopback stub tests need `lo`, so a bare `unshare -rn` is not accepted.

## Known limitations

- Shell detection is best-effort pattern matching on a parsed command line.
  Python or perl one-liners that write files, scripts written earlier,
  variable indirection and `bash script.sh` are not detected.
- `cargo mutants`, `cargo llvm-cov` and similar subcommands that run tests
  are not covered by rule (e), which lists `test`, `run`, `nextest`, `bench`.
  `cargo build` may run build scripts and is allowed by specification.
- A literal `RT_DEMO_BLESS` or `--bless` in any command (for example in a
  `grep`) is blocked until the user approves that exact command.
- A quoted operator-only word such as `";"` is read as an operator.
- Stop does not fire for sub-agents. Nothing depends on it there.
- `ask` rules of the agent configuration do not apply to sub-agents; these
  hooks do.
- Run the tests with `python3 -B -m unittest discover -s tools/hooks/tests`
  to avoid a `__pycache__` directory. The hook scripts never write bytecode.

## Tests

`python3 -B -m unittest discover -s tools/hooks/tests` builds throwaway git
repositories under a temporary directory and does not touch this repository.
`python3 -B tools/hooks/tests/mutants.py` applies controlled mutants to a
copy of the scripts and reports which tests kill each one.
