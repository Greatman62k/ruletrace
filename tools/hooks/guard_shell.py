#!/usr/bin/env python3
"""Block dangerous shell commands issued by an agent.

Event: PreToolUse for execute_bash. Exit 0 allows. Exit 2 blocks and the
stderr text, which names the rule, is shown to the agent.

Rules: (a) destructive or publishing git, (b) GitHub write operations,
(c) credential exposure, (d) direct calls to model-provider endpoints,
(e) network isolation for code execution, (f) golden and fixture writes.
Only (f) can be approved, for one exact command, with
`golden_approve.py --command`.

Rule (e) also covers `cargo mutants|llvm-cov|tarpaulin|miri|careful`, which
run project tests. Rule (f) blocks `RT_DEMO_BLESS` only as an environment
assignment and `--bless` only as an argument of a command that is not a
read-only text tool, so searching for the words stays allowed.

This is best-effort pattern detection on a parsed command line, not a
sandbox. A python or perl one-liner that writes a file, a script that was
written earlier, variable indirection and similar tricks are not detected.
Heredoc bodies are not parsed as commands (unless the heredoc feeds a
shell or interpreter), but the raw text is still scanned for credential
references. Unparseable commands are blocked only when they contain a
dangerous word.
"""

import fnmatch
import glob
import os
import re
import shlex
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hooklib  # noqa: E402

PUNCT = ";&|()<>\n"
MAX_DEPTH = 3
SHELLS = ("sh", "bash", "zsh", "dash", "ksh")
INTERPRETERS = ("python", "perl", "ruby", "node", "php", "lua", "awk", "gawk")
ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*\+?=")
KEYWORDS = ("if", "then", "else", "elif", "do", "while", "until", "!", "{", "}")

WRAPPER = ["unshare", "-rn", "sh", "-c", 'ip link set lo up && exec "$@"', "sh"]

WRAPPERS = {
    "env": ("-u", "-C", "--unset", "--chdir"),
    "nice": ("-n", "--adjustment"),
    "time": ("-f", "-o", "--format", "--output"),
    "command": (),
    "exec": ("-a",),
    "nohup": (),
    "timeout": ("-s", "-k", "--signal", "--kill-after"),
    "builtin": (),
    "stdbuf": ("-i", "-o", "-e"),
    "setsid": (),
    "ionice": ("-c", "-n", "-p"),
    "xargs": ("-n", "-I", "-P", "-L", "-s", "-d", "-E", "-a"),
}

CRED_NAMES = (
    "MODEL_API_KEY",
    "OPENAI_API_KEY",
    "RULETRACE_OPENAI_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "ANTHROPIC_API_KEY",
)
CRED_REF = re.compile(r"\$\{?\s*[!#]?\s*(%s)\b" % "|".join(CRED_NAMES))
PROC_ENVIRON = re.compile(r"/proc/\S*environ")
PROVIDER_HOST = re.compile(
    r"api\.(meta\.ai|openai\.com|anthropic\.com)", re.IGNORECASE
)
NET_COMMANDS = (
    "curl",
    "wget",
    "http",
    "https",
    "xh",
    "nc",
    "ncat",
    "netcat",
    "telnet",
    "socat",
    "openssl",
)
EXEC_NAMES = ("ruletrace-ai", "ruletrace", "prize-check", "bracket-check")

GIT_VALUE_OPTS = ("-C", "-c", "--git-dir", "--work-tree", "--namespace")
GIT_REMOTE_WRITE = ("add", "set-url", "remove", "rm", "rename")
GH_ANY = ("workflow", "release", "secret", "variable", "gist", "label", "ruleset")
GH_WRITE = {
    "run": ("rerun", "cancel", "delete"),
    "pr": (
        "create",
        "merge",
        "close",
        "edit",
        "comment",
        "review",
        "ready",
        "reopen",
    ),
    "issue": (
        "create",
        "edit",
        "close",
        "comment",
        "delete",
        "reopen",
        "transfer",
    ),
    "repo": (
        "create",
        "delete",
        "edit",
        "rename",
        "fork",
        "archive",
        "unarchive",
    ),
    "cache": ("delete",),
}

FALLBACK_WORDS = (
    re.compile(
        r"\bgit\b[^\n]*\b(push|reset|clean|checkout|restore|branch|tag|rebase|"
        r"stash|commit|update-ref|reflog|gc|filter-branch|remote|config)\b"
    ),
    re.compile(r"\bgh\b"),
    re.compile(r"\bprintenv\b"),
    re.compile(r"\bsudo\b"),
    PROVIDER_HOST,
    re.compile(
        r"\bcargo\b[^\n]*\b(test|run|nextest|bench|mutants|llvm-cov|"
        r"tarpaulin|miri|careful)\b"
    ),
    re.compile(r"ruletrace-ai"),
    re.compile(r"\bunshare\b"),
    re.compile(r"--bless"),
    re.compile(r"RT_DEMO_BLESS\s*\+?="),
)

BLESS_ASSIGN = re.compile(r"^RT_DEMO_BLESS\+?=")
ASSIGN_BUILTINS = ("export", "declare", "typeset", "readonly", "local")
BLESS_READERS = (
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "echo",
    "printf",
    "cat",
    "less",
    "head",
    "tail",
    "awk",
    "gawk",
    "wc",
)

TEST_RUNNING_CARGO = ("mutants", "llvm-cov", "tarpaulin", "miri", "careful")
TEST_RUNNING_BINARIES = tuple("cargo-" + name for name in TEST_RUNNING_CARGO)
INFO_FLAGS = ("--version", "-V", "--help", "-h")
MUTANTS_LIST_FLAGS = ("--list", "--list-files", "--list-dirs")


class Finding:
    def __init__(self, rule, text, approvable=False):
        self.rule = rule
        self.text = text
        self.approvable = approvable


class Ctx:
    def __init__(self, root, cwds, home):
        self.root = root
        self.cwds = cwds
        self.home = home


class Segment:
    def __init__(self):
        self.words = []
        self.redirects = []
        self.inputs = []


def preprocess(text):
    """Join line continuations and turn unquoted backticks into parens."""
    text = text.replace("\\\n", "")
    out = []
    single = double = escaped = False
    opened = False
    for ch in text:
        if escaped:
            escaped = False
        elif ch == "\\" and not single:
            escaped = True
        elif ch == "'" and not double:
            single = not single
        elif ch == '"' and not single:
            double = not double
        elif ch == "`" and not single and not double:
            out.append(" ) " if opened else " ( ")
            opened = not opened
            continue
        out.append(ch)
    return "".join(out)


HEREDOC = re.compile(r"<<(-?)\s*(?:'([^']+)'|\"([^\"]+)\"|\\?([A-Za-z0-9_]+))")


def strip_heredocs(text):
    lines = text.split("\n")
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1
        found = None
        for match in HEREDOC.finditer(line):
            if line[match.start() - 1 : match.start()] == "<":
                continue
            found = match
        if not found:
            continue
        head = line[: found.start()].split()
        keep = bool(head) and os.path.basename(head[0]) in SHELLS + INTERPRETERS
        delim = found.group(2) or found.group(3) or found.group(4)
        dash = found.group(1) == "-"
        body = []
        while i < len(lines):
            cand = lines[i].lstrip("\t") if dash else lines[i]
            i += 1
            if cand == delim:
                break
            body.append(lines[i - 1])
        if keep:
            out.extend(body)
    return "\n".join(out)


def split_segments(text):
    lexer = shlex.shlex(text, posix=True, punctuation_chars=PUNCT)
    lexer.whitespace = " \t\r"
    lexer.whitespace_split = True
    lexer.commenters = ""
    segments = [Segment()]
    mode = None
    for tok in lexer:
        if tok and all(c in PUNCT for c in tok):
            i = 0
            while i < len(tok):
                c = tok[i]
                if c in "<>":
                    j = i + 1
                    while j < len(tok) and tok[j] in c + "|&":
                        j += 1
                    mode = "out" if c == ">" else "in"
                    i = j
                else:
                    segments.append(Segment())
                    mode = None
                    i += 1
            continue
        if mode == "out":
            segments[-1].redirects.append(tok)
        elif mode == "in":
            segments[-1].inputs.append(tok)
        else:
            segments[-1].words.append(tok)
        mode = None
    return segments


def substitutions(word):
    """Inner command strings of `$(...)` and backtick forms inside a word."""
    found = []
    i = 0
    while True:
        i = word.find("$(", i)
        if i < 0:
            break
        depth = 1
        j = i + 2
        while j < len(word) and depth:
            depth += {"(": 1, ")": -1}.get(word[j], 0)
            j += 1
        found.append(word[i + 2 : j - 1 if depth == 0 else j])
        i += 2
    parts = word.split("`")
    found.extend(parts[1::2])
    return found


def fallback_findings(text):
    for pattern in FALLBACK_WORDS:
        if pattern.search(text):
            return [
                Finding(
                    "unparseable",
                    "the command cannot be parsed and contains the dangerous "
                    "text %r; rewrite it as a plain command" % pattern.pattern,
                )
            ]
    return []


def options_split(args):
    """Return (options before `--`, operands) with `--` operands kept."""
    operands = []
    options = []
    rest = False
    for arg in args:
        if rest:
            operands.append(arg)
        elif arg == "--":
            rest = True
        elif arg.startswith("-") and len(arg) > 1:
            options.append(arg)
        else:
            operands.append(arg)
    return options, operands


def short_has(args, letters):
    for arg in args:
        if arg == "--":
            break
        if arg.startswith("-") and not arg.startswith("--") and len(arg) > 1:
            if set(arg[1:]) & set(letters):
                return True
    return False


def long_is(arg, name, minimum=2):
    """True when `arg` is `--name` or a unique-looking prefix of it."""
    if not arg.startswith("--"):
        return False
    body = arg[2:].split("=", 1)[0]
    return len(body) >= minimum and name.startswith(body)


def strip_prefix(words):
    """Remove env assignments and wrappers. Returns (core, bare_env, extra)."""
    i = 0
    bare_env = False
    extra = []
    while i < len(words):
        word = words[i]
        base = os.path.basename(word)
        if ASSIGN.match(word) or word in KEYWORDS:
            i += 1
            continue
        if base not in WRAPPERS:
            break
        value_opts = WRAPPERS[base]
        i += 1
        while i < len(words) and words[i].startswith("-") and len(words[i]) > 1:
            opt = words[i]
            i += 1
            if base == "env" and opt in ("-S", "--split-string") and i < len(words):
                extra.append(words[i])
                i += 1
            elif opt in value_opts and i < len(words):
                i += 1
        if base == "timeout" and i < len(words):
            i += 1
        if base == "env":
            while i < len(words) and ASSIGN.match(words[i]):
                i += 1
            if i >= len(words) and not extra:
                bare_env = True
    return words[i:], bare_env, extra


def git_parse(args):
    i = 0
    while i < len(args):
        arg = args[i]
        if arg in GIT_VALUE_OPTS:
            i += 2
        elif arg.startswith("-"):
            i += 1
        else:
            break
    return (args[i] if i < len(args) else None), args[i + 1 :], args[:i]


def check_git(args):
    sub, rest, before = git_parse(args)
    out = []

    def bad(text):
        out.append(Finding("a", "git-destructive: " + text))

    for idx, opt in enumerate(before):
        if opt == "-c" and idx + 1 < len(before):
            if before[idx + 1].lower().startswith("alias."):
                bad("`git -c alias.*` could hide a blocked subcommand")
    if sub is None:
        return out
    options, operands = options_split(rest)
    if sub == "push":
        bad("`git push` publishes; pushing is the user's decision")
    elif sub == "reset" and any(
        long_is(o, n) for o in rest for n in ("hard", "merge", "keep")
    ):
        bad("`git reset --hard|--merge|--keep` discards work")
    elif sub == "clean":
        bad("`git clean` deletes untracked files")
    elif sub == "checkout" and (
        "--" in rest
        or "." in operands
        or short_has(rest, "f")
        or any(long_is(o, n) for o in rest for n in ("force", "ours", "theirs"))
    ):
        bad("`git checkout` that overwrites working-tree files")
    elif sub == "switch" and (
        short_has(rest, "f") or any(long_is(o, "discard-changes") for o in rest)
    ):
        bad("`git switch` that discards changes")
    elif sub == "restore":
        bad("`git restore` overwrites working-tree files")
    elif sub == "branch" and (
        short_has(rest, "dD") or any(long_is(o, "delete") for o in rest)
    ):
        bad("deleting a branch")
    elif sub == "tag" and (
        short_has(rest, "d") or any(long_is(o, "delete") for o in rest)
    ):
        bad("deleting a tag")
    elif sub == "rebase":
        bad("`git rebase` rewrites history")
    elif sub == "stash" and operands[:1] and operands[0] in ("drop", "clear"):
        bad("`git stash %s` destroys stashed work" % operands[0])
    elif sub == "commit" and any(long_is(o, "amend") for o in rest):
        bad("`git commit --amend` rewrites a commit")
    elif sub == "update-ref" and (
        short_has(rest, "d") or any(long_is(o, "delete") for o in rest)
    ):
        bad("`git update-ref -d` deletes a ref")
    elif sub == "reflog" and operands[:1] and operands[0] in ("expire", "delete"):
        bad("`git reflog %s` destroys recovery data" % operands[0])
    elif sub == "gc" and any(o.startswith("--prune") for o in rest):
        bad("`git gc --prune` deletes unreachable objects")
    elif sub in ("filter-branch", "filter-repo"):
        bad("`git %s` rewrites history" % sub)
    elif sub == "remote" and operands[:1] and operands[0] in GIT_REMOTE_WRITE:
        bad("`git remote %s` changes remotes" % operands[0])
    elif sub == "config":
        reads = ("--get", "--get-all", "--get-regexp", "--list", "--get-urlmatch")
        writes = (
            "--unset",
            "--unset-all",
            "--add",
            "--replace-all",
            "--edit",
            "--rename-section",
            "--remove-section",
        )
        has_read = short_has(rest, "l") or any(o in reads for o in options)
        has_write = short_has(rest, "e") or any(o in writes for o in options)
        subcommand_write = operands[:1] and operands[0] in (
            "set",
            "unset",
            "edit",
            "rename-section",
            "remove-section",
        )
        if has_write or subcommand_write or (len(operands) >= 2 and not has_read):
            bad("`git config` write")
    return out


def positionals(args, value_opts):
    out = []
    i = 0
    while i < len(args):
        arg = args[i]
        if arg in value_opts:
            i += 2
            continue
        if not arg.startswith("-"):
            out.append(arg)
        i += 1
    return out


def check_gh(args):
    out = []

    def bad(text):
        out.append(Finding("b", "gh-write: " + text))

    pos = positionals(args, ("-R", "--repo", "--hostname"))
    if not pos:
        return out
    group = pos[0]
    sub = pos[1] if len(pos) > 1 else None
    if group in GH_ANY:
        bad("`gh %s` can change repository state" % group)
    elif group in GH_WRITE and sub in GH_WRITE[group]:
        bad("`gh %s %s` is a GitHub write operation" % (group, sub))
    elif group == "api":
        rest = args[args.index("api") + 1 :]
        for idx, arg in enumerate(rest):
            method = None
            if arg in ("-X", "--method") and idx + 1 < len(rest):
                method = rest[idx + 1]
            elif arg.startswith("--method="):
                method = arg.split("=", 1)[1]
            elif arg.startswith("-X") and len(arg) > 2:
                method = arg[2:]
            if method is not None and method.upper() != "GET":
                bad("`gh api` with method %s" % method)
            implied = arg in ("-f", "-F", "--field", "--raw-field", "--input")
            implied = implied or arg.startswith(("--field=", "--raw-field=", "--input="))
            implied = implied or (
                arg.startswith(("-f", "-F")) and not arg.startswith("--") and len(arg) > 2
            )
            if implied:
                bad("`gh api` with a field or input option implies POST")
    elif group == "auth":
        if sub == "token":
            out.append(Finding("c", "credential-exposure: `gh auth token`"))
        elif sub == "status" and (
            short_has(args, "t") or any(a.startswith("--show-token") for a in args)
        ):
            out.append(Finding("c", "credential-exposure: `gh auth status` token"))
    return out


def cargo_subcommand(args):
    i = 0
    while i < len(args):
        arg = args[i]
        if arg in ("-C", "-Z", "--config", "--color", "--manifest-path"):
            i += 2
        elif arg.startswith("+") or arg.startswith("-"):
            i += 1
        else:
            return arg, args[i + 1 :]
    return None, []


def check_cargo(args, wrapped):
    sub, rest = cargo_subcommand(args)
    if wrapped or sub is None:
        return []
    if "--" in rest:
        rest = rest[: rest.index("--")]
    executes = False
    if sub in ("test", "t", "bench"):
        executes = "--no-run" not in rest
    elif sub in ("run", "r"):
        executes = True
    elif sub == "nextest":
        executes = not (rest and rest[0] == "list")
    elif sub in TEST_RUNNING_CARGO:
        executes = not info_only(sub, rest)
    if executes:
        return [Finding("e", network_isolation_text("cargo %s" % sub))]
    return []


def info_only(sub, rest):
    """True when a test-running cargo tool only prints information.

    Every other form is treated as executing project tests, including
    `cargo mutants --check` and `cargo miri setup`.
    """
    if any(arg in INFO_FLAGS for arg in rest):
        return True
    return sub == "mutants" and any(arg in MUTANTS_LIST_FLAGS for arg in rest)


def bless_flag_exempt(base, args):
    """Read-only text tools may carry `--bless` as plain text."""
    if base in BLESS_READERS:
        return True
    return (
        base == "sed"
        and (short_has(args, "n") or any(long_is(a, "quiet") or a == "--silent" for a in args))
        and not short_has(args, "i")
        and not any(long_is(a, "in-place") for a in args)
    )


def network_isolation_text(what):
    return (
        "network-isolation: `%s` executes project code and must run inside the "
        "approved wrapper, exactly: unshare -rn sh -c 'ip link set lo up && "
        'exec "$@"\' sh <command...>' % what
    )


def home_expand(word, home):
    if word == "~" or word.startswith("~/"):
        word = home + word[1:]
    for form in ("${HOME}", "$HOME"):
        if word.startswith(form):
            word = home + word[len(form) :]
    return word


def secret_file_hit(word, home):
    path = home_expand(word, home)
    base = os.path.basename(path.rstrip("/"))
    if base == ".modal.toml" or path.endswith(".config/gh/hosts.yml"):
        return True
    kiro = os.path.join(home, ".kiro") + "/"
    if os.path.normpath(path).startswith(kiro):
        lower = base.lower()
        return fnmatch.fnmatch(lower, "auth*") or fnmatch.fnmatch(lower, "*token*")
    return False


def targets_for(core):
    """Return (path, mode) pairs for files a command would modify."""
    base = os.path.basename(core[0])
    args = core[1:]
    options, operands = options_split(args)
    pairs = []
    if base == "tee":
        pairs = [(p, "exact") for p in operands]
    elif base == "sed" and (short_has(args, "i") or "--in-place" in " ".join(options)):
        pairs = [(p, "exact") for p in operands]
    elif base in ("cp", "install", "ln", "rsync", "scp"):
        dest = None
        for idx, arg in enumerate(args):
            if arg in ("-t", "--target-directory") and idx + 1 < len(args):
                dest = args[idx + 1]
            elif arg.startswith("--target-directory="):
                dest = arg.split("=", 1)[1]
        if dest is None and operands:
            dest = operands[-1]
        if dest is not None:
            pairs = [(dest, "dest")]
    elif base == "mv":
        pairs = [(p, "destructive") for p in operands[:-1]]
        pairs += [(operands[-1], "dest")] if operands else []
    elif base in ("rm", "unlink", "shred", "rmdir"):
        pairs = [(p, "destructive") for p in operands]
    elif base in ("truncate", "touch", "patch"):
        pairs = [(p, "exact") for p in operands]
    elif base == "dd":
        pairs = [(a[3:], "exact") for a in args if a.startswith("of=")]
    return pairs


def path_findings(ctx, path, mode):
    if ctx.root is None or "$" in path or not path:
        return False
    candidates = []
    for base in ctx.cwds:
        expanded = os.path.expanduser(path)
        candidates.append(path)
        if any(c in expanded for c in "*?["):
            full = os.path.join(base, expanded)
            candidates.extend(glob.glob(full))
    for candidate in candidates:
        for base in ctx.cwds:
            for rel in hooklib.rel_candidates(ctx.root, base, candidate):
                if hooklib.is_protected(rel):
                    return True
                if mode in ("dest", "destructive") and hooklib.is_protected(
                    rel + "/_"
                ):
                    return True
                if (
                    mode == "destructive"
                    and os.path.isdir(os.path.join(ctx.root, rel))
                    and hooklib.is_protected_ancestor(rel)
                ):
                    return True
    return False


def golden_text(path):
    return (
        "golden-write: the command writes the protected path %s. Goldens, "
        "fixtures and recordings change only by an explicit, reviewed edit" % path
    )


BLESS_TEXT = "golden-write: RT_DEMO_BLESS regenerates goldens"
BLESS_FLAG_TEXT = "golden-write: `--bless` regenerates goldens"


def analyze_words(words, ctx, depth, wrapped):
    core, bare_env, extra = strip_prefix(words)
    out = []
    for word in words[: len(words) - len(core)]:
        if BLESS_ASSIGN.match(word):
            out.append(Finding("f", BLESS_TEXT, True))
    for text in extra:
        out.extend(analyze_command(text, ctx, depth + 1, wrapped))
    if bare_env:
        out.append(Finding("c", "credential-exposure: bare `env` prints the environment"))
    command = os.path.basename(core[0]) if core else ""
    if not bless_flag_exempt(command, core[1:]):
        for word in words:
            if word == "--bless" or word.startswith("--bless="):
                out.append(Finding("f", BLESS_FLAG_TEXT, True))
    if not core:
        return out
    base = os.path.basename(core[0])
    args = core[1:]

    if base in ASSIGN_BUILTINS and any(BLESS_ASSIGN.match(a) for a in args):
        out.append(Finding("f", BLESS_TEXT, True))
    if base in ("sudo", "doas"):
        out.append(Finding("sudo", "sudo: privilege escalation is blocked"))
        return out
    if base == "unshare":
        if core[: len(WRAPPER)] == WRAPPER and len(core) > len(WRAPPER):
            return out + analyze_words(core[len(WRAPPER) :], ctx, depth, True)
        out.append(
            Finding(
                "e",
                "network-isolation: a weaker or different unshare wrapper is "
                "not accepted (loopback must be brought up for stub tests). "
                "Use exactly: unshare -rn sh -c 'ip link set lo up && exec "
                '"$@"\' sh <command...>',
            )
        )
        return out
    if base in SHELLS:
        for idx, arg in enumerate(args):
            if arg.startswith("-") and not arg.startswith("--") and "c" in arg:
                if idx + 1 < len(args):
                    out.extend(analyze_command(args[idx + 1], ctx, depth + 1, wrapped))
                break
    elif base == "eval":
        out.extend(analyze_command(" ".join(args), ctx, depth + 1, wrapped))
    elif base == "find":
        for idx, arg in enumerate(args):
            if arg in ("-exec", "-execdir", "-ok", "-okdir"):
                inner = []
                for word in args[idx + 1 :]:
                    if word in (";", "+"):
                        break
                    inner.append(word)
                out.extend(analyze_words(inner, ctx, depth + 1, wrapped))
    elif base in ("cd", "pushd") and args:
        target = [a for a in args if not a.startswith("-")]
        if target:
            expanded = os.path.expanduser(target[-1])
            first = os.path.normpath(os.path.join(ctx.cwds[0], expanded))
            last = os.path.normpath(os.path.join(ctx.cwds[-1], expanded))
            ctx.cwds.extend(d for d in (first, last) if d not in ctx.cwds)
    elif base == "git":
        out.extend(check_git(args))
    elif base == "gh":
        out.extend(check_gh(args))
    elif base == "cargo":
        out.extend(check_cargo(args, wrapped))
    elif base in TEST_RUNNING_BINARIES:
        out.extend(check_cargo([base[len("cargo-") :]] + args, wrapped))
    elif base == "printenv":
        out.append(Finding("c", "credential-exposure: `printenv`"))
    elif base == "export" and (not args or "-p" in args):
        out.append(Finding("c", "credential-exposure: `export` lists the environment"))
    elif base == "set" and not args:
        out.append(Finding("c", "credential-exposure: bare `set` lists variables"))
    elif base in ("declare", "typeset") and (not args or short_has(args, "px")):
        out.append(Finding("c", "credential-exposure: `%s` lists variables" % base))

    if base.startswith("python") or base in INTERPRETERS:
        for word in args:
            for name in CRED_NAMES:
                if name in word:
                    out.append(
                        Finding("c", "credential-exposure: %s referenced by %s" % (name, base))
                    )

    if base in NET_COMMANDS or base.startswith("python") or base in INTERPRETERS:
        if PROVIDER_HOST.search(" ".join(core)):
            out.append(
                Finding(
                    "d",
                    "provider-network: direct calls to model-provider endpoints "
                    "are user-run, not agent-run",
                )
            )

    if base == "ruletrace-ai" and not wrapped:
        live = any(a in ("propose", "models") for a in args)
        if live and "--dry-run" not in args:
            out.append(
                Finding(
                    "d",
                    "provider-network: live `ruletrace-ai propose|models` is "
                    "user-run; use --dry-run or the isolation wrapper",
                )
            )
    if "/" in core[0] and base in EXEC_NAMES and not wrapped:
        out.append(Finding("e", network_isolation_text(core[0])))

    for path, mode in targets_for(core):
        if path_findings(ctx, path, mode):
            out.append(Finding("f", golden_text(path), True))
    return out


def analyze_command(text, ctx, depth=0, wrapped=False):
    out = []
    if CRED_REF.search(text):
        out.append(Finding("c", "credential-exposure: a credential variable is referenced"))
    if PROC_ENVIRON.search(text):
        out.append(Finding("c", "credential-exposure: /proc/*/environ exposes secrets"))
    if ".modal.toml" in text or ".config/gh/hosts.yml" in text:
        out.append(Finding("c", "credential-exposure: credential file access"))
    if depth > MAX_DEPTH:
        return out + fallback_findings(text)
    try:
        segments = split_segments(strip_heredocs(preprocess(text)))
    except ValueError:
        return out + fallback_findings(text)
    for seg in segments:
        for target in seg.redirects:
            if path_findings(ctx, target, "exact"):
                out.append(Finding("f", golden_text(target), True))
        for path in seg.inputs + seg.words + seg.redirects:
            if secret_file_hit(path, ctx.home):
                out.append(Finding("c", "credential-exposure: credential file %s" % path))
        for word in seg.words:
            if "$(" in word or "`" in word:
                for inner in substitutions(word):
                    out.extend(analyze_command(inner, ctx, depth + 1, wrapped))
        if seg.words:
            out.extend(analyze_words(seg.words, ctx, depth, wrapped))
    return out


def decide(payload, now=None):
    """Return (exit_code, stderr_text) for one PreToolUse payload."""
    if payload.get("tool_name") != "execute_bash":
        return 0, ""
    tool_input = payload.get("tool_input")
    command = tool_input.get("command") if isinstance(tool_input, dict) else None
    if not isinstance(command, str):
        return 2, "guard_shell: execute_bash call without a string command\n"
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not os.path.isdir(cwd):
        return 2, "guard_shell: payload has no usable cwd; blocked\n"
    cwds = [os.path.realpath(cwd)]
    tool_cwd = tool_input.get("cwd")
    if isinstance(tool_cwd, str) and os.path.isdir(tool_cwd):
        cwds.append(os.path.realpath(tool_cwd))
    try:
        root = hooklib.repo_root(cwd)
    except RuntimeError:
        root = None
    ctx = Ctx(root, cwds, os.path.expanduser("~"))
    findings = analyze_command(command, ctx)
    if not findings:
        return 0, ""
    blocking = [f for f in findings if not f.approvable]
    if not blocking and root is not None:
        digest = hooklib.sha256_bytes(command.encode("utf-8"))
        directory = hooklib.store_dir(cwd)
        try:
            with hooklib.locked(directory):
                if hooklib.consume_approval(
                    directory,
                    lambda e: e.get("kind") == "shell"
                    and e.get("command_sha256") == digest,
                    now,
                ):
                    return 0, ""
        except hooklib.CorruptStore as exc:
            return 2, "guard_shell: approval store problem, blocked: %s\n" % exc
        ident = hooklib.sha256_bytes(("shell|" + command).encode("utf-8"))[:12]
        hooklib.write_json_atomic(
            os.path.join(directory, "pending", ident + ".json"),
            {
                "id": ident,
                "kind": "shell",
                "command": command,
                "command_sha256": digest,
                "created_at": int(time.time() if now is None else now),
            },
        )
    lines = ["guard_shell: command blocked."]
    for finding in findings:
        lines.append("  rule (%s) %s" % (finding.rule, finding.text))
    if not blocking:
        lines.append(
            "Only the user can approve this exact command. Ask them to run:\n"
            "  tools/hooks/golden_approve.py --command %s\n"
            "then repeat the identical command once." % shlex.quote(command)
        )
    return 2, "\n".join(lines) + "\n"


def main():
    payload = hooklib.read_payload()
    if payload is None:
        sys.stderr.write("guard_shell: stdin is not a JSON object; blocked\n")
        return 2
    code, message = decide(payload)
    if message:
        sys.stderr.write(message)
    return code


if __name__ == "__main__":
    sys.exit(main())
