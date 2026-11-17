#!/usr/bin/env python3
"""User-run approval of a blocked golden write or shell command.

Usage:
  golden_approve.py <id> [--minutes N]
  golden_approve.py --command "<exact command>" [--minutes N]
  golden_approve.py --list
  golden_approve.py --revoke <id>

Approving needs the literal answer `yes` on an interactive terminal, so an
agent that only controls pipes cannot grant itself an approval. Entries
bind the exact old and new file hashes (or the exact command), expire, and
are consumed by the first matching write.
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
from guard_golden import state_hash  # noqa: E402

DEFAULT_MINUTES = 30
MAX_MINUTES = 120
ID_PATTERN = re.compile(r"^[0-9a-f]{12}$")


class ApproveError(Exception):
    pass


def shell_id(command):
    return hooklib.sha256_bytes(("shell|" + command).encode("utf-8"))[:12]


def check_minutes(minutes):
    if not isinstance(minutes, int) or not 1 <= minutes <= MAX_MINUTES:
        raise ApproveError("--minutes must be between 1 and %d" % MAX_MINUTES)


def _append(directory, entry):
    with hooklib.locked(directory):
        entries = hooklib.load_approvals(directory)
        entries.append(entry)
        hooklib.save_approvals(directory, entries)


def approve_pending(cwd, ident, minutes=DEFAULT_MINUTES, now=None):
    """Append a file approval for a pending request. Returns the entry."""
    check_minutes(minutes)
    if not ID_PATTERN.match(ident):
        raise ApproveError("id must be 12 lowercase hex characters")
    directory = hooklib.store_dir(cwd)
    path = os.path.join(directory, "pending", ident + ".json")
    try:
        with open(path) as handle:
            pending = json.load(handle)
    except (OSError, ValueError):
        raise ApproveError("no pending request %s" % ident)
    root = hooklib.repo_root(cwd)
    full = os.path.join(root, pending["path"])
    if os.path.isdir(full):
        raise ApproveError("%s is now a directory" % pending["path"])
    current = None
    if os.path.exists(full):
        with open(full, "rb") as handle:
            current = handle.read()
    if state_hash(current) != pending["old_sha256"]:
        raise ApproveError(
            "%s changed since the request; repeat the write to get a new id"
            % pending["path"]
        )
    now = time.time() if now is None else now
    entry = {
        "id": ident,
        "kind": "file",
        "path": pending["path"],
        "old_sha256": pending["old_sha256"],
        "new_sha256": pending["new_sha256"],
        "expires_at": int(now + minutes * 60),
        "used": False,
    }
    _append(directory, entry)
    return entry


def approve_command(cwd, command, minutes=DEFAULT_MINUTES, now=None):
    check_minutes(minutes)
    if not command.strip():
        raise ApproveError("empty command")
    now = time.time() if now is None else now
    entry = {
        "id": shell_id(command),
        "kind": "shell",
        "command_sha256": hooklib.sha256_bytes(command.encode("utf-8")),
        "expires_at": int(now + minutes * 60),
        "used": False,
    }
    _append(hooklib.store_dir(cwd), entry)
    return entry


def revoke(cwd, ident):
    directory = hooklib.store_dir(cwd)
    with hooklib.locked(directory):
        entries = hooklib.load_approvals(directory)
        kept = [e for e in entries if e.get("id") != ident]
        if len(kept) == len(entries):
            raise ApproveError("no approval with id %s" % ident)
        hooklib.save_approvals(directory, kept)
    return len(entries) - len(kept)


def list_state(cwd, now=None):
    now = time.time() if now is None else now
    directory = hooklib.store_dir(cwd)
    lines = []
    pending_dir = os.path.join(directory, "pending")
    if os.path.isdir(pending_dir):
        for name in sorted(os.listdir(pending_dir)):
            if name.endswith(".json"):
                with open(os.path.join(pending_dir, name)) as handle:
                    item = json.load(handle)
                what = item.get("path") or item.get("command", "?")
                lines.append("pending  %s  %s" % (item["id"], what))
    for entry in hooklib.load_approvals(directory):
        if entry.get("used"):
            state = "used"
        elif entry.get("expires_at", 0) <= now:
            state = "expired"
        else:
            state = "active"
        what = entry.get("path") or "command sha256 %s" % entry.get("command_sha256")
        lines.append("approval %s  %-7s %s" % (entry.get("id"), state, what))
    return lines


def confirm():
    if not sys.stdin.isatty():
        raise ApproveError(
            "stdin is not an interactive terminal; run this command yourself "
            "in a terminal. Approvals cannot be granted through a pipe."
        )
    sys.stdout.write('Type "yes" to approve: ')
    sys.stdout.flush()
    return sys.stdin.readline().strip() == "yes"


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("id", nargs="?")
    parser.add_argument("--minutes", type=int, default=DEFAULT_MINUTES)
    parser.add_argument("--command")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--revoke", metavar="ID")
    args = parser.parse_args(argv)
    cwd = os.getcwd()
    try:
        if args.list:
            for line in list_state(cwd):
                print(line)
            return 0
        if args.revoke:
            print("revoked %d entr(y/ies)" % revoke(cwd, args.revoke))
            return 0
        check_minutes(args.minutes)
        if args.command is not None:
            print("Command to approve (single use, exact string):\n  %s" % args.command)
            if not confirm():
                print("not approved")
                return 1
            entry = approve_command(cwd, args.command, args.minutes)
        elif args.id:
            if not ID_PATTERN.match(args.id):
                raise ApproveError("id must be 12 lowercase hex characters")
            diff_path = os.path.join(
                hooklib.store_dir(cwd), "pending", args.id + ".diff"
            )
            try:
                with open(diff_path) as handle:
                    sys.stdout.write(handle.read())
            except OSError:
                raise ApproveError("no pending request %s" % args.id)
            if not confirm():
                print("not approved")
                return 1
            entry = approve_pending(cwd, args.id, args.minutes)
        else:
            parser.print_usage(sys.stderr)
            return 2
    except (ApproveError, hooklib.CorruptStore, RuntimeError) as exc:
        sys.stderr.write("golden_approve: %s\n" % exc)
        return 2
    print(
        "approved %s, valid %d minutes, single use" % (entry["id"], args.minutes)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
