#!/usr/bin/env python3
"""Print a working-tree report when the agent stops.

Event: Stop. Always exits 0, because a non-zero Stop exit would start
another agent turn. Every report line is labelled OK, WARN or FAIL and the
last line is `stop-report: N fail, M warn`. Project checks run only when
the changed paths make them relevant, inside the approved isolation
wrapper, within a 180 s total budget. Stop does not fire for sub-agents.
"""

import os
import subprocess
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402
import review_integrity as ri  # noqa: E402

BUDGET_SECONDS = 180
WRAPPER = ["unshare", "-rn", "sh", "-c", 'ip link set lo up && exec "$@"', "sh"]
POLICY_FILES = (
    "tools/allowed-edges.v1",
    "tools/contracts.v1",
    "tools/dependencies.v1",
    "tools/scan-allow.v1",
    "deny.toml",
    "Cargo.toml",
    "Cargo.lock",
)
CATALOGUE_GENERATED = "crates/rt-foundation/src/code_generated.rs"


def is_cargo_file(path):
    return os.path.basename(path) in ("Cargo.toml", "Cargo.lock")


def classify(path):
    """Notice classes of one changed path (a path can have several)."""
    classes = []
    if hooklib.is_protected(path):
        classes.append("golden/fixture/recording")
    if path.startswith("catalogue/") or path == CATALOGUE_GENERATED:
        classes.append("catalogue")
    if (
        path in POLICY_FILES
        or path.startswith(".github/workflows/")
        or is_cargo_file(path)
    ):
        classes.append("architecture edges and policy")
    if path.startswith("tools/decisions/"):
        classes.append("decision record")
    if path.startswith(".kiro/specs/"):
        classes.append("spec file")
    return classes


def relevant_checks(paths):
    """Checks to run, in a fixed order, for the changed paths."""
    wanted = []
    tools_changed = any(
        p.startswith("tools/")
        and not p.startswith(("tools/hooks/", "tools/decisions/"))
        for p in paths
    )
    code_changed = any(
        p.endswith(".rs") or is_cargo_file(p) or p == "deny.toml" for p in paths
    )
    if code_changed or tools_changed:
        wanted += ["check-graph", "check-tree", "scan-sources", "scan-api"]
    if any(p.startswith("catalogue/") or p == CATALOGUE_GENERATED for p in paths):
        wanted.append("check-catalogue")
    if any(p.startswith("tools/decisions/") for p in paths):
        wanted.append("check-decisions")
    if any(p == "tools/dependencies.v1" or is_cargo_file(p) for p in paths):
        wanted.append("check-justification")
    return wanted


def porcelain_paths(root):
    out = hooklib.run_git(
        root, "status", "--porcelain=v1", "-z", "--untracked-files=all"
    ).stdout
    records = out.split("\0")
    entries = []
    i = 0
    while i < len(records):
        record = records[i]
        i += 1
        if len(record) < 4:
            continue
        entries.append((record[:2], record[3:]))
        if record[0] in "RC" or record[1] in "RC":
            i += 1
    return entries


def run_check(root, name, deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        return "WARN", "%s skipped: the %d s budget is used up" % (name, BUDGET_SECONDS), True
    command = WRAPPER + ["cargo", "run", "-q", "-p", "rt-xtask", "--", name]
    try:
        proc = subprocess.run(
            command,
            cwd=root,
            capture_output=True,
            text=True,
            errors="replace",
            timeout=remaining,
        )
    except FileNotFoundError:
        return "WARN", "%s not run: `unshare` is not available, so the wrapper cannot run" % name, True
    except subprocess.TimeoutExpired:
        return "WARN", "%s stopped: the %d s budget ran out" % (name, BUDGET_SECONDS), True
    if proc.returncode == 0:
        return "OK", "%s passed" % name, False
    first = (proc.stderr or "").strip().splitlines()[:1]
    if first and first[0].startswith("unshare:"):
        return "WARN", "%s not run: the wrapper cannot start (%s)" % (name, first[0]), True
    tail = " | ".join((proc.stdout + proc.stderr).strip().splitlines()[-3:])
    return "FAIL", "%s exited %d: %s" % (name, proc.returncode, tail), False


def build_report(cwd):
    lines = []
    root = hooklib.repo_root(cwd)
    branch = hooklib.run_git(root, "branch", "--show-current").stdout.strip() or "(detached)"
    head = hooklib.run_git(root, "rev-parse", "--short", "HEAD", check=False)
    entries = porcelain_paths(root)
    staged = sum(1 for xy, _ in entries if xy[0] not in " ?")
    unstaged = sum(1 for xy, _ in entries if xy[1] not in " ?")
    untracked = sum(1 for xy, _ in entries if xy == "??")
    lines.append("OK   branch %s, HEAD %s" % (branch, head.stdout.strip() or "(none)"))
    lines.append(
        "OK   working tree: %d staged, %d unstaged, %d untracked"
        % (staged, unstaged, untracked)
    )
    for xy, path in entries:
        lines.append("OK   status %s %s" % (xy, path))
    paths = [p for _, p in entries]
    lines.append("NOTICE")
    notices = 0
    for path in paths:
        for cls in classify(path):
            lines.append("WARN %s changed: %s" % (cls, path))
            notices += 1
    if not notices:
        lines.append("OK   no notice-class path changed")
    deadline = time.monotonic() + BUDGET_SECONDS
    checks = relevant_checks(paths)
    if not checks:
        lines.append("OK   no project check is relevant to the changed paths")
    for name in checks:
        label, text, stop = run_check(root, name, deadline)
        lines.append("%s %s" % (label.ljust(4), text))
        if stop and label == "WARN" and "not run" in text:
            break
    try:
        baseline = ri.active_baseline(root)
    except ri.IntegrityError:
        baseline = None
    if baseline is not None:
        problems = ri.verify_baseline(root, baseline)
        if problems:
            for problem in problems:
                lines.append("FAIL review baseline %s: %s" % (baseline["id"], problem))
        else:
            lines.append("OK   review baseline %s is intact" % baseline["id"])
    fails = sum(1 for l in lines if l.startswith("FAIL"))
    warns = sum(1 for l in lines if l.startswith("WARN"))
    lines.append("stop-report: %d fail, %d warn" % (fails, warns))
    return lines


def main():
    payload = hooklib.read_payload() or {}
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not os.path.isdir(cwd):
        cwd = os.getcwd()
    try:
        lines = build_report(cwd)
    except (RuntimeError, OSError) as exc:
        lines = ["FAIL stop-report could not inspect the repository: %s" % exc,
                 "stop-report: 1 fail, 0 warn"]
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
