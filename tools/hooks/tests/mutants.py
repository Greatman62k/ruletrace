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
     "        sys.stderr.write(\"stop-report: cannot write the report file: %s\\n\" % exc)\n    return 0\n",
     "        sys.stderr.write(\"stop-report: cannot write the report file: %s\\n\" % exc)\n    return 1 if any(l.startswith('FAIL') for l in lines) else 0\n", "StopReport"),
    ("stop: justification check dropped", "stop_report.py",
     '        wanted.append("check-justification")\n', "        pass\n", "StopReport"),
    ("stop: golden notice dropped", "stop_report.py",
     "    if hooklib.is_protected(path):\n", "    if False:\n", "StopReport"),
    ("stop: baseline verify dropped", "stop_report.py",
     "    if baseline is not None:\n", "    if False:\n", "StopReport"),
    # Persistent reports, FORMAT section, rule (e) additions, rule (f) narrowing.
    ("fmt state: clean path keeps its entry", "fmt_check.py",
     "        elif rel in entries:\n", "        elif False:\n", "FmtCheck"),
    ("fmt state: entries not merged", "fmt_check.py",
     "        entries, _ = hooklib.load_fmt_warnings(directory)\n", "        entries = {}\n", "FmtCheck"),
    ("fmt state: timestamp blank", "fmt_check.py",
     '"timestamp": hooklib.utc_now()', '"timestamp": ""', "FmtCheck"),
    ("fmt state: absolute key", "fmt_check.py",
     'rel = os.path.relpath(full, root).replace(os.sep, "/")', "rel = full", "FmtCheck"),
    ("fmt state: write failure is fatal", "fmt_check.py",
     "        except (OSError, RuntimeError) as exc:\n            sys.stderr.write(\n                \"fmt_check: cannot",
     "        except RuntimeError as exc:\n            sys.stderr.write(\n                \"fmt_check: cannot", "FmtCheck"),
    ("fmt state: deleted file keeps its entry", "fmt_check.py",
     '    if not os.path.isfile(full):\n        return 0, "", ("clear", rel, "")',
     '    if not os.path.isfile(full):\n        return 0, "", None', "FmtCheck"),
    ("fmt state: inconclusive run clears the entry", "fmt_check.py",
     '        return 0, "fmt_check: %s\\n" % payload_out, None',
     '        return 0, "fmt_check: %s\\n" % payload_out, ("clear", rel, "")', "FmtCheck"),
    ("fmt state: no new entry for a syntax error", "fmt_check.py",
     '    return 1, text, ("set", rel, text)', "    return 1, text, None", "FmtCheck"),
    ("report files: not written atomically", "hooklib.py",
     '        with open(tmp, "w") as handle:\n            handle.write(text)\n        os.replace(tmp, path)\n',
     '        with open(path, "w") as handle:\n            handle.write(text)\n', "FmtCheck,StopReport"),
    ("rustfmt: --check dropped", "fmt_check.py",
     '"--color", "never", "--check", full]', '"--color", "never", full]', "FmtCheck,StopReport"),
    ("rustfmt: timeout dropped", "fmt_check.py",
     "            timeout=TIMEOUT,\n", "", "StopReport"),
    ("stop file: nothing written", "stop_report.py",
     'hooklib.STOP_REPORT), text + "\\n"', 'hooklib.STOP_REPORT), ""', "StopReport"),
    ("stop file: appended instead of replaced", "stop_report.py",
     '        hooklib.write_text_atomic(\n            os.path.join(directory, hooklib.STOP_REPORT), text + "\\n"\n        )\n',
     '        os.makedirs(directory, exist_ok=True)\n        with open(os.path.join(directory, hooklib.STOP_REPORT), "a") as handle:\n            handle.write(text + "\\n")\n', "StopReport"),
    ("stop file: write failure is fatal", "stop_report.py",
     "    except (OSError, RuntimeError) as exc:\n        sys.stderr.write(\"stop-report: cannot",
     "    except RuntimeError as exc:\n        sys.stderr.write(\"stop-report: cannot", "StopReport"),
    ("stop format: unclean file labelled OK", "stop_report.py",
     'lines.append("WARN format %s: not rustfmt-clean" % path)',
     'lines.append("OK   format %s: not rustfmt-clean" % path)', "StopReport"),
    ("stop format: unreadable file labelled OK", "stop_report.py",
     'lines.append("WARN format %s: rustfmt could not check it" % path)',
     'lines.append("OK   format %s: rustfmt could not check it" % path)', "StopReport"),
    ("stop format: limit raised", "stop_report.py",
     "MAX_FORMAT_FILES = 40", "MAX_FORMAT_FILES = 50", "StopReport"),
    ("stop format: untracked files not listed", "stop_report.py",
     '"--untracked-files=all"', '"--untracked-files=no"', "StopReport"),
    ("stop format: deleted files checked", "stop_report.py",
     " and os.path.isfile(os.path.join(root, path))", "", "StopReport"),
    ("stop format: pending entries not listed", "stop_report.py",
     "    for path in sorted(pending):\n", "    for path in ():\n", "StopReport"),
    ("stop format: timeout ends the section", "stop_report.py",
     '            lines.append("WARN format %s: %s" % (path, detail))\n            continue\n',
     '            lines.append("WARN format %s: %s" % (path, detail))\n            break\n', "StopReport"),
    ("stop format: missing rustfmt not reported", "stop_report.py",
     '            lines.append("WARN %s" % detail)\n            break\n', "            continue\n", "StopReport"),
    ("stop format: damaged state not reported", "stop_report.py",
     "    if problem:\n", "    if False:\n", "StopReport"),
    ("shell (e): cargo info flags ignored", "guard_shell.py",
     "    if any(arg in INFO_FLAGS for arg in rest):\n        return True\n", "", "GuardShellTable"),
    ("shell (e): any test-running cargo tool allowed", "guard_shell.py",
     "        executes = not info_only(sub, rest)\n", "        executes = False\n", "GuardShellTable"),
    ("shell (e): mutants --list blocked", "guard_shell.py",
     'return sub == "mutants" and any(arg in MUTANTS_LIST_FLAGS for arg in rest)', "return False", "GuardShellTable"),
    ("shell (e): mutants --list allowed for every tool", "guard_shell.py",
     'return sub == "mutants" and any(', "return any(", "GuardShellTable"),
    ("shell (e): cargo-* binaries not checked", "guard_shell.py",
     "    elif base in TEST_RUNNING_BINARIES:\n", "    elif base in ():\n", "GuardShellTable"),
    ("shell (e): fallback misses new tools", "guard_shell.py",
     "(test|run|nextest|bench|mutants|llvm-cov|", "(test|run|nextest|bench|", "GuardShellTable"),
    ("shell (f): assignment before a command not checked", "guard_shell.py",
     "        if BLESS_ASSIGN.match(word):\n", "        if False:\n", "GuardShellGoldenWrites"),
    ("shell (f): export assignment not checked", "guard_shell.py",
     "    if base in ASSIGN_BUILTINS and any(", "    if False and any(", "GuardShellGoldenWrites"),
    ("shell (f): assignment finding not approvable", "guard_shell.py",
     "        if BLESS_ASSIGN.match(word):\n            out.append(Finding(\"f\", BLESS_TEXT, True))",
     "        if BLESS_ASSIGN.match(word):\n            out.append(Finding(\"f\", BLESS_TEXT, False))", "GuardShellGoldenWrites"),
    ("shell (f): += not an assignment word", "guard_shell.py",
     'ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*\\+?=")', 'ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")', "GuardShellGoldenWrites"),
    ("shell (f): += not matched as bless assignment", "guard_shell.py",
     'BLESS_ASSIGN = re.compile(r"^RT_DEMO_BLESS\\+?=")', 'BLESS_ASSIGN = re.compile(r"^RT_DEMO_BLESS=")', "GuardShellGoldenWrites"),
    ("shell (f): text mention blocks again", "guard_shell.py",
     "    if PROC_ENVIRON.search(text):\n",
     '    if "RT_DEMO_BLESS" in text:\n        out.append(Finding("f", BLESS_TEXT, True))\n    if PROC_ENVIRON.search(text):\n', "GuardShellGoldenWrites"),
    ("shell (f): read-only tools not exempt", "guard_shell.py",
     "    if base in BLESS_READERS:\n        return True\n", "", "GuardShellGoldenWrites"),
    ("shell (f): every sed exempt", "guard_shell.py",
     '        base == "sed"\n        and (', '        base == "sed"\n        and (True or ', "GuardShellGoldenWrites"),
    ("shell (f): sed in-place exempt", "guard_shell.py",
     '        and not short_has(args, "i")\n', "", "GuardShellGoldenWrites"),
    ("shell (f): --bless flag never checked", "guard_shell.py",
     "    if not bless_flag_exempt(command, core[1:]):\n", "    if False:\n", "GuardShellGoldenWrites"),
    ("shell (f): --bless in wrapper options missed", "guard_shell.py",
     '        for word in words:\n            if word == "--bless"', '        for word in core:\n            if word == "--bless"', "GuardShellGoldenWrites"),
    ("shell (f): fallback misses unparseable assignment", "guard_shell.py",
     '    re.compile(r"RT_DEMO_BLESS\\s*\\+?="),\n', "", "GuardShellGoldenWrites"),
]


def _tool_mutants():
    full = ("mutants", "llvm-cov", "tarpaulin", "miri", "careful")
    out = []
    for tool in full:
        kept = tuple(t for t in full if t != tool)
        out.append((
            "shell (e): cargo %s not covered" % tool,
            "guard_shell.py",
            "TEST_RUNNING_CARGO = (%s)" % ", ".join('"%s"' % t for t in full),
            "TEST_RUNNING_CARGO = (%s,)" % ", ".join('"%s"' % t for t in kept),
            "GuardShellTable",
        ))
    return out


MUTANTS += _tool_mutants()


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
                [sys.executable, "-m", "unittest"]
                + ["test_hooks." + name for name in classes.split(",")],
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
