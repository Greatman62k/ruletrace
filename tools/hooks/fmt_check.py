#!/usr/bin/env python3
"""Warn when a written Rust file is not rustfmt-clean.

Event: PostToolUse for fs_write, str_replace, fs_append and delete_file.
Never modifies source files. Exit 1 (a warning only, the tool already ran)
when `rustfmt --edition 2021 --check` reports a difference for the file.
Exit 0 when the file is formatted, is not a Rust file, no longer exists,
lies outside the repository, or when rustfmt is missing or times out (a
one-line notice goes to stderr).

The agent does not see PostToolUse output in headless mode, so the latest
report per path is also kept in `.git/ruletrace-hooks/fmt-warnings.json`.
An entry is removed when the path is later found clean or is deleted. When
that file cannot be written, one notice goes to stderr and the exit code is
unchanged.
"""

import os
import re
import subprocess
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402

TIMEOUT = 20
MAX_REPORT_LINES = 40
HEADER = re.compile(r"^Diff in (.+?)(?::(\d+)| at line (\d+)):?$")


def file_hunks(output, target):
    """Keep only the diff hunks whose header names `target`.

    rustfmt also formats `mod` children, so the report can mention other
    files that this write did not touch.
    """
    kept = []
    keep = False
    for line in output.splitlines():
        match = HEADER.match(line)
        if match:
            keep = os.path.realpath(match.group(1)) == target
        if keep:
            kept.append(line)
    return kept


def run_rustfmt(root, full):
    """Check one file. Returns (status, payload).

    Status is `unavailable` or `timeout` (payload: notice), `clean`, `diff`
    (payload: the hunk lines of this file only) or `error` (payload: detail
    text).
    """
    try:
        proc = subprocess.run(
            ["rustfmt", "--edition", "2021", "--color", "never", "--check", full],
            cwd=root,
            capture_output=True,
            text=True,
            errors="replace",
            timeout=TIMEOUT,
        )
    except FileNotFoundError:
        return "unavailable", "rustfmt not found; format not checked"
    except subprocess.TimeoutExpired:
        return "timeout", "rustfmt timed out after %d s; format not checked" % TIMEOUT
    if proc.returncode == 0:
        return "clean", ""
    hunks = file_hunks(proc.stdout, full)
    if hunks:
        return "diff", hunks
    if "Diff in " in proc.stdout and "error" not in proc.stderr:
        return "clean", ""
    detail = (proc.stderr or proc.stdout).strip().splitlines()[:5]
    if not detail:
        return "clean", ""
    return "error", "\n".join(detail)


def render_diff(rel, hunks):
    shown = hunks[:MAX_REPORT_LINES]
    more = len(hunks) - len(shown)
    text = "fmt_check: %s is not rustfmt-clean. Run `rustfmt --edition 2021 %s`.\n" % (rel, rel)
    text += "\n".join(shown) + "\n"
    if more > 0:
        text += "... %d more diff lines\n" % more
    return text


def evaluate(payload):
    """Return (exit code, stderr text, state update or None).

    The update is ("set", rel, report) or ("clear", rel, "").
    """
    if payload.get("tool_name") not in hooklib.WRITE_TOOLS:
        return 0, "", None
    path = hooklib.tool_path(payload.get("tool_input"))
    cwd = payload.get("cwd")
    if path is None or not path.endswith(".rs") or not isinstance(cwd, str):
        return 0, "", None
    try:
        root = hooklib.repo_root(cwd)
    except RuntimeError:
        return 0, "", None
    full = os.path.realpath(os.path.join(cwd, os.path.expanduser(path)))
    if not hooklib.rel_candidates(root, cwd, path):
        return 0, "", None
    if os.path.commonpath([root, full]) != root:
        return 0, "", None
    rel = os.path.relpath(full, root).replace(os.sep, "/")
    if not os.path.isfile(full):
        return 0, "", ("clear", rel, "")
    status, payload_out = run_rustfmt(root, full)
    if status in ("unavailable", "timeout"):
        return 0, "fmt_check: %s\n" % payload_out, None
    if status == "clean":
        return 0, "", ("clear", rel, "")
    if status == "diff":
        text = render_diff(rel, payload_out)
    else:
        text = "fmt_check: rustfmt could not check %s:\n%s\n" % (rel, payload_out)
    return 1, text, ("set", rel, text)


def record(cwd, update):
    """Apply a state update to fmt-warnings.json under a lock."""
    kind, rel, text = update
    directory = hooklib.report_dir(cwd)
    target = os.path.join(directory, hooklib.FMT_WARNINGS)
    if kind == "clear" and not os.path.exists(target):
        return
    with hooklib.locked(directory):
        entries, _ = hooklib.load_fmt_warnings(directory)
        if kind == "set":
            entries[rel] = {"report": text, "timestamp": hooklib.utc_now()}
        elif rel in entries:
            del entries[rel]
        else:
            return
        hooklib.write_json_atomic(target, entries)


def main():
    payload = hooklib.read_payload()
    if payload is None:
        sys.stderr.write("fmt_check: stdin is not a JSON object; skipped\n")
        return 0
    code, message, update = evaluate(payload)
    if message:
        sys.stderr.write(message)
    if update is not None:
        try:
            record(payload["cwd"], update)
        except (OSError, RuntimeError) as exc:
            sys.stderr.write(
                "fmt_check: cannot update the warnings file: %s\n" % exc
            )
    return code


if __name__ == "__main__":
    sys.exit(main())
