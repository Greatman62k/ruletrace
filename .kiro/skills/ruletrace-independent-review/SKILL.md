---
name: ruletrace-independent-review
description: Run an independent RuleTrace code review of a commit range with the ruletrace-reviewer agent. Use when an implementation increment is ready for review, before a task is marked complete, or when asked to review RuleTrace commits.
---

# Independent review procedure

Roles: the orchestrator prepares and checks; the `ruletrace-reviewer` agent
reviews. The implementer's report is not review evidence.

## Orchestrator, before the review

1. Choose a review id, for example `9.2-<short sha>`.
2. Record the integrity baseline of the main worktree:
   `python3 -B tools/hooks/review_integrity.py begin <review-id>`.
   Add `--exclude <glob>` only for paths a concurrent implementer owns.
3. Write the review brief: the commit range, the binding requirements and
   design references (quoted or cited), the report file name
   `docs/reviews/<date>-review-<slug>.md`. Do not paste the implementer's
   conclusions. Facts the reviewer must check may be listed as claims.

## Reviewer

Follow `.kiro/agents/prompts/reviewer.md`: separate worktree under
`~/review-wt-*`, its own target directory outside the repository, all test
runs inside `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh <cmd>`,
own mutants, the nine-heading report.

## Orchestrator, after the review

1. The `review-integrity` hook blocks the report write if the main worktree or
   `HEAD` changed since the baseline. If it blocked, find out what changed
   before anything else.
2. Run `python3 -B tools/hooks/review_integrity.py verify <review-id>` and
   `git worktree list` (the scratch worktree must be gone).
3. Commit the report. Fix blocking findings through the implementer, then
   review the fix (a new review id).

Known limits: permissions cannot stop a sub-agent's shell redirect into the
main worktree; the integrity check detects it at report time.
