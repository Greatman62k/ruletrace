# Review doc-66ef6ec: fixes F1-F7 of the L1 fix review, and the final documentation set at 439b49d

Reviewer: independent reviewer (`ruletrace-reviewer`). Documents only. No cargo command, test or mutant was run. No scratch worktree was created.

## 1. Verdict

**APPROVED WITH FINDINGS.** Of F1 to F7, six are resolved and one (F2, the mutant half) is partly resolved. Commit `66ef6ec` introduces no contradiction and decides no OPEN item. Four low findings remain (R1 to R4), two of them in text that later commits touched. None blocks the user's decisions on LT-1 to LT-9 or the start of task 13.1 review closure.

| F | Source finding | Status | Evidence at HEAD `439b49d` |
| --- | --- | --- | --- |
| F1 | RN-33 lacks the second question | **Resolved** | `design.md:3146` now ends "Second question (§11.7): decisions recorded in the session copy are deleted at session end; choose a workspace ledger written directly by `ruletrace review`, or an export". It matches `design.md:3369` ("RN-33 asks the user to choose"). |
| F2 | Limits not labelled; no mutants for 36.4, 36.6, 36.9 | **Partly resolved** | Labels: resolved. `design.md:3402-3404` read "design value, LT-9"; `threat-model:153` reads "design values, not requirement text (LT-9)"; LT-9 exists at `threat-model:351`, with "LT-1 to LT-9" in `design.md:45`, `:3087`, `:3215`, `:3512` and `threat-model:2`. Mutants: RN-48 (`design.md:3161`) now says "(also 36.4, 36.6, 36.9)", but the test plan at `design.md:3501` still lists only 6, 21, 24, 25, 26 as additions. See R1. |
| F3 | T30 lacks the session-end exemption | **Resolved** | `threat-model:197` now reads "One workflow, review or upload operation at a time per session (`POST /session/end` pre-empts, RN-45)". It matches `threat-model:323-326` and `design.md:3341`. |
| F4 | T12b "every POST answers 303" | **Resolved, with a residual (R3)** | `threat-model:155` reads "every accepted POST answers 303 to a GET (a refused POST answers 4xx)". |
| F5 | Launch_Secret exchange during a step not in RN-45 | **Resolved** | `design.md:3158` adds "a Launch_Secret exchange during a step gets 401 under 37.6, not 409 under 37.25 (design §11.5)". It matches `design.md:3338`, `design.md:3325` (`POST /session` exempt from CSRF per 37.9) and `threat-model:312-314`. |
| F6 | T23 "owner-only ACL" asserted before verified | **Resolved** | `threat-model:180` now reads "inherits the ACL of that directory, which may also grant SYSTEM and Administrators; L2 must verify who else has access". No other "owner-only" claim about the Windows session directory exists (grep of all `*.md` outside `docs/reviews`). |
| F7 | Author note inside the user's decision row | **Resolved** | The parenthetical was removed from the "Limits" row (`traceability.md:996`). The check result is now a separate paragraph, "Note by the design author, not part of the user's decision" (`traceability.md:1008`). The row still says "chosen after checking the planned snapshots and fixtures" after "Added:". That is a description of the author's addition, not a claim of fact, so I leave it. |

## 2. Scope

- Commit range `e5dd33a..66ef6ec` (three files: `design.md`, `traceability.md`, `laboratory-threat-model.md`; 18 insertions, 17 deletions), then `66ef6ec..439b49d` for the later commits `a4628bd`, `bf44576`, `439b49d` and the review commit `de92189`.
- Read in full or by targeted line: the diff `e5dd33a..66ef6ec` (`git diff -U0`); the earlier review `2026-11-18-review-L1-fix-6f7e851.md` (sections 1 to 5); the review `2026-11-18-review-tasks-a4628bd.md` (sections 1 to 4); `requirements.md` Req 7.16/7.17, 24.13, 27.1, 36 (criteria 2, 4, 6, 9, 21), 37 (criteria 5, 6, 9, 10, 13, 14, 17, 23, 25, 26); `design.md` §9.4 (RN-31 to RN-49, lines 3144-3162), §11.5, §11.7, §11.9, §11.16, §11.17 index; `docs/security/laboratory-threat-model.md` (T12, T12b, T23, T30, §7, §9, §10); `tools/decisions/UD-38.md` §8; `tasks.md` lines 142, 1080, 1088, 1162, 1193, 1417, 1558-1613; `traceability.md` lines 990-1008; `docs/kiro-usage-log.md:58`.
- Not read: the full `requirements.md` and `design.md` outside the lines above; `tasks.md` outside Phase 13 and the 9.x/11.x notes listed. I searched all `*.md`, `*.v1` files outside `docs/reviews` with `grep -rnE` for `LT-`, `RN-3x/4x`, `RT-LAB`, `owner-only|ACL`, `303`.

## 3. Blocking findings

None.

## 4. Non-blocking findings

Severity is low for all.

- **R1 (F2 residual). The test plan does not carry the 36.4, 36.6, 36.9 mutants.** `design.md:3501` says "The design adds mutants for 6, 21, 24, 25 and 26 ...; extending 37.23 to them is RN-48". RN-48 at `design.md:3161` now also names 36.4, 36.6 and 36.9. `tasks.md:1598` (13.6) says "those that RN-48 adds", which covers them by reference. `tasks.md:1585` (13.4) lists only 37.x criteria. A reader of §11.16 alone would not see the Req 36 mutants. Also, RN-48 speaks of extending "the mutant list of 37.23", which is a Req 37 criterion; the Req 36 criteria would need either an amendment of 36.4/36.6/36.9 or a wording that 37.23 names them. Fix: add "36.4, 36.6, 36.9" to `design.md:3501` and say in RN-48 which criterion text is amended.
- **R2. `tasks.md:1567` is stale after `bf44576`.** It reads "Open: LT-1 to LT-9 and RN-31 to RN-48 (user)". RN-49 was added in `bf44576` (`design.md:3162`) and `tasks.md:1580` itself cites RN-49. `design.md:3087` and `:3515` were updated to RN-49. Fix: "RN-31 to RN-49".
- **R3. T12b "a refused POST answers 4xx" is not exact.** Two refusals in the design produce no HTTP response: the connection cap ("closed without a response", `design.md:3404`, `threat-model:153`) and the header and body deadlines ("connection closed", `design.md:3402-3403`). The statement is about requests that reached a handler, so it is a wording gap, not a contradiction. Fix: "a refused request that reaches a handler answers 4xx".
- **R4. Small index and numbering inconsistencies.**
  - `threat-model:351-352` lists LT-9 before LT-8 in the §9 table.
  - `design.md:3396` heading names "LT-8, OPEN" only, but the same section's table (`:3402-3404`) now carries LT-9.
  - `tools/decisions/UD-38.md:132` says "tasks L5". `tasks.md` has no L5; the labels L1 to L4 map to 13.1, 13.4, 13.5, 13.6, and distribution is 13.7 (`tasks.md:1560`, `:1604`). This predates `66ef6ec`.
  - `docs/kiro-usage-log.md:58` says the reviewer reviewed "the L1 threat model and design §11 three times", and lists `...-tasks-a4628bd.md` among them. That review compared `tasks.md` (and the threat-model status and review table), not the threat model itself. It also says the `66ef6ec` fixes were "re-reviewed"; `66ef6ec` was reviewed for the first time by this report. `threat-model:359` states this correctly ("not separately reviewed").
- **R5 (record only). `threat-model` §10 will need one more row** for this report, and its status line (`threat-model:2`) says "independently reviewed (APPROVED WITH FINDINGS after fixes)" before `66ef6ec` had a review. The statement becomes true with this report, with the findings above.

## 5. What held up

### 5.1 No contradiction introduced by `66ef6ec`

| Check | Result | How verified |
| --- | --- | --- |
| LT numbering LT-1 to LT-9, consistent | Holds | `grep -rnE 'LT-1 to LT-[0-8]'` over `*.md` outside `docs/reviews` returns no hit. Every range says LT-1 to LT-9: `design.md:45`, `:3087`, `:3215`, `:3512`; `threat-model:2`; `tasks.md:1564` ("LT-1 to LT-3 ... decided LT-4 to LT-9 or recorded each as deferred"), `:1567`. `UD-38.md:123-134` names only LT-1 to LT-7 individually, which is consistent. |
| RN numbers cited elsewhere exist in §9.4 | Holds | RN-31 to RN-49 are rows `design.md:3144-3162`. Citations: RN-33 (`design.md:3262`, `:3263`, `:3369`; `UD-38.md:126`), RN-35 (`UD-38.md:131`), RN-36/37/38/42/44 (`tasks.md:1080`, `:1417`, `:1584`; `design.md:3286`), RN-40 (`UD-38.md:130`), RN-45 (`threat-model:197`, `:326`), RN-46 (`design.md:3504`), RN-47 (`threat-model:317`, `:348`), RN-48 (`design.md:3418`, `:3501`, `tasks.md:1598`), RN-49 (`design.md:3497`, `tasks.md:1580`). No citation of a missing RN. |
| 36.4, 36.6, 36.9 exist and match what RN-48 means | Holds | `requirements.md` Req 36 criterion 4 (ledger and Rulebook change only through review subprocess, hash test), 6 (confirmation names rule, revision, hash), 9 (hand-off hash check). 36.4 and 36.6 are checks that a mutant can remove; 36.9 likewise. |
| RN-45 premise is true | Holds | `requirements.md` 37.9 makes any POST state-changing and exempts only the Launch_Secret exchange from CSRF; 37.25 serializes "at most one state-changing request"; 37.17 requires killing running subprocesses; 37.6 requires 401 for a second presentation. The RN states the conflict and the chosen reading as a question to the user. |
| RN-49 premise is true | Holds | `requirements.md` 7.16 lists "RT-DGN, RT-PRV, RT-APR, and RT-AI, and the families that the design assigns to the Diagnostics of Requirements 1 to 14, 19, and 21". Req 36 and 37 are outside that list. `grep -rn RT-LAB` finds the family only in `design.md:3162`, `:3497`, `tasks.md:1577`, `:1580`; it is not in `catalogue/` or `tools/`, so no code is pre-claimed. |
| RN-33 does not conflict with 36.4 as written | Holds | RN-33 is itself the recorded conflict (`design.md:3146`); `design.md:3369` states that a byte-identical copy is a write "in the literal sense" and does it only for the initial copy. |
| No OPEN item presented as decided | Holds | `threat-model:344-352`: LT-1 to LT-9 all **OPEN**. Recommendations are marked "recommended" (`threat-model:254`, `:316`; `design.md:3338`, `:3426`). RN-31 to RN-49 are "User decision" rows. LT-9's recommendation "As stated (design values)" is a proposal. `tasks.md:1567` says "Not complete until the user decides them". `design.md:3215` says no Laboratory code is written before the user's decisions. |
| No requirement claimed satisfied | Holds | I searched §11 of `design.md` and the threat model for "satisf", "is met", "are met", "verified by", "was run". The hits are statements of non-satisfaction (`threat-model:247`, M5 "does not satisfy" the orchestration statement for live runs), planned tests, or "no test was run for this review" (`design.md` §11.14). `tasks.md:1088` keeps 9.6 `[-]` with the disclosure that Req 32.3 and 32.7 stay unsatisfied. `tasks.md` 13.1 is `[-]`; 13.2 to 13.8 are `[ ]`. |
| Req 24.13, 27, 28, 31, 33, 34, 7.16 not altered or weakened | Holds | `git diff --stat 66ef6ec..HEAD` and `e5dd33a..66ef6ec` show no change to `requirements.md` or `tools/`. 24.13's edge list is still shown in RN-44 as a mismatch, not edited. |
| Windows and limits statements add no new "verified" claims | Holds | T23 says "L2 must verify"; T12 limits are labelled design values; none states a test passed. |

### 5.2 Later commits `a4628bd`, `bf44576`, `439b49d`

- `a4628bd` and `bf44576` touch `tasks.md` (Phase 13, 9.x notes, line 142), `design.md` (RN-49; three RN-range updates), the threat-model status and review table. `439b49d` adds one row to `docs/kiro-usage-log.md`.
- The task-plan review's findings F1 to F5 are reflected at HEAD: 13.1 now requires LT-1 to LT-3 decided and allows LT-4 to LT-9 to be deferred with their consequence (`tasks.md:1564`); line 142 now reads "and also 13.2 when LT-1 selects M2", matching 13.4's blocker (`tasks.md:1586`). I did not re-run that review's other checks.
- Items they touch are consistent with `66ef6ec` except R2 and R5 above and the usage-log wording in R4.

## 6. Commands run

All read-only. Exit code 0 for each, except the first.

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD && git status --porcelain && git branch --show-current ...` (first attempt) | -1, blocked by the agent permission rule on `git branch`; re-run without it |
| `git rev-parse HEAD`, `git status --porcelain`, `git log --oneline e5dd33a..HEAD`, `git diff --stat e5dd33a..66ef6ec`, `ls docs/reviews` | 0 |
| `git diff -U0 e5dd33a..66ef6ec`, `git diff -U0 66ef6ec..HEAD -- design.md docs/security`, `git diff --stat 66ef6ec..HEAD` | 0 |
| `git diff 66ef6ec..HEAD -- docs/kiro-usage-log.md` | 0 |
| `grep -rnE` for `LT-[0-9]`, `RN-(3[1-9]|4[0-9])`, `RT-LAB`, `owner-only|ACL`, `303`, `LT-1 to LT-[0-8]` over `*.md`/`*.v1`/`*.json`/`*.toml` | 0 |
| `sed -n` and `awk` line reads of `design.md`, `requirements.md`, `tasks.md`, `traceability.md`, `laboratory-threat-model.md`, `UD-38.md` | 0 |

## 7. Mutants run

None. The change is documentation only and no executable behaviour or test was touched.

## 8. Not verified

- Whether the user wants RN-33's second question answered as "workspace ledger" or "export"; no decision exists and none is assumed.
- Whether the Windows temporary-directory ACL grants SYSTEM and Administrators. The text now says "may" and defers to L2; I did not test it.
- The truth of the design's source-code claims (for example `evaluate.rs` `classify`, `rt-cli` argument parsers) quoted in RN-31 to RN-38. These were outside the focus of `66ef6ec` and I did not re-read the code.
- The rest of `requirements.md`, `design.md` and `tasks.md` for stale references beyond the searched patterns. In particular I searched for RN and LT ranges only; other counts (for example "three runtime executables") were not swept across the whole set.
- The requirements checkpoint wording of the user's original "Limits" decision, so I cannot say the row at `traceability.md:996` equals the user's words (same limitation as the earlier review).

## 9. Integrity

- Main worktree before writing this report: `HEAD` `439b49dd73ab28c4a9d1e627e305b385c2040bec`, `git status --porcelain` empty.
- Main worktree after: `HEAD` unchanged. The only change in status is this untracked report file, `docs/reviews/2026-11-18-review-doc-66ef6ec.md`. No commit, push, reset, branch switch, scratch worktree or target directory was made.
