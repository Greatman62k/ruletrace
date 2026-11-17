#!/usr/bin/env python3
"""Record and verify the state of the main working tree around a review.

Usage:
  review_integrity.py begin <review-id> [--exclude <glob> ...] [--force]
  review_integrity.py verify [<review-id>]
  review_integrity.py status

`begin` stores HEAD, the branch and a content hash of every modified,
staged and untracked path under `.git/ruletrace-review/`. `verify`
recomputes that state and exits 1 when anything differs. This is a
tamper-evidence control: it reveals changes made during a review, it does
not prevent them. Exit codes: 0 identical or ok, 1 differences or refusal,
2 no usable baseline or usage error.
"""

import argparse
import json
import os
import re
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402

MAX_AGE_SECONDS = 12 * 3600
ID_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
ALWAYS_IGNORED = ("docs/reviews/*.md",)
DELETED = "DELETED"


class IntegrityError(Exception):
    pass


class NoBaseline(IntegrityError):
    """No usable baseline: exit code 2."""


def baseline_dir(cwd):
    return os.path.join(hooklib.git_common_dir(cwd), "ruletrace-review")


def ignored(rel, excludes):
    if rel.startswith("target/"):
        return True
    for pattern in ALWAYS_IGNORED + tuple(excludes):
        if hooklib.glob_match(pattern, rel) or rel == pattern:
            return True
        if pattern.endswith("/") and rel.startswith(pattern):
            return True
    return False


def hash_path(full):
    if os.path.islink(full):
        return hooklib.sha256_bytes(b"symlink:" + os.readlink(full).encode())
    if os.path.isdir(full):
        return "DIRECTORY"
    if not os.path.exists(full):
        return DELETED
    with open(full, "rb") as handle:
        return hooklib.sha256_bytes(handle.read())


def snapshot(cwd, excludes):
    root = hooklib.repo_root(cwd)
    head = hooklib.run_git(root, "rev-parse", "--verify", "-q", "HEAD", check=False)
    branch = hooklib.run_git(root, "symbolic-ref", "-q", "--short", "HEAD", check=False)
    status = hooklib.run_git(
        root, "status", "--porcelain=v1", "-z", "--untracked-files=all"
    ).stdout
    records = status.split("\0")
    files = {}
    i = 0
    while i < len(records):
        record = records[i]
        i += 1
        if len(record) < 4:
            continue
        paths = [record[3:]]
        if record[0] in "RC" or record[1] in "RC":
            paths.append(records[i])
            i += 1
        for rel in paths:
            if not ignored(rel, excludes):
                files[rel] = hash_path(os.path.join(root, rel))
    return {
        "head": head.stdout.strip() if head.returncode == 0 else "(none)",
        "branch": branch.stdout.strip() if branch.returncode == 0 else "(detached)",
        "files": files,
    }


def begin(cwd, review_id, excludes=(), force=False, now=None):
    if not ID_PATTERN.match(review_id):
        raise IntegrityError("review id must match %s" % ID_PATTERN.pattern)
    now = time.time() if now is None else now
    directory = baseline_dir(cwd)
    path = os.path.join(directory, review_id + ".json")
    if os.path.exists(path) and not force:
        try:
            with open(path) as handle:
                age = now - json.load(handle)["created_at"]
        except (OSError, ValueError, KeyError, TypeError):
            age = 0
        if age < MAX_AGE_SECONDS:
            raise IntegrityError(
                "baseline %s already exists and is younger than 12 hours; "
                "use --force to replace it" % review_id
            )
    state = snapshot(cwd, excludes)
    state.update(
        {"id": review_id, "excludes": list(excludes), "created_at": int(now)}
    )
    with hooklib.locked(directory):
        hooklib.write_json_atomic(path, state)
        with open(os.path.join(directory, "latest"), "w") as handle:
            handle.write(review_id + "\n")
    return state


def load(cwd, review_id=None):
    directory = baseline_dir(cwd)
    if review_id is None:
        try:
            with open(os.path.join(directory, "latest")) as handle:
                review_id = handle.read().strip()
        except OSError:
            raise NoBaseline("no review baseline exists (no `latest`)")
    if not ID_PATTERN.match(review_id):
        raise IntegrityError("invalid review id %r" % review_id)
    try:
        with open(os.path.join(directory, review_id + ".json")) as handle:
            baseline = json.load(handle)
        baseline["files"], baseline["head"], baseline["created_at"]
    except (OSError, ValueError, KeyError, TypeError):
        raise NoBaseline("baseline %s is missing or unreadable" % review_id)
    return baseline


def active_baseline(cwd, now=None):
    now = time.time() if now is None else now
    baseline = load(cwd)
    age = now - baseline["created_at"]
    if age >= MAX_AGE_SECONDS or age < -300:
        raise NoBaseline(
            "the latest baseline %s is older than 12 hours (or dated in the "
            "future); begin a new one" % baseline.get("id")
        )
    return baseline


def compare(baseline, current):
    problems = []
    if baseline["head"] != current["head"]:
        problems.append("HEAD moved: %s -> %s" % (baseline["head"], current["head"]))
    if baseline["branch"] != current["branch"]:
        problems.append(
            "branch changed: %s -> %s" % (baseline["branch"], current["branch"])
        )
    old, new = baseline["files"], current["files"]
    for rel in sorted(set(new) - set(old)):
        problems.append("added: %s" % rel)
    for rel in sorted(set(old) - set(new)):
        problems.append("removed: %s" % rel)
    for rel in sorted(set(old) & set(new)):
        if old[rel] != new[rel]:
            problems.append("changed: %s" % rel)
    return problems


def verify_baseline(cwd, baseline):
    current = snapshot(cwd, baseline.get("excludes", []))
    return compare(baseline, current)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    p_begin = sub.add_parser("begin")
    p_begin.add_argument("review_id")
    p_begin.add_argument("--exclude", action="append", default=[])
    p_begin.add_argument("--force", action="store_true")
    p_verify = sub.add_parser("verify")
    p_verify.add_argument("review_id", nargs="?")
    sub.add_parser("status")
    args = parser.parse_args(argv)
    cwd = os.getcwd()
    try:
        if args.cmd == "begin":
            state = begin(cwd, args.review_id, args.exclude, args.force)
            print(
                "baseline %s: HEAD %s on %s, %d changed path(s) recorded"
                % (state["id"], state["head"][:12], state["branch"], len(state["files"]))
            )
            return 0
        if args.cmd == "verify":
            baseline = load(cwd, args.review_id)
            problems = verify_baseline(cwd, baseline)
            if problems:
                print("review integrity FAILED for baseline %s:" % baseline["id"])
                for line in problems:
                    print("  " + line)
                return 1
            print("review integrity ok for baseline %s" % baseline["id"])
            return 0
        baseline = active_baseline(cwd)
        age = int(time.time() - baseline["created_at"])
        print(
            "active baseline %s, %d s old, HEAD %s on %s, %d path(s), excludes %s"
            % (
                baseline["id"],
                age,
                baseline["head"][:12],
                baseline["branch"],
                len(baseline["files"]),
                baseline.get("excludes", []),
            )
        )
        return 0
    except (IntegrityError, RuntimeError, OSError) as exc:
        sys.stderr.write("review_integrity: %s\n" % exc)
        return 2 if isinstance(exc, NoBaseline) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
