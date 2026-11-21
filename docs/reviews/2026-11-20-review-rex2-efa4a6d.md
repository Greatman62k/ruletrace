# Review rex2-efa4a6d: follow-up to the real-executables record

## 1. Verdict
APPROVED. No blocking findings.

## 2. Scope
Main worktree, branch m3, HEAD efa4a6d, read-only (git and grep only; no cargo,
no CI). Examined `docs/security/2026-11-20-lt7-lifeline-real-executables.md`
lines 12-18, 40-46, `crates/rt-cli/tests/lifeline.rs`, `git show --stat efa4a6d`,
`docs/reviews/2026-11-20-review-life2-1070445.md`.

## 3. Blocking findings
None.

## 4. Findings
- N1 confirmed. `crates/rt-cli/tests/lifeline.rs:24` has
  `MIN_RENDER_BYTES: usize = 1 << 20` (exactly 1 MiB) and `:153` asserts
  `plain.stdout.len() >= MIN_RENDER_BYTES`. "The test asserts at least 1 MiB"
  matches (doc line 43-44). The "about 1.8 MiB in the implementer's local run"
  is a claim I could not check without running the test; it is labelled as the
  implementer's. Note the assertion measures the unwatched control run's output;
  the doc sentence about the child blocking on an unread pipe follows from that
  size exceeding pipe buffers, which the assertion message states.
- N2 partly confirmed.
  - Confirmed: `git show --stat efa4a6d` lists 19 paths: catalogue row,
    `rt-ai-tool` (7 files), `rt-cli` (8 files incl. `lifeline_rig/mod.rs`),
    `rt-foundation` (3 files), and the two review reports. All five files the
    life2 review names as examined are in the commit. `git diff 1070445 efa4a6d
    --stat` is identical to that list, so the commit adds exactly those paths
    on top of 1070445.
  - Confirmed: the commit message ends "Reviews: life-1070445 (APPROVED,
    N1-N5), life2-1070445 (APPROVED)". life2 report scope says "uncommitted
    changes on branch m3 (HEAD 1070445)", matching the doc's wording.
  - Not confirmable: that the `life2-1070445` integrity check passed
    "immediately before" the commit. The life2 report records HEAD and status
    as unchanged before and after the review, which supports the pass, but the
    timing relative to the commit is not recorded in any file I read, and a past
    baseline cannot be re-run. The life2 review also examined only five named
    files, not all 19 paths; the doc says "examined the uncommitted tree",
    which is slightly broader than that report's stated scope (non-blocking).
- N3: the cited report `docs/reviews/2026-11-20-review-rex-efa4a6d.md` exists
  (untracked).
- Nothing else changed. Working tree: modified `.kiro/specs/ruletrace-compiler/
  tasks.md`, `traceability.md`, `docs/security/laboratory-threat-model.md`;
  untracked the rex report and the real-executables record. No source, test,
  catalogue or golden file is modified (`git diff HEAD --stat` shows only the
  three docs). The record itself is untracked, so I could not diff the two
  sentences against its earlier text; I checked their current content only.

## 5. Not verified
The 1.8 MiB figure; the timing of the life2 integrity pass; the earlier
wording of the record (no tracked copy).

## 6. Integrity (baseline rex2-efa4a6d)
HEAD before and after: efa4a6d84fc1709c303c19b631d512b1bc524b68. Status before:
the three modified docs plus the two untracked files above; after: the same plus
this report.
