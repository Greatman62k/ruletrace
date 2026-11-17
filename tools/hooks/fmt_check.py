#!/usr/bin/env python3
"""Warn when a written Rust file is not rustfmt-clean.

Event: PostToolUse for fs_write, str_replace, fs_append and delete_file.
Never modifies files. Exit 1 (a warning only, the tool already ran) when
`rustfmt --edition 2021 --check` reports a difference for the file. Exit 0
when the file is formatted, is not a Rust file, no longer exists, lies
outside the repository, or when rustfmt is missing or times out (a one-line
notice goes to stderr).
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


def evaluate(payload):
    if payload.get("tool_name") not in hooklib.WRITE_TOOLS:
        return 0, ""
    path = hooklib.tool_path(payload.get("tool_input"))
    cwd = payload.get("cwd")
    if path is None or not path.endswith(".rs") or not isinstance(cwd, str):
        return 0, ""
    try:
        root = hooklib.repo_root(cwd)
    except RuntimeError:
        return 0, ""
    full = os.path.realpath(os.path.join(cwd, os.path.expanduser(path)))
    if not hooklib.rel_candidates(root, cwd, path) or not os.path.isfile(full):
        return 0, ""
    if os.path.commonpath([root, full]) != root:
        return 0, ""
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
        return 0, "fmt_check: rustfmt not found; format not checked\n"
    except subprocess.TimeoutExpired:
        return 0, "fmt_check: rustfmt timed out after %d s; format not checked\n" % TIMEOUT
    if proc.returncode == 0:
        return 0, ""
    hunks = file_hunks(proc.stdout, full)
    rel = os.path.relpath(full, root)
    if hunks:
        shown = hunks[:MAX_REPORT_LINES]
        more = len(hunks) - len(shown)
        text = "fmt_check: %s is not rustfmt-clean. Run `rustfmt --edition 2021 %s`.\n" % (rel, rel)
        text += "\n".join(shown) + "\n"
        if more > 0:
            text += "... %d more diff lines\n" % more
        return 1, text
    if "Diff in " in proc.stdout and "error" not in proc.stderr:
        return 0, ""
    detail = (proc.stderr or proc.stdout).strip().splitlines()[:5]
    if not detail:
        return 0, ""
    return 1, "fmt_check: rustfmt could not check %s:\n%s\n" % (rel, "\n".join(detail))


def main():
    payload = hooklib.read_payload()
    if payload is None:
        sys.stderr.write("fmt_check: stdin is not a JSON object; skipped\n")
        return 0
    code, message = evaluate(payload)
    if message:
        sys.stderr.write(message)
    return code


if __name__ == "__main__":
    sys.exit(main())
