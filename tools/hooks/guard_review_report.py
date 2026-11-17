#!/usr/bin/env python3
"""Require an intact review baseline before a review report is written.

Event: PreToolUse for fs_write, str_replace, fs_append and delete_file.
Exit 0 allows, exit 2 blocks with the reason on stderr. A write to
docs/reviews/*.md needs an active baseline (review_integrity.py begin,
younger than 12 hours) and an unchanged main working tree and HEAD, apart
from docs/reviews/*.md and the baseline's excludes.

This is a tamper-evidence control: it stops a report from being filed over
a tree that changed during the review. It does not prevent writes while
the review runs.
"""

import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402
import review_integrity as ri  # noqa: E402


def evaluate(payload, now=None):
    tool = payload.get("tool_name")
    if tool not in hooklib.WRITE_TOOLS:
        return 0, ""
    path = hooklib.tool_path(payload.get("tool_input"))
    if path is None:
        return 2, "guard_review_report: %s call without a path field is blocked\n" % tool
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not os.path.isdir(cwd):
        return 2, "guard_review_report: payload has no usable cwd; blocked\n"
    try:
        root = hooklib.repo_root(cwd)
    except RuntimeError as exc:
        return 2, "guard_review_report: cannot locate the repository: %s\n" % exc
    reports = [
        rel
        for rel in hooklib.rel_candidates(root, cwd, path)
        if hooklib.glob_match(hooklib.REVIEW_PATTERN, rel)
    ]
    if not reports:
        return 0, ""
    try:
        baseline = ri.active_baseline(cwd, now)
        problems = ri.verify_baseline(cwd, baseline)
    except (ri.IntegrityError, RuntimeError, OSError) as exc:
        return 2, (
            "guard_review_report: writing %s is blocked: %s.\n"
            "The orchestrator must run `tools/hooks/review_integrity.py begin "
            "<review-id>` before the review.\n" % (reports[0], exc)
        )
    if problems:
        return 2, (
            "guard_review_report: writing %s is blocked. The main working tree "
            "changed since baseline %s:\n  %s\n"
            % (reports[0], baseline["id"], "\n  ".join(problems))
        )
    return 0, ""


def main():
    payload = hooklib.read_payload()
    if payload is None:
        sys.stderr.write("guard_review_report: stdin is not a JSON object; blocked\n")
        return 2
    code, message = evaluate(payload)
    if message:
        sys.stderr.write(message)
    return code


if __name__ == "__main__":
    sys.exit(main())
