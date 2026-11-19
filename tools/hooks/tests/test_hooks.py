"""Tests for the RuleTrace Kiro hook scripts.

Every test builds throwaway git repositories under a temporary directory
and runs the scripts as subprocesses with a JSON payload on stdin, as Kiro
does. The real repository is never touched. Expected values are written by
hand or derived with plain hashlib, not with the hook helper library.

Run: python3 -m unittest discover -s tools/hooks/tests
"""

import calendar
import hashlib
import json
import os
import pty
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

sys.dont_write_bytecode = True
HOOKS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HOOKS)
import golden_approve  # noqa: E402
import review_integrity as ri  # noqa: E402

WRAPPER = "unshare -rn sh -c 'ip link set lo up && exec \"$@\"' sh"


def sha(data):
    if isinstance(data, str):
        data = data.encode()
    return hashlib.sha256(data).hexdigest()


class RepoCase(unittest.TestCase):
    FILES = {
        "README.md": "readme\n",
        "src/lib.rs": "pub fn f() {}\n",
        "demo/a/golden/f": "golden\n",
        "demo/a/provider/recorded-1.json": "{}\n",
        "demo/README.md": "demo\n",
        "tests/fixtures/f": "fixture\n",
        "crates/x/tests/fixtures/f": "fx\n",
        "crates/x/tests/golden/g": "gg\n",
        "catalogue/baseline.v1.json": "{}\n",
        "catalogue/diagnostics.v1.json": "{}\n",
        "docs/reviews/old.md": "old\n",
    }

    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="hooktest-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.home = os.path.join(self.tmp, "home")
        os.makedirs(self.home)
        self.repo = os.path.join(self.tmp, "repo")
        os.makedirs(self.repo)
        for rel, text in self.FILES.items():
            self.put(rel, text)
        self.git("init", "-q", "-b", "main")
        self.git("add", "-A")
        self.commit("init")
        os.symlink("demo/a/golden", os.path.join(self.repo, "link"))

    def put(self, rel, text):
        path = os.path.join(self.repo, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)

    def read(self, rel):
        with open(os.path.join(self.repo, rel)) as handle:
            return handle.read()

    def env(self, extra=None):
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env["HOME"] = self.home
        real_home = os.path.expanduser("~")
        env.setdefault("RUSTUP_HOME", os.path.join(real_home, ".rustup"))
        env.setdefault("CARGO_HOME", os.path.join(real_home, ".cargo"))
        env.update(extra or {})
        return env

    def git(self, *args, cwd=None):
        proc = subprocess.run(
            ["git", *args],
            cwd=cwd or self.repo,
            capture_output=True,
            text=True,
            env=self.env(),
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return proc.stdout

    def commit(self, message):
        self.git(
            "-c", "user.name=t", "-c", "user.email=t@e.invalid", "commit", "-q",
            "-m", message,
        )

    def run_script(self, name, payload, extra_env=None, cwd=None, raw=None):
        proc = subprocess.run(
            [sys.executable, os.path.join(HOOKS, name)],
            input=raw if raw is not None else json.dumps(payload),
            cwd=cwd or self.repo,
            capture_output=True,
            text=True,
            env=self.env(extra_env),
            timeout=60,
        )
        return proc.returncode, proc.stderr, proc.stdout

    def pre(self, tool, **tool_input):
        return {
            "session_id": "s",
            "hook_event_name": "PreToolUse",
            "cwd": self.repo,
            "tool_name": tool,
            "tool_input": tool_input,
        }

    def shell(self, command, **extra):
        return self.pre("execute_bash", command=command, **extra)

    @property
    def golden_dir(self):
        return os.path.join(self.repo, ".git", "ruletrace-golden")

    def approvals(self):
        with open(os.path.join(self.golden_dir, "approvals.json")) as handle:
            return json.load(handle)


class GuardShellTable(RepoCase):
    """Rules (a) to (e) and the allowed benign commands."""

    BLOCK = [
        # (a) git
        ("git push", "(a)"),
        ("git   push", "(a)"),
        ('"git" "push"', "(a)"),
        ("git -c x=y push", "(a)"),
        ("FOO=1 git push", "(a)"),
        ("git -C /tmp/x push", "(a)"),
        ("git --git-dir=/x push", "(a)"),
        ("git --git-dir /x push origin", "(a)"),
        ("git --no-pager push", "(a)"),
        ("cd x && git push", "(a)"),
        ("true; git reset --hard", "(a)"),
        ("echo $(git push)", "(a)"),
        ("echo `git push`", "(a)"),
        ('echo "$(git push)"', "(a)"),
        ('bash -c "git push"', "(a)"),
        ("sh -c 'git push'", "(a)"),
        ("bash -lc 'git push'", "(a)"),
        ("bash -c \"bash -c 'git push'\"", "(a)"),
        ("env GIT_X=1 git push", "(a)"),
        ("command git push", "(a)"),
        ("/usr/bin/git push origin", "(a)"),
        ("(git push)", "(a)"),
        ("{ git push; }", "(a)"),
        ("if true; then git push; fi", "(a)"),
        ("git push | cat", "(a)"),
        ("git push &", "(a)"),
        ("true &&\\\n git push", "(a)"),
        ("echo hi\ngit push", "(a)"),
        ("echo x | xargs git push", "(a)"),
        ("find . -exec git push {} \\;", "(a)"),
        ("eval 'git push'", "(a)"),
        ("nice -n 5 git push", "(a)"),
        ("timeout 5 git push", "(a)"),
        ("env -S 'git push'", "(a)"),
        ("bash <<EOF\ngit push\nEOF", "(a)"),
        ("git push \"unterminated", "unparseable"),
        ("git reset --hard HEAD~1", "(a)"),
        ("git reset --merge", "(a)"),
        ("git reset --keep", "(a)"),
        ("git reset --har", "(a)"),
        ("git clean -fd", "(a)"),
        ("git clean -n", "(a)"),
        ("git checkout -- file", "(a)"),
        ("git checkout .", "(a)"),
        ("git checkout -f main", "(a)"),
        ("git restore f", "(a)"),
        ("git branch -D x", "(a)"),
        ("git branch -d x", "(a)"),
        ("git branch --delete x", "(a)"),
        ("git tag -d v1", "(a)"),
        ("git tag --delete v1", "(a)"),
        ("git rebase main", "(a)"),
        ("git stash drop", "(a)"),
        ("git stash clear", "(a)"),
        ("git commit --amend", "(a)"),
        ("git commit --amend -m x", "(a)"),
        ("git update-ref -d refs/heads/x", "(a)"),
        ("git reflog expire --all", "(a)"),
        ("git reflog delete HEAD@{1}", "(a)"),
        ("git gc --prune=now", "(a)"),
        ("git filter-branch --all", "(a)"),
        ("git remote add o u", "(a)"),
        ("git remote set-url o u", "(a)"),
        ("git remote remove o", "(a)"),
        ("git remote rename a b", "(a)"),
        ("git config user.name x", "(a)"),
        ("git config --unset user.name", "(a)"),
        ("git config --global core.editor vi", "(a)"),
        ("git -c alias.p=push p", "(a)"),
        # (b) gh
        ("gh run rerun 1", "(b)"),
        ("gh run cancel 1", "(b)"),
        ("gh run delete 1", "(b)"),
        ("gh workflow run x.yml", "(b)"),
        ("gh workflow list", "(b)"),
        ("gh pr create", "(b)"),
        ("gh pr merge 1", "(b)"),
        ("gh pr close 1", "(b)"),
        ("gh pr edit 1", "(b)"),
        ("gh pr comment 1", "(b)"),
        ("gh pr review 1", "(b)"),
        ("gh pr ready 1", "(b)"),
        ("gh pr reopen 1", "(b)"),
        ("gh -R a/b pr merge 1", "(b)"),
        ("gh issue create", "(b)"),
        ("gh issue edit 1", "(b)"),
        ("gh issue close 1", "(b)"),
        ("gh issue comment 1", "(b)"),
        ("gh issue delete 1", "(b)"),
        ("gh issue reopen 1", "(b)"),
        ("gh issue transfer 1 r", "(b)"),
        ("gh release create v1", "(b)"),
        ("gh release list", "(b)"),
        ("gh repo create x", "(b)"),
        ("gh repo delete x", "(b)"),
        ("gh repo edit", "(b)"),
        ("gh repo rename y", "(b)"),
        ("gh repo fork", "(b)"),
        ("gh repo archive", "(b)"),
        ("gh repo unarchive", "(b)"),
        ("gh secret set X", "(b)"),
        ("gh variable list", "(b)"),
        ("gh gist create f", "(b)"),
        ("gh label create l", "(b)"),
        ("gh ruleset list", "(b)"),
        ("gh cache delete 1", "(b)"),
        ("gh api -X POST repos/x/y", "(b)"),
        ("gh api -X post repos/x/y", "(b)"),
        ("gh api --method DELETE repos/x/y", "(b)"),
        ("gh api --method=PATCH repos/x/y", "(b)"),
        ("gh api -XPUT repos/x/y", "(b)"),
        ("gh api repos/x/y -f a=b", "(b)"),
        ("gh api repos/x/y -F a=b", "(b)"),
        ("gh api repos/x/y -fa=b", "(b)"),
        ("gh api repos/x/y --field a=b", "(b)"),
        ("gh api repos/x/y --raw-field a=b", "(b)"),
        ("gh api repos/x/y --input body.json", "(b)"),
        # (c) credentials
        ("printenv", "(c)"),
        ("printenv HOME", "(c)"),
        ("env", "(c)"),
        ("env | grep KEY", "(c)"),
        ("export -p", "(c)"),
        ("export", "(c)"),
        ("set", "(c)"),
        ("declare -p", "(c)"),
        ("echo $OPENAI_API_KEY", "(c)"),
        ("echo ${GH_TOKEN}", "(c)"),
        ('printf %s "$GITHUB_TOKEN"', "(c)"),
        ("echo $MODEL_API_KEY", "(c)"),
        ("echo $RULETRACE_OPENAI_API_KEY", "(c)"),
        ("echo $ANTHROPIC_API_KEY", "(c)"),
        ("cat <<EOF\n$MODEL_API_KEY\nEOF", "(c)"),
        ("python3 -c 'import os; print(os.environ[\"OPENAI_API_KEY\"])'", "(c)"),
        ("gh auth token", "(c)"),
        ("gh auth status -t", "(c)"),
        ("gh auth status --show-token", "(c)"),
        ("cat ~/.modal.toml", "(c)"),
        ("cat ~/.config/gh/hosts.yml", "(c)"),
        ("cat ~/.kiro/foo/auth.json", "(c)"),
        ("cat ~/.kiro/x/my_token", "(c)"),
        ('cat "$HOME/.kiro/auth_state"', "(c)"),
        ("cat /proc/self/environ", "(c)"),
        ("tr '\\0' '\\n' < /proc/1234/environ", "(c)"),
        ("cat < ~/.modal.toml", "(c)"),
        # (d) provider endpoints
        ("curl https://api.openai.com/v1/x", "(d)"),
        ("wget https://api.anthropic.com/x", "(d)"),
        ("curl -s https://api.meta.ai/x", "(d)"),
        ("http api.openai.com", "(d)"),
        ("nc api.openai.com 443", "(d)"),
        ("openssl s_client -connect api.openai.com:443", "(d)"),
        (
            "python3 -c \"import urllib.request as u; "
            "u.urlopen('https://api.openai.com')\"",
            "(d)",
        ),
        ("ruletrace-ai propose --config x.toml", "(d)"),
        ("ruletrace-ai models --config x.toml", "(d)"),
        # (e) isolation
        ("cargo test", "(e)"),
        ("cargo test -p a --test b", "(e)"),
        ("cargo run -q -p x", "(e)"),
        ("cargo nextest run", "(e)"),
        ("cargo bench", "(e)"),
        ("cargo +nightly test", "(e)"),
        ("cargo t", "(e)"),
        ("cargo test -- --no-run", "(e)"),
        ("CARGO_TARGET_DIR=/x cargo test", "(e)"),
        ("sh -c 'cargo test'", "(e)"),
        ("cargo mutants", "(e)"),
        ("cargo mutants --in-place -p x", "(e)"),
        ("cargo mutants --check", "(e)"),
        ("cargo +nightly mutants", "(e)"),
        ("cargo mutants -- --list", "(e)"),
        ("cargo-mutants mutants", "(e)"),
        ("sh -c 'cargo mutants'", "(e)"),
        ("cargo llvm-cov", "(e)"),
        ("cargo llvm-cov --html --workspace", "(e)"),
        ("cargo llvm-cov --no-run", "(e)"),
        ("cargo tarpaulin", "(e)"),
        ("cargo tarpaulin --out Html", "(e)"),
        ("cargo miri test", "(e)"),
        ("cargo +nightly miri run", "(e)"),
        ("cargo miri setup", "(e)"),
        ("cargo miri test --list", "(e)"),
        ("cargo llvm-cov --list", "(e)"),
        ("cargo tarpaulin --list-files", "(e)"),
        ("cargo careful test", "(e)"),
        ("cargo careful run", "(e)"),
        ("cargo mutants --list 'unterminated", "unparseable"),
        ("unshare -n cargo test", "weaker"),
        ("unshare -rn cargo test", "weaker"),
        ("unshare -rn sh -c 'ip link set lo up' sh cargo test", "weaker"),
        ("unshare -rn sh -c 'ip link set lo up && exec \"$@\"' cargo test", "weaker"),
        ("unshare -rn sh -c 'ip link set lo up && exec \"$@\"' sh", "weaker"),
        ("./target/debug/ruletrace-ai --dry-run", "(e)"),
        ("./target/release/ruletrace evaluate", "(e)"),
        ("./target/debug/prize-check", "(e)"),
        ("./target/debug/bracket-check", "(e)"),
        # other
        ("sudo ls", "sudo"),
    ]

    ALLOW = [
        "cargo build",
        "cargo build --release -p x",
        "cargo check",
        "cargo fmt --all -- --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo metadata --format-version 1",
        "cargo tree",
        "cargo deny check",
        "cargo doc",
        "cargo fetch",
        "cargo --version",
        "cargo test --no-run",
        "cargo test --no-run -p x",
        "cargo mutants --list",
        "cargo mutants --list -p x",
        "cargo +nightly mutants --list --json",
        "cargo mutants --list-files",
        "cargo mutants --list-dirs",
        "cargo mutants --version",
        "cargo mutants -V",
        "cargo mutants --help",
        "cargo-mutants --list",
        "cargo llvm-cov --version",
        "cargo tarpaulin --help",
        "cargo miri --version",
        "cargo careful --version",
        WRAPPER + " cargo mutants -p x",
        WRAPPER + " cargo llvm-cov --workspace",
        WRAPPER + " cargo +nightly miri test",
        WRAPPER + " cargo tarpaulin",
        WRAPPER + " cargo careful test",
        "rustfmt --edition 2021 --check x",
        "git status",
        "git status --short",
        'git commit -m "msg"',
        "git commit -m 'avoid git push here'",
        "git add tools/hooks/x.py",
        "git diff",
        "git log --oneline -5",
        "git worktree add --detach /tmp/w HEAD",
        "git checkout -b newbranch",
        "git switch m3",
        "git checkout m3",
        "git stash push -m x",
        "git branch -a",
        "git tag -l",
        "git remote -v",
        "git config user.name",
        "git config --get user.name",
        "git config --list",
        "git reset --soft HEAD~1",
        "git reset HEAD file",
        "gh run view 1",
        "gh run list",
        "gh pr view 1",
        "gh issue list",
        "gh auth status",
        "gh api repos/x/y/actions/runs",
        "gh api -X GET repos/x/y",
        "gh api --method GET repos/x/y",
        WRAPPER + " cargo test -p rt-ai-responses --test stub_http",
        WRAPPER + " cargo run -q -p rt-xtask -- check-graph",
        "CARGO_TARGET_DIR=/t " + WRAPPER + " cargo test",
        WRAPPER + " ./target/debug/ruletrace-ai propose --config c.toml",
        WRAPPER + " ./target/debug/ruletrace evaluate",
        "ruletrace-ai propose --dry-run --config c.toml",
        "sed -n 1,5p file",
        "python3 -c 'print(1)'",
        "ls -la",
        "echo hello",
        "echo $HOME",
        "export FOO=1",
        "env FOO=1 true",
        "set -e",
        "cat .kiro/specs/x/authoring.md",
        "curl https://example.org",
        "echo api.openai.com",
        'git commit -m "mention api.openai.com"',
        "echo x > docs/reviews/r.md",
        "echo x > /tmp/not-in-repo",
        "cat demo/a/golden/f",
        "cp demo/a/golden/f /tmp/copy",
        "sed -n 1p demo/a/golden/f",
        "rm demo/README.md",
        "cat <<'EOF'\ngit push\nEOF",
        "echo 'unterminated",
    ]

    def test_blocked_commands(self):
        for command, marker in self.BLOCK:
            with self.subTest(command=command):
                code, err, _ = self.run_script("guard_shell.py", self.shell(command))
                self.assertEqual(code, 2, err)
                self.assertIn(marker, err)

    def test_allowed_commands(self):
        for command in self.ALLOW:
            with self.subTest(command=command):
                code, err, _ = self.run_script("guard_shell.py", self.shell(command))
                self.assertEqual((code, err), (0, ""))

    def test_reason_names_the_rule_and_tells_how_to_isolate(self):
        _, err, _ = self.run_script("guard_shell.py", self.shell("unshare -n cargo test"))
        self.assertIn("a weaker or different unshare wrapper is not accepted", err)
        self.assertIn("ip link set lo up && exec", err)
        _, err, _ = self.run_script("guard_shell.py", self.shell("git push"))
        self.assertIn("guard_shell: command blocked", err)
        self.assertIn("rule (a) git-destructive", err)

    def test_other_tools_and_bad_input(self):
        payload = self.pre("read_file", path="x")
        self.assertEqual(self.run_script("guard_shell.py", payload)[0], 0)
        self.assertEqual(self.run_script("guard_shell.py", None, raw="not json")[0], 2)
        self.assertEqual(self.run_script("guard_shell.py", None, raw="[1]")[0], 2)
        bad = self.pre("execute_bash", command=5)
        self.assertEqual(self.run_script("guard_shell.py", bad)[0], 2)

    def test_home_dependent_secret_path_uses_the_real_home(self):
        other = os.path.join(self.tmp, "elsewhere")
        os.makedirs(os.path.join(other, ".kiro"))
        command = "cat %s/.kiro/auth.json" % other
        code, _, _ = self.run_script("guard_shell.py", self.shell(command))
        self.assertEqual(code, 0)
        command = "cat %s/.kiro/auth.json" % self.home
        code, err, _ = self.run_script("guard_shell.py", self.shell(command))
        self.assertEqual(code, 2, err)


class GuardShellGoldenWrites(RepoCase):
    """Rule (f): approvable writes to protected paths through the shell."""

    BLOCK = [
        "echo x > demo/a/golden/f",
        "echo x >> tests/fixtures/f",
        "echo x>demo/a/provider/recorded-x.json",
        "echo x > ./crates/x/../../tests/fixtures/f",
        "echo x > link/f",
        "echo x | tee crates/x/tests/golden/g",
        "sed -i s/a/b/ demo/a/golden/f",
        "sed -i.bak s/a/b/ demo/a/golden/f",
        "sed --in-place s/a/b/ demo/a/golden/f",
        "cp /tmp/a demo/a/golden/f",
        "cp /tmp/a demo/a/golden/",
        "cp -t demo/a/golden /tmp/a",
        "mv /tmp/a tests/fixtures/f",
        "mv tests/fixtures/f /tmp/b",
        "rm demo/a/golden/f",
        "rm -rf demo/a/golden",
        "rm -r tests/fixtures",
        "rm -rf demo/a",
        "truncate -s 0 catalogue/baseline.v1.json",
        "install /tmp/a crates/x/tests/fixtures/f",
        "patch demo/a/provider/recorded-1.json /tmp/p",
        "dd if=/tmp/a of=demo/a/provider/authored-script.json",
        "cd demo/a/golden && echo x > f",
        "echo x > demo/a/golden/*",
        "RT_DEMO_BLESS=1 true",
        "env RT_DEMO_BLESS=1 true",
        "true --bless",
        "export RT_DEMO_BLESS=1",
        "export FOO=1 RT_DEMO_BLESS=1",
        "declare RT_DEMO_BLESS=1",
        "typeset RT_DEMO_BLESS=1",
        "readonly RT_DEMO_BLESS=1",
        "sh -c 'RT_DEMO_BLESS=1 true'",
        "bash -c \"bash -c 'export RT_DEMO_BLESS=1'\"",
        "env -i RT_DEMO_BLESS=1 true",
        "env FOO=1 RT_DEMO_BLESS=1 true",
        "nice env RT_DEMO_BLESS=1 true",
        "timeout 5 env RT_DEMO_BLESS=1 true",
        "echo x | xargs env RT_DEMO_BLESS=1 true",
        "env -S 'RT_DEMO_BLESS=1 true'",
        "echo ok && RT_DEMO_BLESS=1 true",
        "RT_DEMO_BLESS=1; true",
        "for i in 1; do RT_DEMO_BLESS=1 true; done",
        "echo $(RT_DEMO_BLESS=1 true)",
        "RT_DEMO_BLESS=0 true",
        "RT_DEMO_BLESS= true",
        "RT_DEMO_BLESS+=1 true",
        "find . -exec env RT_DEMO_BLESS=1 true \\;",
        "bash <<EOF\nRT_DEMO_BLESS=1 true\nEOF",
        WRAPPER + " cargo run --bless",
        "git commit -m --bless",
        "true --bless=all",
        "env --bless true",
        "sed --bless f",
        "sed s/a/b/ --bless",
        "sed -in --bless f",
        "xargs --bless",
        "bash --bless",
        "bash -c 'echo x > tests/fixtures/f'",
        # RT_APP_GOLDEN_BLESS (task 9.16) writes the app-report goldens.
        "RT_APP_GOLDEN_BLESS=/r/tests/fixtures/app-reports true",
        "env RT_APP_GOLDEN_BLESS=/x true",
        "export RT_APP_GOLDEN_BLESS=/x",
        "sh -c 'RT_APP_GOLDEN_BLESS=/x true'",
        "env -i RT_APP_GOLDEN_BLESS=/x true",
        "env FOO=1 RT_APP_GOLDEN_BLESS=/x true",
        "RT_APP_GOLDEN_BLESS= true",
        WRAPPER + " env RT_APP_GOLDEN_BLESS=/x cargo test -p rt-tests-core --test app_report_goldens",
        # RULETRACE_BLESS writes the slice goldens (rt-tests-core golden/slice.rs).
        "RULETRACE_BLESS=1 true",
        "env RULETRACE_BLESS=1 true",
        "export RULETRACE_BLESS=1",
        "sh -c 'RULETRACE_BLESS=1 true'",
        WRAPPER + " env RULETRACE_BLESS=1 cargo test -p rt-tests-core",
    ]

    def test_blocked_and_explained(self):
        for command in self.BLOCK:
            with self.subTest(command=command):
                code, err, _ = self.run_script("guard_shell.py", self.shell(command))
                self.assertEqual(code, 2, err)
                self.assertIn("rule (f) golden-write", err)
                self.assertIn("golden_approve.py --command", err)

    ALLOW = [
        "grep -rn RT_DEMO_BLESS crates",
        "grep RT_DEMO_BLESS=1 README.md",
        "grep -rn --include='*.rs' RT_DEMO_BLESS crates tools",
        "grep -rn RT_DEMO_BLESS crates | head -5",
        "egrep -rn 'RT_DEMO_BLESS|--bless' crates",
        "rg RT_DEMO_BLESS",
        "rg -n RT_DEMO_BLESS=1 crates",
        "echo RT_DEMO_BLESS",
        "echo RT_DEMO_BLESS=1",
        "printf '%s\\n' RT_DEMO_BLESS",
        "cat RT_DEMO_BLESS",
        "head -n 3 RT_DEMO_BLESS",
        "tail -n 3 RT_DEMO_BLESS",
        "less RT_DEMO_BLESS",
        "wc -l RT_DEMO_BLESS",
        "awk '/RT_DEMO_BLESS/' README.md",
        "sed -n /RT_DEMO_BLESS/p README.md",
        "bash -c 'grep -rn RT_DEMO_BLESS .'",
        "nohup grep RT_DEMO_BLESS README.md",
        "find . -exec grep -l RT_DEMO_BLESS {} \\;",
        "echo --bless",
        "echo --bless=1",
        "grep -- --bless README.md",
        "grep -rn -- --bless crates",
        "rg -- --bless",
        "printf '%s' --bless",
        "cat --bless",
        "head --bless",
        "tail --bless",
        "less --bless",
        "wc --bless",
        "awk --bless",
        "sed -n --bless",
        "sed -n -e p --bless",
        "sed --quiet --bless f",
        "sed -n /--bless/p README.md",
        "echo 'RT_DEMO_BLESS=1 cargo run'",
        "cargo build -p x",
        "cat <<'EOF'\nRT_DEMO_BLESS=1 true\nEOF",
        "grep RT_DEMO_BLESS <<'EOF'\nRT_DEMO_BLESS=1 true\nEOF",
        "grep -rn RT_APP_GOLDEN_BLESS crates",
        "rg RT_APP_GOLDEN_BLESS=/x",
        "echo RT_APP_GOLDEN_BLESS=/x",
        "grep -rn RULETRACE_BLESS crates",
    ]

    def test_read_only_mentions_are_allowed(self):
        for command in self.ALLOW:
            with self.subTest(command=command):
                code, err, _ = self.run_script("guard_shell.py", self.shell(command))
                self.assertEqual((code, err), (0, ""))

    def test_unparseable_assignment_text_is_blocked_without_approval(self):
        command = "RT_DEMO_BLESS=1 true 'unterminated"
        code, err, _ = self.run_script("guard_shell.py", self.shell(command))
        self.assertEqual(code, 2)
        self.assertIn("unparseable", err)
        self.assertNotIn("golden_approve.py --command", err)

    def test_approval_covers_each_blocked_bless_form_once(self):
        for command in (
            "export RT_DEMO_BLESS=1",
            "sh -c 'RT_DEMO_BLESS=1 true'",
            "true --bless",
        ):
            with self.subTest(command=command):
                run = lambda c: self.run_script("guard_shell.py", self.shell(c))
                code, err, _ = run(command)
                self.assertEqual(code, 2)
                self.assertIn("golden_approve.py --command", err)
                golden_approve.approve_command(self.repo, command)
                self.assertEqual(run(command), (0, "", ""))
                self.assertEqual(run(command)[0], 2)

    def test_the_app_report_bless_command_runs_once_after_approval(self):
        command = (
            WRAPPER
            + " env RT_APP_GOLDEN_BLESS=/r/tests/fixtures/app-reports"
            + " CARGO_TARGET_DIR=/r/target"
            + " cargo test -p rt-tests-core --test app_report_goldens"
        )
        run = lambda c: self.run_script("guard_shell.py", self.shell(c))
        code, err, _ = run(command)
        self.assertEqual(code, 2, err)
        self.assertIn("rule (f) golden-write", err)
        golden_approve.approve_command(self.repo, command)
        self.assertEqual(run(command), (0, "", ""))
        self.assertEqual(run(command)[0], 2)

    def test_approval_is_exact_single_use_and_expiring(self):
        command = "RT_DEMO_BLESS=1 true"
        run = lambda c: self.run_script("guard_shell.py", self.shell(c))
        self.assertEqual(run(command)[0], 2)
        golden_approve.approve_command(self.repo, command)
        entry = self.approvals()[0]
        self.assertEqual(entry["kind"], "shell")
        self.assertEqual(entry["command_sha256"], sha(command))
        self.assertIs(entry["used"], False)
        self.assertEqual(run(command + " ")[0], 2)
        self.assertEqual(run(command), (0, "", ""))
        self.assertTrue(self.approvals()[0]["used"])
        self.assertEqual(run(command)[0], 2)

    def test_expired_approval_does_not_apply(self):
        command = "echo x > tests/fixtures/f"
        golden_approve.approve_command(
            self.repo, command, minutes=1, now=time.time() - 3600
        )
        self.assertEqual(self.run_script("guard_shell.py", self.shell(command))[0], 2)

    def test_approval_cannot_override_a_non_approvable_rule(self):
        command = "git push && echo x > tests/fixtures/f"
        golden_approve.approve_command(self.repo, command)
        code, err, _ = self.run_script("guard_shell.py", self.shell(command))
        self.assertEqual(code, 2)
        self.assertIn("rule (a)", err)
        self.assertNotIn("golden_approve.py --command", err)
        self.assertIs(self.approvals()[0]["used"], False)

    def test_approved_bless_run_still_needs_the_wrapper(self):
        command = "RT_DEMO_BLESS=1 cargo run -q -p x"
        golden_approve.approve_command(self.repo, command)
        code, err, _ = self.run_script("guard_shell.py", self.shell(command))
        self.assertEqual(code, 2)
        self.assertIn("rule (e)", err)

    def test_tool_cwd_is_honoured(self):
        payload = self.shell("echo x > f", cwd=os.path.join(self.repo, "demo/a/golden"))
        self.assertEqual(self.run_script("guard_shell.py", payload)[0], 2)


class GuardGolden(RepoCase):
    def write(self, rel, text, tool="fs_write"):
        return self.run_script(
            "guard_golden.py", self.pre(tool, path=os.path.join(self.repo, rel), text=text)
        )

    def pending(self, ident):
        base = os.path.join(self.golden_dir, "pending", ident)
        with open(base + ".json") as handle:
            return json.load(handle)

    def test_every_protected_pattern_blocks_and_near_misses_pass(self):
        protected = [
            "demo/a/golden/f",
            "demo/a/golden/deep/er/f",
            "tests/fixtures/f",
            "crates/x/tests/fixtures/sub/f",
            "crates/x/tests/golden/g",
            "demo/a/provider/recorded-1.json",
            "demo/a/provider/authored-script.json",
            "catalogue/baseline.v1.json",
            "catalogue/baseline-2.json",
        ]
        free = [
            "docs/reviews/r.md",
            "src/lib.rs",
            "demo/a/other/f",
            "demo/a/provider/other.json",
            "demo/golden/f",
            "crates/x/tests/other/f",
            "crates/x/y/tests/fixtures/f",
            "catalogue/diagnostics.v1.json",
            "catalogue/baseline.txt",
        ]
        for rel in protected:
            with self.subTest(blocked=rel):
                code, err, _ = self.write(rel, "new\n")
                self.assertEqual(code, 2, err)
                self.assertIn("is blocked", err)
        for rel in free:
            with self.subTest(allowed=rel):
                self.assertEqual(self.write(rel, "new\n"), (0, "", ""))

    def test_outside_the_repository_is_not_protected(self):
        outside = os.path.join(self.tmp, "elsewhere", "tests", "fixtures", "f")
        payload = self.pre("fs_write", path=outside, text="x")
        self.assertEqual(self.run_script("guard_golden.py", payload), (0, "", ""))

    def test_block_writes_pending_files_and_prints_the_exact_command(self):
        text = "".join("l%d\n" % i for i in range(100))
        rel = "demo/a/golden/new.txt"
        code, err, _ = self.write(rel, text)
        ident = sha("%s|ABSENT|%s" % (rel, sha(text)))[:12]
        self.assertEqual(code, 2)
        self.assertIn("write to %s is blocked" % rel, err)
        self.assertIn("Pending id: %s" % ident, err)
        self.assertIn("tools/hooks/golden_approve.py %s" % ident, err)
        diff_path = os.path.join(self.golden_dir, "pending", ident + ".diff")
        self.assertIn("Diff file: %s" % diff_path, err)
        self.assertEqual(
            self.pending(ident),
            {
                "id": ident,
                "kind": "file",
                "path": rel,
                "old_sha256": "ABSENT",
                "new_sha256": sha(text),
                "created_at": self.pending(ident)["created_at"],
            },
        )
        with open(diff_path) as handle:
            diff = handle.read()
        self.assertIn("+l99\n", diff)
        self.assertIn("+l26\n", err)
        self.assertNotIn("+l27\n", err)
        self.assertFalse(os.path.exists(os.path.join(self.repo, rel)))

    def test_diff_file_is_truncated_at_200_kib(self):
        text = "x" * 10 + "\n" + ("y" * 99 + "\n") * 5000
        _, err, _ = self.write("tests/fixtures/big", text)
        ident = [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]
        size = os.path.getsize(os.path.join(self.golden_dir, "pending", ident + ".diff"))
        self.assertLessEqual(size, 200 * 1024 + 64)
        self.assertGreater(size, 190 * 1024)

    def test_approved_write_is_allowed_once(self):
        rel = "tests/fixtures/f"
        text = "changed\n"
        _, err, _ = self.write(rel, text)
        ident = sha("%s|%s|%s" % (rel, sha("fixture\n"), sha(text)))[:12]
        self.assertIn(ident, err)
        golden_approve.approve_pending(self.repo, ident)
        entry = self.approvals()[0]
        self.assertEqual(
            {k: entry[k] for k in ("id", "path", "old_sha256", "new_sha256", "used")},
            {
                "id": ident,
                "path": rel,
                "old_sha256": sha("fixture\n"),
                "new_sha256": sha(text),
                "used": False,
            },
        )
        self.assertAlmostEqual(entry["expires_at"], time.time() + 1800, delta=30)
        self.assertEqual(self.write(rel, text), (0, "", ""))
        self.assertTrue(self.approvals()[0]["used"])
        self.assertEqual(self.write(rel, text)[0], 2)

    def test_approval_for_other_content_or_path_does_not_apply(self):
        rel = "tests/fixtures/f"
        _, err, _ = self.write(rel, "A\n")
        ident = [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]
        golden_approve.approve_pending(self.repo, ident)
        self.assertEqual(self.write(rel, "B\n")[0], 2)
        self.assertEqual(self.write("demo/a/golden/f", "A\n")[0], 2)
        self.assertIs(self.approvals()[0]["used"], False)
        self.assertEqual(self.write(rel, "A\n"), (0, "", ""))

    def test_approval_is_bound_to_the_path_when_old_and_new_content_match(self):
        self.put("tests/fixtures/twin", "fixture\n")
        _, err, _ = self.write("tests/fixtures/f", "A\n")
        ident = [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]
        golden_approve.approve_pending(self.repo, ident)
        self.assertEqual(self.write("tests/fixtures/twin", "A\n")[0], 2)
        self.assertEqual(self.write("tests/fixtures/f", "A\n"), (0, "", ""))

    def test_stale_approval_after_the_file_changed(self):
        rel = "tests/fixtures/f"
        _, err, _ = self.write(rel, "A\n")
        ident = [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]
        golden_approve.approve_pending(self.repo, ident)
        self.put(rel, "someone else changed it\n")
        self.assertEqual(self.write(rel, "A\n")[0], 2)
        self.assertIs(self.approvals()[0]["used"], False)

    def test_expired_approval(self):
        rel = "tests/fixtures/f"
        _, err, _ = self.write(rel, "A\n")
        ident = [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]
        golden_approve.approve_pending(self.repo, ident, minutes=1, now=time.time() - 3600)
        self.assertEqual(self.write(rel, "A\n")[0], 2)

    def test_identical_content_needs_no_approval(self):
        self.assertEqual(self.write("tests/fixtures/f", "fixture\n"), (0, "", ""))

    def test_path_traversal_and_symlinks_are_resolved(self):
        cases = [
            os.path.join(self.repo, "crates/x/../../demo/a/golden/f"),
            os.path.join(self.repo, "link/f"),
            os.path.join(self.repo, "link", "..", "golden", "f"),
        ]
        for path in cases:
            with self.subTest(path=path):
                code, err, _ = self.run_script(
                    "guard_golden.py", self.pre("fs_write", path=path, text="x")
                )
                self.assertEqual(code, 2, err)
                self.assertIn("demo/a/golden", err)
        relative = self.pre("fs_write", path="crates/x/../../demo/a/golden/f", text="x")
        self.assertEqual(self.run_script("guard_golden.py", relative)[0], 2)

    def test_symlink_pointing_into_a_protected_file(self):
        os.symlink("../tests/fixtures/f", os.path.join(self.repo, "docs", "alias.txt"))
        payload = self.pre("fs_write", path=os.path.join(self.repo, "docs/alias.txt"), text="x")
        code, err, _ = self.run_script("guard_golden.py", payload)
        self.assertEqual(code, 2, err)
        self.assertIn("tests/fixtures/f", err)

    def test_str_replace(self):
        rel = "demo/a/golden/f"
        self.put(rel, "one two two\n")
        run = lambda **kw: self.run_script(
            "guard_golden.py", self.pre("str_replace", path=os.path.join(self.repo, rel), **kw)
        )
        code, err, _ = run(oldStr="missing", newStr="x")
        self.assertEqual(code, 2)
        self.assertIn("not found", err)
        code, err, _ = run(oldStr="two", newStr="x")
        self.assertEqual(code, 2)
        self.assertIn("ambiguous", err)
        self.assertEqual(run(oldStr="one", newStr="1")[0], 2)
        expected_new = sha("1 two two\n")
        ident = sha("%s|%s|%s" % (rel, sha("one two two\n"), expected_new))[:12]
        self.assertEqual(self.pending(ident)["new_sha256"], expected_new)
        golden_approve.approve_pending(self.repo, ident)
        self.assertEqual(run(oldStr="one", newStr="1"), (0, "", ""))
        code, _, _ = run(old_str="one", new_str="1")
        self.assertEqual(code, 2)

    def test_str_replace_all_and_alternate_field_names(self):
        rel = "demo/a/golden/f"
        self.put(rel, "one two two\n")
        payload = self.pre(
            "str_replace",
            path=os.path.join(self.repo, rel),
            old_str="two",
            new_str="2",
            replace_all=True,
        )
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)
        ident = sha("%s|%s|%s" % (rel, sha("one two two\n"), sha("one 2 2\n")))[:12]
        self.assertEqual(self.pending(ident)["path"], rel)

    def test_str_replace_unusable_shapes_fail_closed(self):
        rel = os.path.join(self.repo, "demo/a/golden/f")
        shapes = [
            {"path": rel},
            {"path": rel, "oldStr": "golden"},
            {"path": rel, "oldStr": "", "newStr": "x"},
            {"path": rel, "oldStr": 1, "newStr": "x"},
            {"path": rel, "oldStr": "golden", "newStr": "x", "replace_all": "yes"},
            {"path": os.path.join(self.repo, "demo/a/golden/absent"), "oldStr": "a", "newStr": "b"},
        ]
        for shape in shapes:
            with self.subTest(shape=shape):
                payload = self.pre("str_replace", **shape)
                self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)

    def test_fs_append_hash_is_old_plus_text(self):
        rel = "tests/fixtures/f"
        payload = self.pre("fs_append", path=os.path.join(self.repo, rel), text="more\n")
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)
        ident = sha("%s|%s|%s" % (rel, sha("fixture\n"), sha("fixture\nmore\n")))[:12]
        golden_approve.approve_pending(self.repo, ident)
        self.assertEqual(self.run_script("guard_golden.py", payload), (0, "", ""))

    def test_delete_file_uses_the_deleted_marker(self):
        rel = "tests/fixtures/f"
        payload = self.pre("delete_file", path=os.path.join(self.repo, rel))
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)
        ident = sha("%s|%s|DELETED" % (rel, sha("fixture\n")))[:12]
        self.assertEqual(self.pending(ident)["new_sha256"], "DELETED")
        golden_approve.approve_pending(self.repo, ident)
        self.assertEqual(self.run_script("guard_golden.py", payload), (0, "", ""))

    def test_unknown_shapes_fail_closed(self):
        self.assertEqual(self.run_script("guard_golden.py", None, raw="nope")[0], 2)
        self.assertEqual(
            self.run_script("guard_golden.py", self.pre("fs_write", text="x"))[0], 2
        )
        payload = self.pre("fs_write", path=os.path.join(self.repo, "tests/fixtures/f"))
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)
        payload = self.pre("fs_write", path=os.path.join(self.repo, "tests/fixtures"), text="x")
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 0)
        directory = self.pre("fs_write", path=os.path.join(self.repo, "demo/a/golden/sub/"), text="x")
        os.makedirs(os.path.join(self.repo, "demo/a/golden/sub"))
        self.assertEqual(self.run_script("guard_golden.py", directory)[0], 2)
        self.assertEqual(self.run_script("guard_golden.py", self.pre("read_file", path="x"))[0], 0)

    def test_corrupt_approval_store_blocks(self):
        os.makedirs(self.golden_dir)
        with open(os.path.join(self.golden_dir, "approvals.json"), "w") as handle:
            handle.write("{not json")
        code, err, _ = self.write("tests/fixtures/f", "A\n")
        self.assertEqual(code, 2)
        self.assertIn("approval store problem", err)

    def test_forged_entries_do_not_apply(self):
        rel = "tests/fixtures/f"
        base = {
            "id": "x",
            "kind": "file",
            "path": rel,
            "old_sha256": sha("fixture\n"),
            "new_sha256": sha("A\n"),
        }
        bad = [
            dict(base, expires_at=time.time() + 600, used=True),
            dict(base, expires_at="soon", used=False),
            dict(base, expires_at=True, used=False),
            dict(base, expires_at=time.time() + 600, used="no"),
            dict(base, expires_at=time.time() + 600),
            dict(base, expires_at=time.time() + 600, used=False, kind="shell"),
        ]
        os.makedirs(self.golden_dir)
        with open(os.path.join(self.golden_dir, "approvals.json"), "w") as handle:
            json.dump(bad, handle)
        self.assertEqual(self.write(rel, "A\n")[0], 2)


class GoldenApproveCli(RepoCase):
    def blocked_id(self, rel="tests/fixtures/f", text="A\n"):
        payload = self.pre("fs_write", path=os.path.join(self.repo, rel), text=text)
        _, err, _ = self.run_script("guard_golden.py", payload)
        return [l for l in err.splitlines() if l.startswith("Pending id:")][0].split()[-1]

    def approve_cli(self, args, answer=None):
        script = os.path.join(HOOKS, "golden_approve.py")
        if answer is None:
            proc = subprocess.run(
                [sys.executable, script, *args],
                cwd=self.repo,
                capture_output=True,
                text=True,
                env=self.env(),
                input="yes\n",
            )
            return proc.returncode, proc.stdout, proc.stderr
        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [sys.executable, script, *args],
            cwd=self.repo,
            stdin=slave,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self.env(),
            text=True,
        )
        os.close(slave)
        os.write(master, (answer + "\n").encode())
        out, err = proc.communicate(timeout=30)
        os.close(master)
        return proc.returncode, out, err

    def test_refuses_without_a_tty_and_writes_nothing(self):
        ident = self.blocked_id()
        code, out, err = self.approve_cli([ident])
        self.assertEqual(code, 2)
        self.assertIn("not an interactive terminal", err)
        self.assertFalse(os.path.exists(os.path.join(self.golden_dir, "approvals.json")))

    def test_yes_on_a_tty_approves_and_shows_the_diff(self):
        ident = self.blocked_id()
        code, out, err = self.approve_cli([ident], answer="yes")
        self.assertEqual(code, 0, err)
        self.assertIn("-fixture", out)
        self.assertIn("+A", out)
        self.assertIn("approved %s" % ident, out)
        entry = self.approvals()[0]
        self.assertEqual(entry["path"], "tests/fixtures/f")
        self.assertAlmostEqual(entry["expires_at"], time.time() + 1800, delta=30)

    def test_other_answers_do_not_approve(self):
        ident = self.blocked_id()
        for answer in ("no", "y", "YES", ""):
            with self.subTest(answer=answer):
                code, out, _ = self.approve_cli([ident], answer=answer)
                self.assertEqual(code, 1)
                self.assertIn("not approved", out)
        self.assertFalse(os.path.exists(os.path.join(self.golden_dir, "approvals.json")))

    def test_minutes_limits(self):
        ident = self.blocked_id()
        code, _, err = self.approve_cli([ident, "--minutes", "121"], answer="yes")
        self.assertEqual(code, 2)
        self.assertIn("between 1 and 120", err)
        code, _, _ = self.approve_cli([ident, "--minutes", "0"], answer="yes")
        self.assertEqual(code, 2)
        code, _, err = self.approve_cli([ident, "--minutes", "120"], answer="yes")
        self.assertEqual(code, 0, err)
        self.assertAlmostEqual(self.approvals()[0]["expires_at"], time.time() + 7200, delta=30)

    def test_unknown_or_malformed_id(self):
        for ident in ("0123456789ab", "../../etc", "ZZ"):
            with self.subTest(ident=ident):
                code, _, err = self.approve_cli([ident], answer="yes")
                self.assertEqual(code, 2)

    def test_refuses_when_the_file_changed_since_the_request(self):
        ident = self.blocked_id()
        self.put("tests/fixtures/f", "changed meanwhile\n")
        code, _, err = self.approve_cli([ident], answer="yes")
        self.assertEqual(code, 2)
        self.assertIn("changed since the request", err)

    def test_list_and_revoke(self):
        ident = self.blocked_id()
        self.approve_cli([ident], answer="yes")
        code, out, _ = self.approve_cli(["--list"], answer="x")
        self.assertEqual(code, 0)
        self.assertIn("approval %s  active  tests/fixtures/f" % ident, out)
        self.assertIn("pending  %s  tests/fixtures/f" % ident, out)
        code, out, _ = self.approve_cli(["--revoke", ident], answer="x")
        self.assertEqual(code, 0)
        payload = self.pre("fs_write", path=os.path.join(self.repo, "tests/fixtures/f"), text="A\n")
        self.assertEqual(self.run_script("guard_golden.py", payload)[0], 2)
        code, _, err = self.approve_cli(["--revoke", ident], answer="x")
        self.assertEqual(code, 2)

    def test_command_approval_through_the_cli(self):
        command = "echo x > tests/fixtures/f"
        code, out, err = self.approve_cli(["--command", command], answer="yes")
        self.assertEqual(code, 0, err)
        self.assertEqual(self.run_script("guard_shell.py", self.shell(command)), (0, "", ""))
        self.assertEqual(self.run_script("guard_shell.py", self.shell(command))[0], 2)

    def test_printed_command_is_copy_pasteable(self):
        command = "echo 'a b' > tests/fixtures/f"
        _, err, _ = self.run_script("guard_shell.py", self.shell(command))
        printed = [l for l in err.splitlines() if "golden_approve.py --command" in l][0]
        argv = shlex.split(printed)
        self.assertEqual(argv[-1], command)


class ReviewIntegrity(RepoCase):
    def cli(self, *args):
        proc = subprocess.run(
            [sys.executable, os.path.join(HOOKS, "review_integrity.py"), *args],
            cwd=self.repo,
            capture_output=True,
            text=True,
            env=self.env(),
        )
        return proc.returncode, proc.stdout, proc.stderr

    def report(self, rel="docs/reviews/new.md", tool="fs_write"):
        return self.run_script(
            "guard_review_report.py",
            self.pre(tool, path=os.path.join(self.repo, rel), text="report\n"),
        )

    def test_begin_records_state_and_latest(self):
        self.put("src/lib.rs", "pub fn changed() {}\n")
        self.put("new.txt", "u\n")
        os.remove(os.path.join(self.repo, "README.md"))
        self.put("docs/reviews/ignored.md", "x\n")
        os.makedirs(os.path.join(self.repo, "target"))
        self.put("target/junk", "x\n")
        self.assertEqual(self.cli("begin", "r1")[0], 0)
        directory = os.path.join(self.repo, ".git", "ruletrace-review")
        with open(os.path.join(directory, "latest")) as handle:
            self.assertEqual(handle.read().strip(), "r1")
        with open(os.path.join(directory, "r1.json")) as handle:
            data = json.load(handle)
        self.assertEqual(data["head"], self.git("rev-parse", "HEAD").strip())
        self.assertEqual(data["branch"], "main")
        self.assertEqual(
            data["files"],
            {
                "src/lib.rs": sha("pub fn changed() {}\n"),
                "new.txt": sha("u\n"),
                "README.md": "DELETED",
                "link": sha(b"symlink:demo/a/golden"),
            },
        )

    def test_verify_identical_then_each_kind_of_change(self):
        self.assertEqual(self.cli("begin", "r1")[0], 0)
        self.assertEqual(self.cli("verify")[0], 0)
        self.assertEqual(self.cli("verify", "r1")[0], 0)
        self.put("src/lib.rs", "x\n")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("added: src/lib.rs", out)
        self.git("checkout", "--", "src/lib.rs")
        self.assertEqual(self.cli("verify")[0], 0)
        self.put("new.txt", "x\n")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("added: new.txt", out)
        os.remove(os.path.join(self.repo, "new.txt"))
        self.assertEqual(self.cli("verify")[0], 0)

    def test_a_path_dirty_at_baseline_and_edited_again_is_changed(self):
        self.put("src/lib.rs", "first\n")
        self.cli("begin", "r1")
        self.put("src/lib.rs", "second\n")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("changed: src/lib.rs", out)

    def test_changed_then_reverted_path_is_reported_as_removed(self):
        self.put("src/lib.rs", "dirty\n")
        self.cli("begin", "r1")
        self.git("checkout", "--", "src/lib.rs")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("removed: src/lib.rs", out)

    def test_moved_head_and_branch(self):
        self.cli("begin", "r1")
        self.put("n.txt", "n\n")
        self.git("add", "n.txt")
        self.commit("second")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("HEAD moved", out)
        self.git("checkout", "-q", "-b", "other")
        _, out, _ = self.cli("verify")
        self.assertIn("branch changed: main -> other", out)

    def test_staging_without_content_change_is_not_a_difference(self):
        self.put("new.txt", "u\n")
        self.cli("begin", "r1")
        self.git("add", "new.txt")
        self.assertEqual(self.cli("verify")[0], 0)

    def test_excludes_and_reviews_are_ignored(self):
        self.assertEqual(self.cli("begin", "r1", "--exclude", "crates/y/**", "--exclude", "mine.txt")[0], 0)
        self.put("crates/y/src/a.rs", "x\n")
        self.put("mine.txt", "x\n")
        self.put("docs/reviews/new.md", "x\n")
        self.put("docs/reviews/old.md", "changed\n")
        self.assertEqual(self.cli("verify")[0], 0)
        self.put("crates/z/a.rs", "x\n")
        code, out, _ = self.cli("verify")
        self.assertEqual(code, 1)
        self.assertIn("added: crates/z/a.rs", out)
        self.assertNotIn("crates/y", out)

    def test_begin_refuses_to_overwrite_a_young_baseline(self):
        self.assertEqual(self.cli("begin", "r1")[0], 0)
        code, _, err = self.cli("begin", "r1")
        self.assertEqual(code, 1)
        self.assertIn("--force", err)
        self.assertEqual(self.cli("begin", "r1", "--force")[0], 0)
        ri.begin(self.repo, "r2", now=time.time() - 13 * 3600)
        self.assertEqual(self.cli("begin", "r2")[0], 0)

    def test_invalid_ids_and_missing_baselines(self):
        self.assertEqual(self.cli("begin", "../x")[0], 1)
        self.assertEqual(self.cli("begin", ".hidden")[0], 1)
        code, _, err = self.cli("verify")
        self.assertEqual(code, 2)
        self.assertIn("no review baseline", err)
        self.assertEqual(self.cli("verify", "nope")[0], 2)
        self.assertEqual(self.cli("status")[0], 2)
        self.cli("begin", "r1")
        code, out, _ = self.cli("status")
        self.assertEqual(code, 0)
        self.assertIn("active baseline r1", out)

    def test_guard_blocks_without_a_baseline(self):
        code, err, _ = self.report()
        self.assertEqual(code, 2)
        self.assertIn("no review baseline exists", err)
        self.assertIn("review_integrity.py begin", err)

    def test_guard_blocks_an_old_baseline(self):
        ri.begin(self.repo, "r1", now=time.time() - 13 * 3600)
        code, err, _ = self.report()
        self.assertEqual(code, 2)
        self.assertIn("older than 12 hours", err)

    def test_guard_allows_on_an_intact_tree(self):
        self.cli("begin", "r1")
        for tool, rel in (
            ("fs_write", "docs/reviews/new.md"),
            ("fs_append", "docs/reviews/old.md"),
            ("str_replace", "docs/reviews/old.md"),
        ):
            with self.subTest(tool=tool):
                self.assertEqual(self.report(rel, tool), (0, "", ""))

    def test_guard_blocks_after_each_kind_of_change(self):
        self.cli("begin", "r1")
        self.put("docs/reviews/new.md", "written\n")
        self.assertEqual(self.report()[0], 0)
        self.put("src/lib.rs", "tampered\n")
        code, err, _ = self.report()
        self.assertEqual(code, 2)
        self.assertIn("added: src/lib.rs", err)
        self.git("checkout", "--", "src/lib.rs")
        self.assertEqual(self.report()[0], 0)
        self.put("extra.txt", "x\n")
        code, err, _ = self.report()
        self.assertIn("added: extra.txt", err)
        os.remove(os.path.join(self.repo, "extra.txt"))
        self.put("n.txt", "n\n")
        self.git("add", "n.txt")
        self.commit("moved")
        code, err, _ = self.report()
        self.assertEqual(code, 2)
        self.assertIn("HEAD moved", err)

    def test_guard_ignores_other_paths_and_resolves_traversal(self):
        self.assertEqual(self.report("src/lib.rs")[0], 0)
        self.assertEqual(self.report("docs/reviews/sub/x.md")[0], 0)
        self.assertEqual(self.report("docs/reviews/x.txt")[0], 0)
        self.assertEqual(self.report("docs/reviews/../../src/lib.rs")[0], 0)
        self.assertEqual(self.report("src/../docs/reviews/new.md")[0], 2)
        outside = self.pre("fs_write", path=os.path.join(self.tmp, "docs/reviews/r.md"), text="x")
        self.assertEqual(self.run_script("guard_review_report.py", outside)[0], 0)
        self.assertEqual(self.run_script("guard_review_report.py", None, raw="x")[0], 2)
        self.assertEqual(self.run_script("guard_review_report.py", self.pre("fs_write", text="x"))[0], 2)

    def test_guard_honours_the_baseline_excludes(self):
        self.cli("begin", "r1", "--exclude", "crates/y/**")
        self.put("crates/y/a.rs", "x\n")
        self.assertEqual(self.report()[0], 0)


class FmtCheck(RepoCase):
    def post(self, rel, tool="fs_write", env=None, absolute=True):
        path = os.path.join(self.repo, rel) if absolute else rel
        payload = self.pre(tool, path=path, text="x")
        payload["hook_event_name"] = "PostToolUse"
        payload["tool_response"] = "ok"
        return self.run_script("fmt_check.py", payload, extra_env=env)

    def test_unformatted_file_warns_with_the_diff_and_is_not_modified(self):
        source = "fn main(){let x=1;}\n"
        self.put("src/main.rs", source)
        code, err, _ = self.post("src/main.rs")
        self.assertEqual(code, 1)
        self.assertIn("src/main.rs is not rustfmt-clean", err)
        self.assertIn("-fn main(){let x=1;}", err)
        self.assertIn("+fn main() {", err)
        self.assertEqual(self.read("src/main.rs"), source)

    def test_formatted_file_passes(self):
        self.put("src/ok.rs", "fn main() {\n    let x = 1;\n}\n")
        self.assertEqual(self.post("src/ok.rs"), (0, "", ""))

    def test_only_the_written_file_is_reported_not_its_mod_children(self):
        self.put("src/lib2.rs", "mod child;\n\nfn main() {}\n")
        self.put("src/child.rs", "fn   bad( ){}\n")
        self.assertEqual(self.post("src/lib2.rs"), (0, "", ""))
        code, err, _ = self.post("src/child.rs")
        self.assertEqual(code, 1)
        self.assertIn("fn   bad", err)

    def test_child_difference_is_filtered_out_of_the_parent_report(self):
        self.put("src/lib2.rs", "mod child;\nfn   main( ){}\n")
        self.put("src/child.rs", "fn   bad( ){}\n")
        code, err, _ = self.post("src/lib2.rs")
        self.assertEqual(code, 1)
        self.assertIn("fn   main", err)
        self.assertNotIn("bad", err)

    def test_non_rust_missing_deleted_and_outside_paths_pass(self):
        self.put("notes.txt", "fn main(){}\n")
        self.assertEqual(self.post("notes.txt"), (0, "", ""))
        self.assertEqual(self.post("src/gone.rs", tool="delete_file"), (0, "", ""))
        outside = os.path.join(self.tmp, "o.rs")
        with open(outside, "w") as handle:
            handle.write("fn main(){}\n")
        self.assertEqual(self.post(outside, absolute=False), (0, "", ""))

    def test_missing_rustfmt_is_a_notice_not_a_failure(self):
        self.put("src/main.rs", "fn main(){}\n")
        only_git = os.path.join(self.tmp, "only-git")
        os.makedirs(only_git)
        os.symlink(shutil.which("git"), os.path.join(only_git, "git"))
        code, err, _ = self.post("src/main.rs", env={"PATH": only_git})
        self.assertEqual(code, 0)
        self.assertEqual(err, "fmt_check: rustfmt not found; format not checked\n")

    def test_syntax_error_is_reported_as_a_warning(self):
        self.put("src/bad.rs", "fn main( {\n")
        code, err, _ = self.post("src/bad.rs")
        self.assertEqual(code, 1)
        self.assertIn("could not check src/bad.rs", err)

    def test_non_write_tools_and_bad_input_are_ignored(self):
        payload = self.pre("execute_bash", command="x")
        self.assertEqual(self.run_script("fmt_check.py", payload), (0, "", ""))
        self.assertEqual(self.run_script("fmt_check.py", None, raw="zz")[0], 0)

    # Persistent warnings (.git/ruletrace-hooks/fmt-warnings.json)

    @property
    def hooks_dir(self):
        return os.path.join(self.repo, ".git", "ruletrace-hooks")

    @property
    def warnings_path(self):
        return os.path.join(self.hooks_dir, "fmt-warnings.json")

    def warnings(self):
        with open(self.warnings_path) as handle:
            return json.load(handle)

    BAD = "fn main(){let x=1;}\n"
    GOOD = "fn main() {\n    let x = 1;\n}\n"

    def test_warning_is_persisted_with_report_and_timestamp(self):
        self.assertFalse(os.path.exists(self.hooks_dir))
        self.put("src/main.rs", self.BAD)
        before = time.time()
        code, err, _ = self.post("src/main.rs")
        after = time.time()
        self.assertEqual(code, 1)
        entries = self.warnings()
        self.assertEqual(list(entries), ["src/main.rs"])
        self.assertEqual(entries["src/main.rs"]["report"], err)
        stamp = entries["src/main.rs"]["timestamp"]
        self.assertRegex(stamp, r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        seconds = calendar.timegm(time.strptime(stamp, "%Y-%m-%dT%H:%M:%SZ"))
        self.assertTrue(before - 2 <= seconds <= after + 2, stamp)

    def test_key_is_repo_relative_for_relative_and_nested_paths(self):
        self.put("crates/q/src/a.rs", self.BAD)
        self.post("crates/q/src/a.rs", absolute=False)
        self.assertEqual(list(self.warnings()), ["crates/q/src/a.rs"])

    def test_entry_is_removed_only_for_the_path_found_clean(self):
        self.put("src/a.rs", self.BAD)
        self.put("src/b.rs", self.BAD)
        self.post("src/a.rs")
        self.post("src/b.rs")
        self.assertEqual(sorted(self.warnings()), ["src/a.rs", "src/b.rs"])
        self.put("src/a.rs", self.GOOD)
        self.assertEqual(self.post("src/a.rs"), (0, "", ""))
        self.assertEqual(list(self.warnings()), ["src/b.rs"])

    def test_latest_report_replaces_the_earlier_one(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        self.put("src/a.rs", "fn other(){}\n")
        _, err, _ = self.post("src/a.rs")
        entry = self.warnings()["src/a.rs"]
        self.assertEqual(entry["report"], err)
        self.assertIn("fn other(){}", entry["report"])
        self.assertNotIn("let x=1", entry["report"])

    def test_deleted_file_entry_is_removed(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        os.remove(os.path.join(self.repo, "src/a.rs"))
        self.assertEqual(self.post("src/a.rs", tool="delete_file"), (0, "", ""))
        self.assertEqual(self.warnings(), {})

    def test_syntax_error_is_persisted_too(self):
        self.put("src/bad.rs", "fn main( {\n")
        _, err, _ = self.post("src/bad.rs")
        self.assertEqual(self.warnings()["src/bad.rs"]["report"], err)

    def test_clean_write_without_state_creates_nothing(self):
        self.put("src/ok.rs", self.GOOD)
        self.assertEqual(self.post("src/ok.rs"), (0, "", ""))
        self.assertFalse(os.path.exists(self.hooks_dir))

    def test_inconclusive_runs_leave_the_entry_alone(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        snapshot = self.warnings()
        only_git = os.path.join(self.tmp, "only-git")
        os.makedirs(only_git)
        os.symlink(shutil.which("git"), os.path.join(only_git, "git"))
        self.put("src/a.rs", self.GOOD)
        code, _, _ = self.post("src/a.rs", env={"PATH": only_git})
        self.assertEqual(code, 0)
        self.assertEqual(self.warnings(), snapshot)

    def test_non_rust_and_outside_paths_do_not_touch_state(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        snapshot = self.warnings()
        self.put("notes.txt", "x\n")
        self.post("notes.txt")
        outside = os.path.join(self.tmp, "o.rs")
        with open(outside, "w") as handle:
            handle.write(self.BAD)
        self.post(outside, absolute=False)
        self.assertEqual(self.warnings(), snapshot)

    def test_damaged_file_is_replaced_without_failing(self):
        os.makedirs(self.hooks_dir)
        with open(self.warnings_path, "w") as handle:
            handle.write("{not json")
        self.put("src/a.rs", self.BAD)
        code, err, _ = self.post("src/a.rs")
        self.assertEqual(code, 1)
        self.assertNotIn("cannot update", err)
        self.assertEqual(list(self.warnings()), ["src/a.rs"])

    def test_rewrite_is_atomic_replace_and_leaves_no_temporary_file(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        first = os.stat(self.warnings_path).st_ino
        self.put("src/b.rs", self.BAD)
        self.post("src/b.rs")
        self.assertNotEqual(os.stat(self.warnings_path).st_ino, first)
        self.assertEqual(
            sorted(n for n in os.listdir(self.hooks_dir) if n != ".lock"),
            ["fmt-warnings.json"],
        )

    def test_unwritable_directory_gives_one_notice_and_keeps_the_exit_code(self):
        with open(self.hooks_dir, "w") as handle:
            handle.write("in the way\n")
        self.put("src/a.rs", self.BAD)
        code, err, _ = self.post("src/a.rs")
        self.assertEqual(code, 1)
        self.assertIn("src/a.rs is not rustfmt-clean", err)
        self.assertEqual(err.count("fmt_check: cannot update the warnings file"), 1)
        self.assertTrue(err.endswith("\n"))

    def test_unwritable_state_on_a_clean_write_is_only_a_notice(self):
        self.put("src/a.rs", self.BAD)
        self.post("src/a.rs")
        os.remove(os.path.join(self.hooks_dir, ".lock"))
        os.mkdir(os.path.join(self.hooks_dir, ".lock"))
        self.put("src/a.rs", self.GOOD)
        code, err, _ = self.post("src/a.rs")
        self.assertEqual(code, 0)
        self.assertEqual(err.count("fmt_check: cannot update the warnings file"), 1)
        self.assertEqual(len(err.splitlines()), 1)
        self.assertEqual(list(self.warnings()), ["src/a.rs"])


class StopReport(RepoCase):
    def setUp(self):
        super().setUp()
        self.bin = os.path.join(self.tmp, "bin")
        os.makedirs(self.bin)
        self.log = os.path.join(self.tmp, "calls.log")
        fake = os.path.join(self.bin, "unshare")
        with open(fake, "w") as handle:
            handle.write(
                "#!%s\n"
                "import json, os, sys\n"
                "open(os.environ['FAKE_LOG'], 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "fail = os.environ.get('FAKE_FAIL', '')\n"
                "if fail and fail in sys.argv[-1]:\n"
                "    sys.stderr.write('check failed loudly\\n')\n"
                "    sys.exit(3)\n" % sys.executable
            )
        os.chmod(fake, 0o755)

    def stop(self, extra=None):
        env = {"PATH": self.bin + os.pathsep + os.environ["PATH"], "FAKE_LOG": self.log}
        env.update(extra or {})
        payload = {"session_id": "s", "hook_event_name": "Stop", "cwd": self.repo}
        code, err, out = self.run_script("stop_report.py", payload, extra_env=env)
        self.assertEqual(code, 0, err)
        return out

    def checks(self):
        if not os.path.exists(self.log):
            return []
        with open(self.log) as handle:
            return [json.loads(l)[-1] for l in handle]

    def test_clean_tree_runs_nothing_and_reports_state(self):
        out = self.stop()
        head = self.git("rev-parse", "--short", "HEAD").strip()
        self.assertIn("OK   branch main, HEAD %s" % head, out)
        self.assertIn("OK   working tree: 0 staged, 0 unstaged, 1 untracked", out)
        self.assertEqual(self.checks(), [])
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 0 warn"))

    def test_rust_change_runs_the_four_structure_checks_in_the_wrapper(self):
        self.put("src/lib.rs", "pub fn g() {}\n")
        out = self.stop()
        self.assertEqual(self.checks(), ["check-graph", "check-tree", "scan-sources", "scan-api"])
        with open(self.log) as handle:
            first = json.loads(handle.readline())
        self.assertEqual(
            first,
            ["-rn", "sh", "-c", 'ip link set lo up && exec "$@"', "sh",
             "cargo", "run", "-q", "-p", "rt-xtask", "--", "check-graph"],
        )
        self.assertIn("OK   check-graph passed", out)

    def test_documentation_change_runs_nothing(self):
        self.put("README.md", "edited\n")
        self.stop()
        self.assertEqual(self.checks(), [])

    def test_catalogue_decisions_and_dependency_files(self):
        self.put("catalogue/diagnostics.v1.json", "{ }\n")
        out = self.stop()
        self.assertEqual(self.checks(), ["check-catalogue"])
        self.assertIn("WARN catalogue changed: catalogue/diagnostics.v1.json", out)
        os.remove(self.log)
        self.git("checkout", "--", "catalogue/diagnostics.v1.json")
        self.put("tools/decisions/UD-9.md", "d\n")
        out = self.stop()
        self.assertEqual(self.checks(), ["check-decisions"])
        self.assertIn("WARN decision record changed: tools/decisions/UD-9.md", out)
        os.remove(self.log)
        os.remove(os.path.join(self.repo, "tools/decisions/UD-9.md"))
        self.put("tools/dependencies.v1", "d\n")
        self.stop()
        self.assertEqual(
            self.checks(),
            ["check-graph", "check-tree", "scan-sources", "scan-api", "check-justification"],
        )

    def test_cargo_file_triggers_justification(self):
        self.put("crates/x/Cargo.toml", "[package]\n")
        out = self.stop()
        self.assertEqual(self.checks()[-1], "check-justification")
        self.assertIn("WARN architecture edges and policy changed: crates/x/Cargo.toml", out)

    def test_generated_catalogue_source_triggers_both(self):
        self.put("crates/rt-foundation/src/code_generated.rs", "// g\n")
        out = self.stop()
        self.assertEqual(
            self.checks(),
            ["check-graph", "check-tree", "scan-sources", "scan-api", "check-catalogue"],
        )
        self.assertIn("catalogue changed: crates/rt-foundation/src/code_generated.rs", out)

    def test_notice_classes(self):
        self.put("demo/a/golden/f", "changed\n")
        self.put(".github/workflows/ci.yml", "x\n")
        self.put(".kiro/specs/s/tasks.md", "x\n")
        out = self.stop()
        self.assertIn("WARN golden/fixture/recording changed: demo/a/golden/f", out)
        self.assertIn("WARN architecture edges and policy changed: .github/workflows/ci.yml", out)
        self.assertIn("WARN spec file changed: .kiro/specs/s/tasks.md", out)
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 3 warn"))
        self.assertLess(out.index("NOTICE"), out.index("WARN golden"))

    def test_failing_check_is_labelled_fail_and_exit_is_still_zero(self):
        self.put("src/lib.rs", "pub fn g() {}\n")
        out = self.stop({"FAKE_FAIL": "scan-sources"})
        self.assertIn("FAIL scan-sources exited 3: check failed loudly", out)
        self.assertIn("OK   scan-api passed", out)
        self.assertTrue(out.rstrip().endswith("stop-report: 1 fail, 0 warn"))

    def test_missing_wrapper_is_reported_clearly(self):
        self.put("src/lib.rs", "pub fn g() {}\n")
        only_git = os.path.join(self.tmp, "only-git")
        os.makedirs(only_git)
        os.symlink(shutil.which("git"), os.path.join(only_git, "git"))
        out = self.stop({"PATH": only_git})
        self.assertIn("`unshare` is not available", out)
        self.assertEqual(out.count("not available"), 1)

    def test_review_baseline_failures_are_listed(self):
        ri.begin(self.repo, "r1")
        self.put("n.txt", "n\n")
        out = self.stop()
        self.assertIn("FAIL review baseline r1: added: n.txt", out)
        self.assertTrue(out.rstrip().endswith("stop-report: 1 fail, 0 warn"))

    def test_intact_review_baseline_is_ok(self):
        ri.begin(self.repo, "r1")
        self.assertIn("OK   review baseline r1 is intact", self.stop())

    # Persistent report file

    @property
    def hooks_dir(self):
        return os.path.join(self.repo, ".git", "ruletrace-hooks")

    @property
    def report_path(self):
        return os.path.join(self.hooks_dir, "last-stop-report.txt")

    def stop_full(self, extra=None):
        env = {"PATH": self.bin + os.pathsep + os.environ["PATH"], "FAKE_LOG": self.log}
        env.update(extra or {})
        payload = {"session_id": "s", "hook_event_name": "Stop", "cwd": self.repo}
        return self.run_script("stop_report.py", payload, extra_env=env)

    def test_complete_report_is_written_to_the_file_and_the_directory_is_created(self):
        self.put("README.md", "edited\n")
        self.assertFalse(os.path.exists(self.hooks_dir))
        code, err, out = self.stop_full()
        self.assertEqual((code, err), (0, ""))
        with open(self.report_path) as handle:
            self.assertEqual(handle.read(), out)
        self.assertIn("OK   status  M README.md", out)
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 0 warn"))

    def test_report_file_is_replaced_atomically_not_appended(self):
        self.put("README.md", "edited\n")
        _, _, first = self.stop_full()
        inode = os.stat(self.report_path).st_ino
        self.put("n1.txt", "n\n")
        _, _, second = self.stop_full()
        with open(self.report_path) as handle:
            text = handle.read()
        self.assertEqual(text, second)
        self.assertIn("n1.txt", text)
        self.assertNotIn("n1.txt", first)
        self.assertEqual(text.count("stop-report:"), 1)
        self.assertNotEqual(os.stat(self.report_path).st_ino, inode)
        self.assertEqual(
            sorted(n for n in os.listdir(self.hooks_dir) if n != ".lock"),
            ["last-stop-report.txt"],
        )

    def test_unwritable_location_gives_one_notice_and_the_same_stdout_and_exit(self):
        with open(self.hooks_dir, "w") as handle:
            handle.write("in the way\n")
        self.put("README.md", "edited\n")
        code, err, out = self.stop_full()
        self.assertEqual(code, 0)
        self.assertEqual(err.count("stop-report: cannot write the report file"), 1)
        self.assertEqual(len(err.splitlines()), 1)
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 0 warn"))
        self.assertIn("OK   status  M README.md", out)

    def test_outside_a_repository_the_failure_is_printed_with_one_notice(self):
        payload = {"session_id": "s", "hook_event_name": "Stop", "cwd": self.tmp}
        code, err, out = self.run_script("stop_report.py", payload, cwd=self.tmp)
        self.assertEqual(code, 0)
        self.assertIn("FAIL stop-report could not inspect the repository", out)
        self.assertEqual(err.count("cannot write the report file"), 1)

    # FORMAT section

    BAD = "fn main(){let x=1;}\n"
    GOOD = "fn main() {\n    let x = 1;\n}\n"

    def fake_rustfmt(self, slow=None):
        real = shutil.which("rustfmt")
        self.rustfmt_log = os.path.join(self.tmp, "rustfmt.log")
        path = os.path.join(self.bin, "rustfmt")
        with open(path, "w") as handle:
            handle.write(
                "#!%s\n"
                "import json, os, sys, time\n"
                "open(%r, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if %r and %r in sys.argv[-1]:\n"
                "    time.sleep(40)\n"
                "os.execv(%r, ['rustfmt'] + sys.argv[1:])\n"
                % (sys.executable, self.rustfmt_log, slow, slow, real)
            )
        os.chmod(path, 0o755)

    def rustfmt_calls(self):
        if not os.path.exists(self.rustfmt_log):
            return []
        with open(self.rustfmt_log) as handle:
            return [json.loads(l) for l in handle]

    def format_lines(self, out):
        lines = out.splitlines()
        start = lines.index("FORMAT")
        return [l for l in lines[start + 1:] if not l.startswith("stop-report:")]

    def commit_file(self, rel, text):
        self.put(rel, text)
        self.git("add", rel)
        self.commit("add " + rel)

    def test_format_section_labels_clean_and_unclean_changed_files(self):
        self.put("src/lib.rs", self.GOOD)
        self.put("src/bad.rs", self.BAD)
        out = self.stop()
        lines = self.format_lines(out)
        self.assertIn("OK   format src/lib.rs", lines)
        self.assertIn("WARN format src/bad.rs: not rustfmt-clean", lines)
        self.assertIn("    -fn main(){let x=1;}", lines)
        self.assertIn("    +fn main() {", lines)
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 1 warn"))

    def test_format_never_modifies_files_and_uses_check_only(self):
        self.fake_rustfmt()
        self.put("src/bad.rs", self.BAD)
        path = os.path.join(self.repo, "src/bad.rs")
        before = (self.read("src/bad.rs"), os.stat(path).st_mtime_ns)
        status = self.git("status", "--porcelain")
        self.stop()
        self.assertEqual((self.read("src/bad.rs"), os.stat(path).st_mtime_ns), before)
        self.assertEqual(self.git("status", "--porcelain"), status)
        self.assertEqual(
            self.rustfmt_calls(),
            [["--edition", "2021", "--color", "never", "--check", path]],
        )

    def test_format_covers_modified_staged_and_untracked_only(self):
        self.commit_file("src/committed_bad.rs", self.BAD)
        self.commit_file("src/gone.rs", self.GOOD)
        os.remove(os.path.join(self.repo, "src/gone.rs"))
        self.put("src/lib.rs", self.GOOD)
        self.put("src/staged.rs", self.BAD)
        self.git("add", "src/staged.rs")
        self.put("src/new.rs", self.BAD)
        self.put("notes.txt", self.BAD)
        out = self.stop()
        labelled = [
            l for l in self.format_lines(out) if l.startswith(("OK   format", "WARN format"))
        ]
        self.assertEqual(
            labelled,
            [
                "OK   format src/lib.rs",
                "WARN format src/new.rs: not rustfmt-clean",
                "WARN format src/staged.rs: not rustfmt-clean",
            ],
        )

    def test_format_reports_only_the_changed_files_own_hunks(self):
        self.put("src/parent.rs", "mod child;\n\nfn main() {}\n")
        self.put("src/child.rs", "fn   bad( ){}\n")
        lines = self.format_lines(self.stop())
        self.assertIn("OK   format src/parent.rs", lines)
        self.assertIn("WARN format src/child.rs: not rustfmt-clean", lines)
        self.assertEqual(sum("fn   bad" in l for l in lines), 1)

    def test_format_syntax_error_is_a_warning(self):
        self.put("src/broken.rs", "fn main( {\n")
        out = self.stop()
        self.assertIn("WARN format src/broken.rs: rustfmt could not check it", out)
        self.assertTrue(out.rstrip().endswith("0 fail, 1 warn"))

    def test_format_checks_at_most_forty_files(self):
        self.fake_rustfmt()
        for i in range(45):
            self.put("src/g/f%02d.rs" % i, self.GOOD)
        out = self.stop()
        oks = [l for l in self.format_lines(out) if l.startswith("OK   format")]
        self.assertEqual(oks, ["OK   format src/g/f%02d.rs" % i for i in range(40)])
        self.assertEqual(len(self.rustfmt_calls()), 40)
        self.assertIn("WARN format: 5 more .rs files not checked (limit 40)", out)
        self.assertTrue(out.rstrip().endswith("0 fail, 1 warn"))

    def test_format_with_no_rust_change(self):
        self.put("README.md", "edited\n")
        lines = self.format_lines(self.stop())
        self.assertEqual(lines[0], "OK   no changed .rs file to format-check")

    def test_missing_rustfmt_is_one_warning(self):
        self.put("src/a.rs", self.GOOD)
        self.put("src/b.rs", self.GOOD)
        only_git = os.path.join(self.tmp, "only-git")
        os.makedirs(only_git)
        os.symlink(shutil.which("git"), os.path.join(only_git, "git"))
        out = self.stop({"PATH": only_git})
        self.assertEqual(out.count("WARN rustfmt not found; format not checked"), 1)
        self.assertNotIn("OK   format", out)

    def test_slow_file_times_out_after_twenty_seconds_and_the_next_file_is_checked(self):
        self.fake_rustfmt(slow="slow")
        self.put("src/aaa_slow.rs", self.GOOD)
        self.put("src/zzz.rs", self.GOOD)
        started = time.monotonic()
        out = self.stop()
        elapsed = time.monotonic() - started
        self.assertIn(
            "WARN format src/aaa_slow.rs: rustfmt timed out after 20 s; format not checked",
            out,
        )
        self.assertIn("OK   format src/zzz.rs", out)
        self.assertTrue(20 <= elapsed < 38, elapsed)

    def test_pending_warnings_are_listed_and_not_modified(self):
        os.makedirs(self.hooks_dir)
        entries = {
            "src/a.rs": {"report": "r1", "timestamp": "2025-01-02T03:04:05Z"},
            "src/b.rs": {"report": "r2", "timestamp": "2025-02-03T04:05:06Z"},
        }
        target = os.path.join(self.hooks_dir, "fmt-warnings.json")
        with open(target, "w") as handle:
            json.dump(entries, handle)
        with open(target, "rb") as handle:
            before = handle.read()
        out = self.stop()
        lines = self.format_lines(out)
        self.assertIn("WARN fmt-warnings pending: src/a.rs (since 2025-01-02T03:04:05Z)", lines)
        self.assertIn("WARN fmt-warnings pending: src/b.rs (since 2025-02-03T04:05:06Z)", lines)
        self.assertNotIn("OK   no pending fmt-warnings entry", lines)
        self.assertTrue(out.rstrip().endswith("stop-report: 0 fail, 2 warn"))
        with open(target, "rb") as handle:
            self.assertEqual(handle.read(), before)

    def test_no_pending_warnings_is_an_ok_line(self):
        lines = self.format_lines(self.stop())
        self.assertIn("OK   no pending fmt-warnings entry", lines)

    def test_damaged_warnings_file_is_a_warning(self):
        os.makedirs(self.hooks_dir)
        with open(os.path.join(self.hooks_dir, "fmt-warnings.json"), "w") as handle:
            handle.write("[1]")
        out = self.stop()
        self.assertIn("WARN fmt-warnings.json: ", out)
        self.assertIn("is not a JSON object", out)

    def test_fmt_check_entries_show_up_in_the_stop_report(self):
        self.put("src/a.rs", self.BAD)
        payload = self.pre("fs_write", path=os.path.join(self.repo, "src/a.rs"), text="x")
        self.assertEqual(self.run_script("fmt_check.py", payload)[0], 1)
        out = self.stop()
        self.assertRegex(
            out, r"WARN fmt-warnings pending: src/a\.rs \(since \d{4}-\d\d-\d\dT"
        )

    def test_format_section_comes_before_the_review_baseline_result(self):
        ri.begin(self.repo, "r1")
        out = self.stop()
        self.assertLess(out.index("FORMAT"), out.index("review baseline r1"))


if __name__ == "__main__":
    unittest.main()
