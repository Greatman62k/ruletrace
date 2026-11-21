# Security review: LT-6 A++ prototype (uncommitted), baseline app-0dfa5d5

## 1. Verdict

NEEDS_CHANGES. One blocking finding: a surviving mutant on a rule you named
(cookie name prefix match). The production code is correct for that rule; the
test does not prove it. No production defect found.

## 2. Scope

Main worktree, branch `m3` (read from `.git/HEAD`), HEAD `0dfa5d5`.
Uncommitted changes in `spikes/lt6-basic-auth/`: `src/lab.rs`, `src/http.rs`,
`src/main.rs`, `src/recorder.rs`, `tests/server.rs`, `README.md`,
`BROWSER-PROTOCOL.md`. Also read `src/util.rs` (unchanged) and
`tools/decisions/UD-48.md` sections 2 to 4. Work was done on a copy in
`~/review-app/` with `CARGO_TARGET_DIR=~/review-target-app`.

## 3. Blocking findings

B1. `tests/server.rs` (`right_path_and_credentials_without_a_matching_cookie_are_403`,
about lines 480-530) has the cookie names `other`, `rt_lab_1` and
`x<name>`, but not `<name>x`. Mutant M02 (`lab.rs:195`, `n == name` replaced by
`n.starts_with(&name)`) SURVIVED all 39 integration tests plus 8 unit tests.
Live probe on the real binary shows the production code is right
(`rt_lab_18555x=<id>` gives 403), so the gap is in the test only. Fix: add the
case `Cookie: <name>x=<identifier>` (and `<name>` followed by `_`), then show
M02 killed.

## 4. Non-blocking findings

N1. Known deviation (Origin on POST only), your question 2. Acceptable for the
prototype, provided it is recorded as owed. Reasons: the browser protocol
exercises only a cross-origin POST (step 6). A cross-origin GET navigation
carries no Origin header in Firefox or Chrome, and a fetch GET from another
port is `same-site` and refused by the `Sec-Fetch-Site` rule, which applies to
every method here. In a browser without `Sec-Fetch-Site` the unguessable path
and the cookie remain. Live probe: `GET <session path>` with
`Origin: http://evil.example` and valid credentials and cookie gives 200. It
weakens the claim "Origin removed" for UD-48 section 4 item 1: mutant M06
shows only the POST case is pinned. The record is weak: `README.md` says
"owed by ruletrace-lab (task 13.4) ... The Origin check applies to POST only,
as in option A." It does not say that the full Requirement 37.4 behaviour (any
request with a wrong Origin; state-changing without Origin needs
`Sec-Fetch-Site: same-origin`) is owed. Make that explicit in `README.md` and in
the 13.4 task, and state in the LT-6 evidence that Origin was shown for POST
only.

N2. Mutant M01 (non-atomic start, `lab.rs` `start_or_refuse`, check and set under
two separate lock acquisitions, no delay) SURVIVED 8 rounds of 16 threads.
With a 1 ms window (M01b) the concurrency test kills it. The current code holds
one `MutexGuard` from check to set (`lab.rs:249-259`), so it is atomic, and
`decide` takes only one snapshot, which cannot change when no session end
exists. The test is probabilistic and detects a wide window only. Suggest more
rounds, or keep it and state this limit in the evidence. Not blocking because
the code is correct and the race test is a recognised weak point of such tests.

N3. `%252e` (double encoding) is not refused (`lab.rs:119-126`); live probe gives
404 after credentials and cookie. The server never decodes the target, so it is
inert here. UD-48 C5 names only `.`, `/`, `\` once-encoded, so this follows the
spec. No test pins the behaviour. Same for `;` path parameters, `//` inside the
session path (404), absolute-form `http://host/...` and `*` (403 session text;
403 open-text before a session), and a dot-segment only in the query (404, not
400).

N4. `Sec-Fetch-Site` is compared case-insensitively and trimmed, which is
stricter than the spec and tested (`Same-Site`, `CROSS-SITE`). A comma list
(`cross-site, same-origin`, `cross-site;x`) is not refused (live: 200). Browsers
send a single token, so there is no practical effect; mention in the 13.4 notes.

N5. `tests/server.rs`: no test shows that the recorder's HTML never contains a
received Authorization or Cookie value. The code is correct (`recorder.rs:30-40`,
the page holds only the lab port and the validated probe token). The tests check
the log, not the page body.

N6. `BROWSER-PROTOCOL.md`: step 5 asks to record the probe log block "with the
cookie value redacted", but that block's request line `GET /s/<token>/probe`
holds the token, and the closing paragraph is the only place that says to
replace the token. Step 4 asks for the "full log block", which would contain the
Authorization value (the secret in base64) if the browser did send it; no
instruction says to redact it. The command in step 5 puts the token into shell
history and the process list. Add: redact the token in the request line, the
Authorization value, and clear the history entry; keep Terminal B output out of
anything shared with the agent.

N7. `README.md`/protocol claim `-> 303 Started` for `GET /` in step 2; the first
request is 401 (no credentials) before it. The expected-lines text covers only
the 303, so the user may think the 401 line is unexpected. Minor.

N8. Constant-time claims. `ct_eq` (`util.rs:30`) has no data-dependent exit
except as length is folded in the result. `authorized`, `in_session_path` and
`binding` use non-short-circuit `&` and `|=`. `csrf_ok` uses `any`, which stops
at the first match only (acceptable: after a match). Timing is not measured and
no test can show it; no mutant exists that the tests could kill.

N9. Log (`lab.rs:373-405`) writes the target with token and identifier
replaced, not the raw target. UD-48 section 3 item 7 says "never echo a raw
request target (C7)"; read together with "path with the token replaced", this is
met for the token and the identifier. The secret and the CSRF value are not
redacted in a target (`GET /?<secret>`); only a sender who knows them can put
them there.

## 5. What held up

1. Decision order in `lab.rs:262-326` is Host, target syntax (400), Origin (POST)
   and Sec-Fetch-Site, credentials (401), session start or Session_Path and
   cookie (403), CSRF. Verified by reading and by test
   `checks_run_in_the_order_...` (killed M04, M05, M06, M13 when each check was
   removed).
2. Atomic start: lock taken at `lab.rs:249`, check `is_some`, set, drop, in one
   guard. No path reads the session between the path and cookie decision and the
   start, because a session is never removed or replaced (no end in the prototype),
   so a `Some` snapshot is final and a `None` snapshot is rechecked under the lock.
3. No 200 without credentials, path and cookie: `decide` returns 401 before the
   session logic and 403 unless `path_ok && cookie_ok`. Mutants M10 (cookie
   removed) and M11 (path removed) killed by five tests each. M14 (user name
   not checked) and M15 (first of several Authorization headers accepted) killed.
4. Token and identifier not in any refusal head or body: refusals use fixed
   bodies; `Location` is only in the 303. Mutant M03 (token appended to a 403
   body) killed by seven tests, including `no_refusal_contains_the_token_or_the_identifier`.
   Log: M12 (redaction removed) killed by the log test.
5. Cookie parsing (live probe, real binary): multiple Cookie lines (200), extra
   whitespace around pairs (200), quoted value (403), `<name>x` (403), `x<name>`
   (403), upper-case name (403), `<name> =<id>` (403), junk value plus valid value
   (200). Empty value handled; with no session the id compares against `""` but is
   masked by `session.is_some()`.
6. Target syntax: mixed-case `%2E`, `%2f`, `%5c`, backslash and dot-segments
   refused with 400 before credentials. Mutant M05 killed by four tests and M19
   (case-sensitive match) killed.
7. Session start: exactly `GET /`; `/?`, `/#x`, `//`, HEAD, POST, `Sec-Purpose`,
   `Purpose` and a wrong Host all refused (tests and mutants M08 and M09 killed).
   `Set-Cookie` attributes pinned exactly (M17 killed), relative `Location`
   pinned (M16 killed).
8. Recorder (`recorder.rs:23-27, 81-90`): `--probe-token` validated as 32
   lowercase hex before binding or opening the log, so no quote or tag reaches the
   HTML; M20 killed. The probe `<img>` uses a relative URL, so it goes to the
   recorder's own origin. The HTML never includes request data.
9. Security headers (`Referrer-Policy`, `X-Frame-Options`,
   `X-Content-Type-Options`, `Cross-Origin-Resource-Policy`) on every status,
   including the malformed-request response (test passes).
10. Expected values in `tests/server.rs` are literals (Base64 credentials,
    fixed 403 texts, exact `Set-Cookie` attributes), not taken from the
    server constants. Token and identifier are random, so tests parse them from
    the 303 and assert shape (32 hex, distinct).
11. BROWSER-PROTOCOL.md steps 4 to 9 map to UD-48 section 4 item 2: recorder at
    `/` and `/other` (step 4); control at `/s/<token>/probe` (step 5); cross-origin
    POST refused (step 6); `/` in a new tab (step 8); second profile (step 9). The
    token-handling paragraph keeps the token on the user's machine and bars
    pasting it to an agent. The results table is blank and its guidance says to
    write `<token>`.

## 6. Commands run

All cargo commands inside `unshare -rn sh -c 'ip link set lo up && exec "$@"' sh`
with `--manifest-path ~/review-app/lt6-basic-auth/Cargo.toml --locked --offline`
(except fmt, which has no `--locked`).

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD`, `git status --porcelain` (before and after) | 0 |
| `cargo fmt --check` | 0 |
| `cargo clippy --all-targets -- -D warnings` | 0 |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | 0 |
| `cargo test -- --test-threads=1` (8 unit, 39 integration passed) | 0 |
| `cargo test` (8 unit, 39 integration passed) | 0 |
| `cargo test --no-fail-fast` per mutant (see 7) | n/a |
| `python3 ~/review-app/h/probe.py` (live binary, loopback) | 0 |

The first attempt to read the branch with `git branch` was blocked by a
permission rule; the branch was read from `.git/HEAD` instead.
Test run notes: windows target was only checked, not run.

## 7. Mutants run

Each mutant applied to the copy in `~/review-app`, tested, and restored with a
byte comparison against `~/review-app/orig-copy` (all restores identical).
Mutants M01..M14, M16..M19 ran without `--no-fail-fast`, so a failing lib unit
test would hide integration results; none of these failed in lib, so their
integration results stand. M15 and M20 were rerun with `--no-fail-fast`.

| Mutant | Result |
| --- | --- |
| M01 non-atomic start, no delay | SURVIVED (N2) |
| M01b non-atomic start, 1 ms window | killed by `concurrent_starts_let_exactly_one_request_win` |
| M02 cookie name `starts_with` | SURVIVED (B1) |
| M02b cookie name `ends_with` | killed by `right_path_and_credentials_without_a_matching_cookie_are_403` |
| M03 token in a 403 body | killed (7 tests) |
| M04 Sec-Fetch-Site check removed | killed (3 tests) |
| M05 target syntax check removed | killed (4 tests) |
| M06 Origin check removed | killed (3 tests) |
| M07 CSRF check removed | killed by `post_with_wrong_or_missing_csrf_is_403` |
| M08 start on any path | killed (2 tests) |
| M09 prefetch exclusion removed | killed by `prefetch_and_prerender_requests_do_not_start_the_session` |
| M10 cookie check removed | killed (5 tests) |
| M11 path check removed | killed (5 tests) |
| M12 token redaction in the log removed | killed by the log test |
| M13 Host check removed | killed (3 tests) |
| M14 user name not checked | killed by `wrong_credentials_are_401_before_and_after_session` |
| M15 first of repeated headers accepted | killed (unit test, `two_authorization_headers_one_valid_is_401`, `wrong_host_is_403_before_and_after_session`) |
| M16 absolute `Location` | killed (26 tests) |
| M17 `SameSite=Lax` | killed by `start_is_303_with_exact_location_and_set_cookie` |
| M18 trailing slash not required | killed by `wrong_or_malformed_session_path_is_403_even_with_cookie` |
| M19 case-sensitive `%2e` match | killed by `dot_segments_backslashes_and_encoded_separators_are_400` |
| M20 probe token validation removed | killed by `recorder_rejects_a_probe_token_that_is_not_32_lower_hex` |

## 8. Not verified

- Any browser behaviour; the protocol was only read, not run.
- Timing of the constant-time comparisons (no measurement).
- macOS and Windows runs; the Windows target was only linted.
- Session end and the 30-minute rule (not in the prototype).
- A mutant that makes `ct_eq` exit early (not detectable by the tests).
- The draft amendment of UD-48 section 5 (out of scope here).

## 9. Integrity

Before: HEAD `0dfa5d586f063dc3c7232ec9e6d0c28b5c89f991`, branch `m3`, status:
the seven modified files under `spikes/lt6-basic-auth/` and no others.
After (before writing this report): identical. The only file added in the main
worktree is this report; scratch directories `~/review-app/` and
`~/review-target-app` were removed after the report was written.
