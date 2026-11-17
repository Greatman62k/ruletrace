---
name: ruletrace-ci-inspection
description: Inspect and diagnose RuleTrace GitHub Actions runs read-only with the gh CLI. Use when asked about CI status, a failed or cancelled job, platform results, or the evidence for a checkpoint.
---

# Read-only CI inspection

Use only read commands. Reruns, cancellations, workflow dispatches and any
mutating `gh api` call are blocked by `guard-shell` and are the user's decision.

```sh
gh run list --limit 10 --json databaseId,headSha,headBranch,event,status,conclusion
gh run view <run> --json jobs --jq '.jobs[]|[.name,.status,.conclusion]|@tsv'
gh run view <run> --log-failed
gh api "repos/<owner>/<repo>/actions/runs/<run>/attempts/<n>/jobs?per_page=50"
gh api repos/<owner>/<repo>/check-runs/<job-id>/annotations
gh api repos/<owner>/<repo>/actions/jobs/<job-id>/logs
```

## Diagnosis of a failed or cancelled job

1. Record run id, attempt, job id, name, created, started and completed times,
   runner name and group, and the step list.
2. Read the annotations. "The job was not acquired by Runner of type hosted"
   with zero steps and no runner means no runner was assigned (capacity), not
   a code failure.
3. A job with steps: find the first failed step and its log lines.
4. Check `concurrency`, `cancel-in-progress` and `timeout-minutes` in
   `.github/workflows/ci.yml` before blaming infrastructure.
5. Check `https://www.githubstatus.com/api/v2/incidents.json` for the time.
6. Classify: no runner, initialisation failure, stalled step, timeout,
   external or concurrency cancellation, or a genuine test failure. Give the
   evidence for the class.

Report the table of jobs and the evidence before proposing a rerun. One
controlled rerun at most, and only with the user's approval.
