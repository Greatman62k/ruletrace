# Kiro hook scripts

Development tooling for Kiro CLI. Nothing here is needed to build, test or
run RuleTrace. Python 3 standard library only. The scripts read one JSON
object from stdin, run with the project root as working directory, and keep
their only state under the repository's `.git/` directory. Wiring them as
hooks is a separate step; this directory contains no hook configuration.

## Verified facts (Kiro CLI 3.3.0)

Observed with headless `kiro-cli chat --v3 --no-interactive`:

| Event | Exit 0 | Exit 2 | Other non-zero | Output reaches the agent |
| --- | --- | --- | --- | --- |
| PreToolUse | allow | block | warning only | stderr on exit 2 only. It is shown to the agent as the reason |
| PostToolUse | ok | warning only (the tool already ran) | warning only | no |
| Stop | ok | not used: a non-zero Stop exit would start another turn | not used | no |

- PreToolUse blocking works as designed for `guard_golden.py`,
  `guard_shell.py` and `guard_review_report.py`.
- The stdout or stderr of PostToolUse and Stop hooks, at exit 0, 1 or 2, did
  not reach the agent and was not visible in the transcript. Whether the
  interactive TUI shows that output is NOT verified. Nothing here depends on
  it: `fmt_check.py` and `stop_report.py` keep their results in files (see
  "Persistent reports").
- The `str_replace` payload fields are exactly `path`, `oldStr`, `newStr` and
  `replace_all`. The scripts still accept `old_str`, `new_str` and
  `replaceAll`.
- `fs_append` and `delete_file` do not exist as tools in the default agent's
  tool set. Their names are unverified. The scripts keep handling them, and
  `guard_golden.py` still blocks a protected-path call whose shape it cannot
  interpret.
- Workspace hooks apply to the top-level agent and to sub-agents. No Stop
  event was observed for sub-agent runs.

## Scripts

| Script | Event | Purpose |
| --- | --- | --- |
| `guard_golden.py` | PreToolUse, `fs_write\|str_replace\|fs_append\|delete_file` | Blocks writes to goldens, fixtures and recordings unless the user approved this exact change |
| `guard_shell.py` | PreToolUse, `execute_bash` | Blocks destructive git, GitHub writes, credential exposure, provider calls, code execution outside the isolation wrapper, and shell writes to protected paths |
| `fmt_check.py` | PostToolUse, write tools | Warns (exit 1) when a written `.rs` file fails `rustfmt --edition 2021 --check` and records it in `.git/ruletrace-hooks/fmt-warnings.json`. Never edits files |
| `stop_report.py` | Stop | Prints a working-tree report, notices, relevant `rt-xtask` checks, a `FORMAT` section and the review-baseline result, and writes the same text to `.git/ruletrace-hooks/last-stop-report.txt`. Always exits 0 |
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

## Persistent reports

Both files live in `.git/ruletrace-hooks/` (the git common directory, so
worktrees share it). The scripts create the directory and replace each file
atomically (temporary file, then rename). They never fail the hook when the
location is not writable: they print one notice on stderr and keep their
exit code. Read the files with an ordinary file read or `cat`.

- `fmt-warnings.json`: an object that maps a repo-relative `.rs` path to
  `{"report": <the diff report>, "timestamp": "YYYY-MM-DDTHH:MM:SSZ"}` (UTC).
  A new report replaces the old one for that path. The entry is removed when
  the same path is later found clean, and also when the file no longer
  exists. An inconclusive run (rustfmt missing or timed out) leaves it as it
  was. A damaged file counts as empty and is replaced by the next write.
  Writers take a lock, so concurrent hooks do not lose entries.
- `last-stop-report.txt`: the complete Stop report, identical to stdout,
  overwritten on every run.

`stop_report.py` has a `FORMAT` section. It runs the read-only
`rustfmt --edition 2021 --color never --check <file>` on each tracked-modified,
staged or untracked `.rs` file that still exists (at most 40, sorted by
path), 20 s per file. Each file is `OK` or `WARN`. A `WARN` shows only that
file's diff hunks, at most 20 lines. A timeout is a `WARN` for that file and
the next file is still checked. Missing rustfmt is one `WARN`. The section
then lists every entry still in `fmt-warnings.json` as `WARN`, or says there
is none. `stop_report.py` never modifies a source file or the warnings file.

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

An `fs_append` hash is computed as old content plus the `text` field. The
tool's existence and behaviour are unverified (see "Verified facts"). If it
adds anything else (for example a newline), the approved hash cannot match
the real result of the append and the write stays blocked.

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

## Rules of `guard_shell.py` that changed

Rule (e), network isolation, now also covers `cargo mutants`,
`cargo llvm-cov`, `cargo tarpaulin`, `cargo miri` and `cargo careful`
(also with `+toolchain` and as the direct binaries `cargo-mutants` and so
on), because they execute project tests. They need the approved wrapper
below. These forms stay allowed without it: `--version`, `-V`, `--help` or
`-h` for any of the five, and `cargo mutants --list`, `--list-files` and
`--list-dirs`. The check looks only at arguments before `--`. Everything
else is blocked, including `cargo mutants --check`, `cargo miri setup` and
`cargo llvm-cov --no-run`, which do not run tests but cannot be told apart
cheaply from forms that do.

Rule (f), golden writes, no longer blocks the bare text. It blocks:

- `RT_DEMO_BLESS=...`, `RT_APP_GOLDEN_BLESS=...` (app-report goldens, task
  9.16) or `RULETRACE_BLESS=...` (slice goldens) (also `+=`) as an
  environment assignment word: before
  a command, after `env` (with or without options, also when `env` itself
  follows `nice`, `timeout` or `xargs`), as a bare assignment statement,
  after `export`, `declare`, `typeset`, `readonly` or `local`, and inside a
  recursed `sh -c`, `eval`, `$(...)`, `find -exec` or heredoc-to-shell
  string.
- a `--bless` or `--bless=...` token when the command is not one of `grep`,
  `egrep`, `fgrep`, `rg`, `echo`, `printf`, `cat`, `less`, `head`, `tail`,
  `awk`, `gawk`, `wc`, or `sed` with `-n` (and without `-i`).

So `grep -rn RT_DEMO_BLESS crates`, `echo RT_DEMO_BLESS`, `rg RT_DEMO_BLESS`
and `grep -- --bless README.md` are allowed. The approval path
(`golden_approve.py --command`) is unchanged for the blocked forms. A
command that cannot be parsed and contains `--bless` or one of the three
assignments is
blocked without an approval path, like other unparseable commands.

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
- Rule (e) lists `test`, `run`, `nextest`, `bench`, `mutants`, `llvm-cov`,
  `tarpaulin`, `miri` and `careful`. Other test-running cargo plugins and
  cargo aliases are not covered. `cargo build` may run build scripts and is
  allowed by specification.
- Rule (f) does not see a bless assignment made through indirection:
  `printf -v`, `read`, `eval` of a built string, a script piped to `sh`, or
  a script file written earlier. A `--bless` passed to a reader such as
  `grep` is treated as text even if that program would act on it.
- PostToolUse and Stop output does not reach the agent in headless mode and
  the interactive display is unverified. Someone has to read
  `.git/ruletrace-hooks/*` (or the agent has to be told to) for those
  results to matter.
- `stop_report.py` spends up to 20 s per `.rs` file on top of the 180 s
  check budget, at most 40 files. Whether Kiro enforces a Stop hook
  timeout is unverified.
- `fmt-warnings.json` records the last hook run per path only. A file edited
  outside the agent's write tools is not re-checked until written again.
- A quoted operator-only word such as `";"` is read as an operator.
- Stop does not fire for sub-agents. Nothing depends on it there.
- `ask` rules of the agent configuration do not apply to sub-agents; these
  hooks do.
- Run the tests with `python3 -B -m unittest discover -s tools/hooks/tests`
  to avoid a `__pycache__` directory. The hook scripts never write bytecode.

## Tests

`python3 -B -m unittest discover -s tools/hooks/tests` builds throwaway git
repositories under a temporary directory and does not touch this repository.
`python3 -B tools/hooks/tests/mutants.py [name-fragment ...]` applies
controlled mutants to a copy of the scripts and reports which tests kill each
one. The suite includes a 20 s timeout test and the mutant run takes several
minutes.
