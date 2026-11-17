"""Shared helpers for the RuleTrace Kiro hook scripts.

Standard library only. The only external state is files under the
repository's git directory.
"""

import contextlib
import fcntl
import hashlib
import json
import os
import re
import subprocess
import sys
import time

WRITE_TOOLS = ("fs_write", "str_replace", "fs_append", "delete_file")

PROTECTED_PATTERNS = (
    "demo/*/golden/**",
    "tests/fixtures/**",
    "crates/*/tests/fixtures/**",
    "crates/*/tests/golden/**",
    "demo/*/provider/recorded-*.json",
    "demo/*/provider/authored-script.json",
    "catalogue/baseline*.json",
)

REVIEW_PATTERN = "docs/reviews/*.md"


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def read_payload():
    """Return the hook payload dict, or None when stdin is not a JSON object."""
    try:
        payload = json.loads(sys.stdin.read())
    except (ValueError, OSError):
        return None
    return payload if isinstance(payload, dict) else None


def run_git(cwd, *args, check=True):
    try:
        proc = subprocess.run(
            ["git", *args],
            cwd=cwd,
            capture_output=True,
            text=True,
            errors="replace",
        )
    except OSError as exc:
        raise RuntimeError("cannot run git: %s" % exc)
    if check and proc.returncode != 0:
        raise RuntimeError("git %s failed: %s" % (" ".join(args), proc.stderr.strip()))
    return proc


def repo_root(cwd):
    out = run_git(cwd, "rev-parse", "--show-toplevel").stdout.strip()
    return os.path.realpath(out)


def git_common_dir(cwd):
    out = run_git(
        cwd, "rev-parse", "--path-format=absolute", "--git-common-dir"
    ).stdout.strip()
    return os.path.realpath(out)


def _glob_regex(pattern):
    """Translate a glob where `*` stays inside a segment and `**` spans."""
    out = []
    i = 0
    while i < len(pattern):
        ch = pattern[i]
        if pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif ch == "*":
            out.append("[^/]*")
            i += 1
        elif ch == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(ch))
            i += 1
    return re.compile("".join(out) + r"\Z")


def glob_match(pattern, rel):
    return _glob_regex(pattern).match(rel) is not None


def is_protected(rel):
    return any(glob_match(p, rel) for p in PROTECTED_PATTERNS)


def is_protected_ancestor(rel):
    """True when `rel` is a directory that contains protected paths.

    Used for recursive removal and moves, where the directory itself is
    the operand. Walks the pattern segment by segment up to its `**`.
    """
    parts = [p for p in rel.split("/") if p not in ("", ".")]
    if not parts:
        return True
    for pattern in PROTECTED_PATTERNS:
        segs = pattern.split("/")
        prefix = []
        for seg in segs:
            if seg == "**":
                break
            prefix.append(seg)
        else:
            continue
        if len(parts) > len(prefix):
            continue
        if all(glob_match(s, p) for s, p in zip(prefix, parts)):
            return True
    return False


def rel_candidates(root, base, path):
    """Repo-relative forms of `path`: lexical and symlink-resolved.

    Both are returned so a symlink cannot hide a protected target and a
    protected-looking link name is still honoured. Paths outside the
    repository yield nothing.
    """
    expanded = os.path.expanduser(path)
    joined = os.path.join(base, expanded)
    absolute = os.path.normpath(joined)
    found = []
    # The kernel resolves symlinks before `..`, so resolve the raw join too.
    for candidate in (absolute, os.path.realpath(joined), os.path.realpath(absolute)):
        rel = os.path.relpath(candidate, root)
        if rel == ".." or rel.startswith("../") or os.path.isabs(rel):
            continue
        rel = rel.replace(os.sep, "/")
        if rel not in found:
            found.append(rel)
    return found


def tool_path(tool_input):
    """The target path of a write tool call, or None when absent."""
    if not isinstance(tool_input, dict):
        return None
    for key in ("path", "file_path", "filePath"):
        value = tool_input.get(key)
        if isinstance(value, str) and value:
            return value
    return None


def first_field(tool_input, *names):
    for name in names:
        if name in tool_input:
            return tool_input[name]
    return None


def store_dir(cwd):
    return os.path.join(git_common_dir(cwd), "ruletrace-golden")


@contextlib.contextmanager
def locked(directory):
    os.makedirs(directory, exist_ok=True)
    with open(os.path.join(directory, ".lock"), "w") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        yield


def write_json_atomic(path, value):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = "%s.tmp.%d" % (path, os.getpid())
    with open(tmp, "w") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
    os.replace(tmp, path)


class CorruptStore(Exception):
    pass


def load_approvals(directory):
    path = os.path.join(directory, "approvals.json")
    if not os.path.exists(path):
        return []
    try:
        with open(path) as handle:
            data = json.load(handle)
    except (ValueError, OSError) as exc:
        raise CorruptStore("%s is unreadable: %s" % (path, exc))
    if not isinstance(data, list):
        raise CorruptStore("%s is not a list" % path)
    return data


def save_approvals(directory, entries):
    write_json_atomic(os.path.join(directory, "approvals.json"), entries)


def consume_approval(directory, predicate, now=None):
    """Mark the first live entry accepted by `predicate` as used.

    Returns True when an entry was consumed. Must be called under the
    store lock held by the caller's `locked` context.
    """
    now = time.time() if now is None else now
    entries = load_approvals(directory)
    for entry in entries:
        if not isinstance(entry, dict) or entry.get("used") is not False:
            continue
        expires = entry.get("expires_at")
        if not isinstance(expires, (int, float)) or isinstance(expires, bool):
            continue
        if expires <= now:
            continue
        if predicate(entry):
            entry["used"] = True
            save_approvals(directory, entries)
            return True
    return False


def block(message):
    sys.stderr.write(message.rstrip("\n") + "\n")
    sys.exit(2)
