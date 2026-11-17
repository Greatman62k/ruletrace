#!/usr/bin/env python3
"""Block unapproved writes to goldens, fixtures and recordings.

Event: PreToolUse for fs_write, str_replace, fs_append and delete_file.
Exit 0 allows the call. Exit 2 blocks it and stderr is shown to the agent.
A write is allowed only when a single-use approval created by
tools/hooks/golden_approve.py matches the path, the current file hash and
the proposed resulting hash exactly. Unknown input shapes for a protected
path are blocked.
"""

import difflib
import os
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402

DIFF_LIMIT = 200 * 1024
PREVIEW_LINES = 30
ABSENT = "ABSENT"
DELETED = "DELETED"


class Refuse(Exception):
    """The call cannot be evaluated safely; the message is the reason."""


def read_current(full):
    if os.path.isdir(full):
        raise Refuse("%s is a directory" % full)
    if not os.path.exists(full):
        return None
    with open(full, "rb") as handle:
        return handle.read()


def propose(tool, tool_input, current):
    """Return the bytes the file would hold after the call, or DELETED."""
    if tool == "delete_file":
        return DELETED
    if tool == "fs_write":
        text = tool_input.get("text")
        if not isinstance(text, str):
            raise Refuse("fs_write without a string `text` field")
        return text.encode("utf-8")
    if tool == "fs_append":
        text = tool_input.get("text")
        if not isinstance(text, str):
            raise Refuse("fs_append without a string `text` field")
        return (current or b"") + text.encode("utf-8")
    if tool == "str_replace":
        old = hooklib.first_field(tool_input, "oldStr", "old_str")
        new = hooklib.first_field(tool_input, "newStr", "new_str")
        every = hooklib.first_field(tool_input, "replace_all", "replaceAll")
        if not isinstance(old, str) or not isinstance(new, str) or old == "":
            raise Refuse("str_replace without usable oldStr/newStr fields")
        if every is not None and not isinstance(every, bool):
            raise Refuse("str_replace with a non-boolean replace_all")
        if current is None:
            raise Refuse("str_replace on a file that does not exist")
        try:
            text = current.decode("utf-8")
        except UnicodeDecodeError:
            raise Refuse("str_replace on a file that is not UTF-8")
        count = text.count(old)
        if count == 0:
            raise Refuse("str_replace: oldStr not found in the file")
        if count > 1 and not every:
            raise Refuse("str_replace: oldStr is ambiguous and replace_all is not set")
        return text.replace(old, new).encode("utf-8")
    raise Refuse("unsupported tool %r" % tool)


def state_hash(data):
    return ABSENT if data is None else hooklib.sha256_bytes(data)


def decode(data):
    return "" if data is None or data == DELETED else data.decode("utf-8", "replace")


def make_diff(rel, current, proposed):
    old = decode(current).splitlines(keepends=True)
    new = decode(proposed).splitlines(keepends=True)
    diff = "".join(
        difflib.unified_diff(
            old,
            new,
            "a/%s%s" % (rel, " (absent)" if current is None else ""),
            "b/%s%s" % (rel, " (deleted)" if proposed == DELETED else ""),
        )
    )
    if len(diff.encode("utf-8")) > DIFF_LIMIT:
        diff = diff.encode("utf-8")[:DIFF_LIMIT].decode("utf-8", "ignore")
        diff += "\n[diff truncated at 200 KiB]\n"
    return diff


def request_id(rel, old_hash, new_hash):
    material = "|".join((rel, old_hash, new_hash)).encode("utf-8")
    return hooklib.sha256_bytes(material)[:12]


def evaluate(payload, now=None):
    """Return (exit_code, stderr_text) for one PreToolUse payload."""
    tool = payload.get("tool_name")
    if tool not in hooklib.WRITE_TOOLS:
        return 0, ""
    tool_input = payload.get("tool_input")
    path = hooklib.tool_path(tool_input)
    if path is None:
        return 2, "guard_golden: %s call without a path field is blocked\n" % tool
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not os.path.isdir(cwd):
        return 2, "guard_golden: payload has no usable cwd; blocked\n"
    try:
        root = hooklib.repo_root(cwd)
    except RuntimeError as exc:
        return 2, "guard_golden: cannot locate the repository: %s\n" % exc
    protected = [
        rel
        for rel in hooklib.rel_candidates(root, cwd, path)
        if hooklib.is_protected(rel)
    ]
    if not protected:
        return 0, ""
    rel = protected[-1]
    full = os.path.join(root, rel)
    try:
        current = read_current(full)
        proposed = propose(tool, tool_input, current)
    except Refuse as exc:
        return 2, "guard_golden: write to protected path %s blocked: %s\n" % (rel, exc)
    except OSError as exc:
        return 2, "guard_golden: cannot read %s: %s\n" % (rel, exc)
    old_hash = state_hash(current)
    new_hash = DELETED if proposed == DELETED else hooklib.sha256_bytes(proposed)
    if proposed != DELETED and current is not None and proposed == current:
        return 0, ""
    directory = hooklib.store_dir(cwd)
    try:
        with hooklib.locked(directory):
            allowed = hooklib.consume_approval(
                directory,
                lambda e: e.get("kind", "file") == "file"
                and e.get("path") == rel
                and e.get("old_sha256") == old_hash
                and e.get("new_sha256") == new_hash,
                now,
            )
    except hooklib.CorruptStore as exc:
        return 2, "guard_golden: approval store problem, blocked: %s\n" % exc
    if allowed:
        return 0, ""
    ident = request_id(rel, old_hash, new_hash)
    pending = os.path.join(directory, "pending")
    diff = make_diff(rel, current, proposed)
    diff_path = os.path.join(pending, ident + ".diff")
    os.makedirs(pending, exist_ok=True)
    with open(diff_path, "w") as handle:
        handle.write(diff)
    hooklib.write_json_atomic(
        os.path.join(pending, ident + ".json"),
        {
            "id": ident,
            "kind": "file",
            "path": rel,
            "old_sha256": old_hash,
            "new_sha256": new_hash,
            "created_at": int(time.time() if now is None else now),
        },
    )
    preview = "".join(diff.splitlines(keepends=True)[:PREVIEW_LINES])
    message = (
        "guard_golden: the write to %s is blocked. Goldens, fixtures and\n"
        "recordings change only by an explicit, reviewed edit.\n"
        "Pending id: %s\n"
        "Diff file: %s\n"
        "--- first %d diff lines ---\n%s"
        "--- end preview ---\n"
        "Ask the user to review the diff and run, in a terminal:\n"
        "  tools/hooks/golden_approve.py %s\n"
        "then repeat exactly this write once. Do not work around the block.\n"
        % (rel, ident, diff_path, PREVIEW_LINES, preview, ident)
    )
    return 2, message


def main():
    payload = hooklib.read_payload()
    if payload is None:
        sys.stderr.write("guard_golden: stdin is not a JSON object; blocked\n")
        return 2
    code, message = evaluate(payload)
    if message:
        sys.stderr.write(message)
    return code


if __name__ == "__main__":
    sys.exit(main())
