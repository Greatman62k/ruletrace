"""Controlled mutants of the hook scripts.

Each mutant is applied to a throwaway copy of tools/hooks, so the real
scripts are never edited. A mutant is killed when the named tests fail.

Run: python3 tools/hooks/tests/mutants.py
"""

import os
import shutil
import subprocess
import sys
import tempfile

HOOKS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, file, original text, replacement, test classes that must fail)
MUTANTS = [
    ("golden: old hash not bound", "guard_golden.py",
     '                and e.get("old_sha256") == old_hash\n', "", "GuardGolden"),
    ("golden: new hash not bound", "guard_golden.py",
     '                and e.get("new_sha256") == new_hash,\n', "                and True,\n", "GuardGolden"),
    ("golden: path not bound", "guard_golden.py",
     '                and e.get("path") == rel\n', "", "GuardGolden"),
    ("golden: approval never consumed", "hooklib.py",
     '            entry["used"] = True\n', '            entry["used"] = False\n', "GuardGolden"),
    ("golden: expiry ignored", "hooklib.py",
     "        if expires <= now:\n", "        if False:\n", "GuardGolden"),
    ("golden: symlinks not resolved", "hooklib.py",
     "(absolute, os.path.realpath(joined), os.path.realpath(absolute))",
     "(absolute,)", "GuardGolden"),
    ("golden: ambiguous str_replace accepted", "guard_golden.py",
     "if count > 1 and not every:", "if count > 1 and False:", "GuardGolden"),
    ("golden: append hash ignores old content", "guard_golden.py",
     'return (current or b"") + text.encode("utf-8")', 'return text.encode("utf-8")', "GuardGolden"),
    ("golden: missing path allowed", "guard_golden.py",
     'return 2, "guard_golden: %s call without a path field is blocked\\n" % tool',
     "return 0, ''", "GuardGolden"),
    ("golden: fixtures pattern dropped", "hooklib.py",
     '    "tests/fixtures/**",\n', "", "GuardGolden"),
    ("golden: no-op write blocked too", "guard_golden.py",
     "if proposed != DELETED and current is not None and proposed == current:", "if False:", "GuardGolden"),
    ("approve: TTY not required", "golden_approve.py",
     "    if not sys.stdin.isatty():", "    if False:", "GoldenApproveCli"),
    ("approve: any answer accepted", "golden_approve.py",
     '== "yes"', '!= "never-matches-xyz"', "GoldenApproveCli"),
    ("approve: minutes cap removed", "golden_approve.py",
     "or not 1 <= minutes <= MAX_MINUTES", "or minutes < 1", "GoldenApproveCli"),
    ("shell: push rule removed", "guard_shell.py",
     'if sub == "push":', 'if sub == "push-disabled":', "GuardShellTable"),
    ("shell: git -C value not skipped", "guard_shell.py",
     'GIT_VALUE_OPTS = ("-C", "-c", "--git-dir", "--work-tree", "--namespace")',
     "GIT_VALUE_OPTS = ()", "GuardShellTable"),
    ("shell: sh -c not recursed", "guard_shell.py",
     "    if base in SHELLS:\n", "    if base in ():\n", "GuardShellTable"),
    ("shell: weak wrapper accepted", "guard_shell.py",
     "core[: len(WRAPPER)] == WRAPPER and len(core) > len(WRAPPER)",
     "core[:2] == WRAPPER[:2] and len(core) > 6", "GuardShellTable"),
    ("shell: env assignments not skipped", "guard_shell.py",
     "if ASSIGN.match(word) or word in KEYWORDS:", "if word in KEYWORDS:", "GuardShellTable"),
    ("shell: --no-run after -- counts", "guard_shell.py",
     '    if "--" in rest:\n        rest = rest[: rest.index("--")]\n', "", "GuardShellTable"),
    ("shell: gh api field flags ignored", "guard_shell.py",
     "            if implied:\n", "            if False:\n", "GuardShellTable"),
    ("shell: GH_TOKEN reference ignored", "guard_shell.py",
     '    "GH_TOKEN",\n', "", "GuardShellTable"),
    ("shell: live ruletrace-ai check inverted", "guard_shell.py",
     'if live and "--dry-run" not in args:', 'if live and "--dry-run" in args:', "GuardShellTable"),
    ("shell: unparseable fallback disabled", "guard_shell.py",
     "    for pattern in FALLBACK_WORDS:\n", "    for pattern in ():\n", "GuardShellTable"),
    ("shell: command approval matches any command", "guard_shell.py",
     'and e.get("command_sha256") == digest', "and True", "GuardShellGoldenWrites"),
    ("shell: directory removal not detected", "guard_shell.py",
     "                    mode == \"destructive\"\n", "                    False\n", "GuardShellGoldenWrites"),
    ("shell: redirect targets not checked", "guard_shell.py",
     "        for target in seg.redirects:\n            if path_findings",
     "        for target in ():\n            if path_findings", "GuardShellGoldenWrites"),
    ("shell: cd not tracked", "guard_shell.py",
     'elif base in ("cd", "pushd") and args:', 'elif base in ("cd-disabled",) and args:', "GuardShellGoldenWrites"),
    ("shell: bare env allowed", "guard_shell.py",
     "    if bare_env:\n", "    if False:\n", "GuardShellTable"),
    ("fmt: difference ignored", "fmt_check.py",
     "    if proc.returncode == 0:\n", "    if True:\n", "FmtCheck"),
    ("fmt: other files' hunks not filtered", "fmt_check.py",
     "keep = os.path.realpath(match.group(1)) == target", "keep = True", "FmtCheck"),
    ("review: HEAD move ignored", "review_integrity.py",
     'if baseline["head"] != current["head"]:', "if False:", "ReviewIntegrity"),
    ("review: content change ignored", "review_integrity.py",
     "        if old[rel] != new[rel]:", "        if False:", "ReviewIntegrity"),
    ("review: reports not ignored", "review_integrity.py",
     'ALWAYS_IGNORED = ("docs/reviews/*.md",)', "ALWAYS_IGNORED = ()", "ReviewIntegrity"),
    ("review: baseline age ignored", "review_integrity.py",
     "if age >= MAX_AGE_SECONDS or age < -300:", "if False:", "ReviewIntegrity"),
    ("review: young baseline overwritten", "review_integrity.py",
     "        if age < MAX_AGE_SECONDS:\n", "        if False:\n", "ReviewIntegrity"),
    ("review guard: missing baseline allowed", "guard_review_report.py",
     "    except (ri.IntegrityError, RuntimeError, OSError) as exc:\n        return 2, (",
     "    except (ri.IntegrityError, RuntimeError, OSError) as exc:\n        return 0, '' or (", "ReviewIntegrity"),
    ("review guard: tree changes ignored", "guard_review_report.py",
     "    if problems:\n", "    if False:\n", "ReviewIntegrity"),
    ("stop: exit code follows failures", "stop_report.py",
     "    print(\"\\n\".join(lines))\n    return 0\n",
     "    print(\"\\n\".join(lines))\n    return 1 if any(l.startswith('FAIL') for l in lines) else 0\n", "StopReport"),
    ("stop: justification check dropped", "stop_report.py",
     '        wanted.append("check-justification")\n', "        pass\n", "StopReport"),
    ("stop: golden notice dropped", "stop_report.py",
     "    if hooklib.is_protected(path):\n", "    if False:\n", "StopReport"),
    ("stop: baseline verify dropped", "stop_report.py",
     "    if baseline is not None:\n", "    if False:\n", "StopReport"),
]


def run(selected):
    survivors = []
    for name, filename, old, new, classes in MUTANTS:
        if selected and not any(s in name for s in selected):
            continue
        with tempfile.TemporaryDirectory(prefix="mutant-") as tmp:
            copy = os.path.join(tmp, "hooks")
            shutil.copytree(HOOKS, copy, ignore=shutil.ignore_patterns("__pycache__"))
            path = os.path.join(copy, filename)
            with open(path) as handle:
                text = handle.read()
            if text.count(old) != 1:
                print("BROKEN  %s: pattern occurs %d times" % (name, text.count(old)))
                survivors.append(name)
                continue
            with open(path, "w") as handle:
                handle.write(text.replace(old, new))
            proc = subprocess.run(
                [sys.executable, "-m", "unittest", "test_hooks." + classes],
                cwd=os.path.join(copy, "tests"),
                capture_output=True,
                text=True,
            )
            failed = [
                line.split()[1]
                for line in proc.stderr.splitlines()
                if line.startswith(("FAIL:", "ERROR:"))
            ]
            if proc.returncode != 0 and failed:
                print("KILLED  %s by %s (+%d more)" % (name, failed[0], len(failed) - 1))
            else:
                print("SURVIVED %s" % name)
                survivors.append(name)
    return survivors


if __name__ == "__main__":
    left = run(sys.argv[1:])
    print("%d mutant(s) not killed" % len(left))
    sys.exit(1 if left else 0)
