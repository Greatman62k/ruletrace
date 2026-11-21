# LT-6 browser protocol (manual), option A++

Purpose: show, on each browser, after the first login to the stand-in
Laboratory on one port of `127.0.0.1`: (a) the browser sends neither the
Basic credentials nor the binding cookie to a server on another port at `/`
or `/other`; (b) control: it does send the binding cookie to that server at
`/s/<token>/`, so the secrecy of the token is what protects the cookie; (c) a
cross-origin form post to the Laboratory is refused; (d) the secret alone, in
a second browser profile, and a new tab at `/` get 403 without the token in
the response (UD-48 section 4 item 2). The tool has no browser automation, so
you run this by hand and record what you see. Results go in the table at the
end. Do not guess or fill in a browser you did not run.

Platforms: the secret generator reads `/dev/urandom`, so the browser protocol
runs on Linux and macOS only (Firefox, Chromium, Safari). It is not run on
Windows.

Token handling: the token is the secret that protects the cookie. You may copy
it from the browser address bar into the recorder command in step 5 on your own
machine. Never paste it into a chat, a ticket or any message to an agent, and
do not put it in the results table; write `<token>` there. Records contain the
token only if you leave the recorder log unredacted; redact it before you share
a log.

## What each program does

- `serve` is the stand-in Laboratory. It prints the address, the user name
  `lab` and the launch secret once to standard error. The first `GET /` with
  valid credentials starts the session: 303 to `/s/<token>/` and a cookie
  `rt_lab_<port>` with `Path=/s/<token>/`. Later requests need the credentials,
  the path and the cookie. Its log line per request shows the path with the
  token replaced by `<session>` and the fields `authorization`, `cookie`,
  `session_path=yes|no` and `binding_cookie=match|nomatch|absent`.
- `listen` is the raw recorder. It serves one page with a cross-origin form and
  logs every request line and header verbatim, with a UTC timestamp, to
  standard output and to a file. With `--probe-token <token>` its page also
  requests `/s/<token>/probe` on the recorder's own origin.
- Not covered: the 30-minute rule and session end are not implemented in the
  prototype (owed by `ruletrace-lab`, task 13.4).

## Preparation

1. Linux or macOS, with no other server on ports 18080 and 18081 (use others
   if busy, and substitute them everywhere below).
2. Build (standard library only, no downloads):
   `cargo build --release --manifest-path spikes/lt6-basic-auth/Cargo.toml`
   The binary is `spikes/lt6-basic-auth/target/release/lt6-basic-auth`; with a
   different `CARGO_TARGET_DIR` it is under that directory. Call it `lt6` below.
3. Use one browser at a time. Start it with a profile in which you have not
   saved credentials for `127.0.0.1`. Do not let a password manager save the
   password. Use one window for steps 2 to 7 (same browser session throughout).
   Do not restart the browser during those steps: a restart drops the cookie
   and the session is then unusable until the Laboratory restarts. Make sure
   no proxy is used for `127.0.0.1` (browser network settings: no proxy, or
   `127.0.0.1` in the bypass list); a proxy would carry the requests instead.
4. Record the browser name, exact version, OS and its version, and the date.
   Firefox: menu, Help, About Firefox. Chromium: `chrome://version`.
   Safari: Safari menu, About Safari. Terminal alternatives:
   `firefox --version`, `chromium --version`, `sw_vers`, `uname -a`.

## Steps (repeat the whole list for each browser)

Use a fresh recorder log file per browser, for example `recorder-firefox.log`.

1. Terminal A, start the stand-in Laboratory: `lt6 serve --port 18080`
   It prints the address `http://127.0.0.1:18080/`, the user name and the
   secret once. Keep the terminal visible; it logs each request.
2. In the browser open `http://127.0.0.1:18080/`. The browser shows its own
   credential prompt. User name `lab`, password the printed secret. Do not tick
   "remember". Expect: the address bar changes to
   `http://127.0.0.1:18080/s/<token>/` and the page "Stand-in Laboratory"
   appears. Record that the address bar shows `/s/<token>/` (not the token).
   Click Submit once. The answer is the text `accepted`. Go back to the form
   page. Record the Terminal A lines: expect `GET / ... -> 303 Started` and
   `GET /s/<session>/ ... session_path=yes binding_cookie=match -> 200 Ok`,
   and for the submit `-> 200 Ok`.
3. Terminal B, start the recorder on another port of the same host without
   the probe:
   `lt6 listen --port 18081 --lab-port 18080 --log recorder-<browser>.log`
4. In the same browser window (a new tab is fine) open
   `http://127.0.0.1:18081/` and `http://127.0.0.1:18081/other` once (a 404
   page is expected). Question: did any recorder request contain a header named
   `Authorization` or a `Cookie` header with `rt_lab_18080`? Search the log:
   `grep -i -e authorization -e rt_lab recorder-<browser>.log` (no output
   means none). First check that the log is not empty: the request for `/` must
   be there, otherwise the visit did not reach the recorder and the step must
   be repeated. Record the full log block of the first request; if it ever
   holds an `Authorization` or `rt_lab_` line, replace the value after the
   colon with `<redacted>` before you share it (the step has then failed).
5. Control: the cookie is sent to a server that knows the token. Stop the
   recorder (Ctrl-C). Copy the token from the address bar of the Laboratory tab
   (the part between `/s/` and the last `/`). Start the recorder again with it,
   on your own machine only, using a new log file:
   `lt6 listen --port 18081 --lab-port 18080 --log recorder-<browser>-probe.log --probe-token <token>`
   (the token is then in your shell history: run `history -d` for that line,
   or start the command with a space if your shell ignores such lines, and in
   any case restart the Laboratory after this protocol so the token is void)
   Open `http://127.0.0.1:18081/`. The page loads an image from
   `/s/<token>/probe` on the recorder. Expect: the log of that request has a
   `Cookie: rt_lab_18080=...` header (and no `Authorization` header). Record
   the block with the cookie value redacted and the token in the request line
   replaced by `<token>` (`GET /s/<token>/probe HTTP/1.1`). This shows the cookie is scoped by
   path, not by port, so the token's secrecy is load-bearing.
6. On the recorder page click "POST to the Laboratory". The browser navigates
   to the stand-in Laboratory. Record the page text and, if you can read it,
   the status (developer tools, Network tab, or the line in Terminal A).
   Expected: status 403, text `forbidden: origin`. Record the Terminal A line,
   which says whether the browser attached an Authorization header
   (`authorization=present` or `absent`), and whether the browser showed a
   credential prompt (expected: none).
7. Control for step 4. Open the session address from step 2
   (`http://127.0.0.1:18080/s/<token>/`) again. Expected: no prompt, and the
   Terminal A line shows `authorization=present` and `binding_cookie=match`.
   This shows the browser keeps the credentials and the cookie and sends them
   to the origin and path they belong to, so a missing header in step 4 is a
   real result.
8. New tab, same browser: open `http://127.0.0.1:18080/` (no token). Expected:
   status 403 and the fixed text that says the session runs in the tab where it
   was started. The text and the Terminal A line (`session_path=no`) must not
   show the token.
9. Second browser profile (or another browser) with no cookie: open
   `http://127.0.0.1:18080/`, enter user `lab` and the secret. Expected: 403
   with the same fixed text, no token anywhere in the page, and no `Set-Cookie`
   (check the Network tab). This is the secret-alone case.
10. Stop both programs (Ctrl-C). Close the browser before testing the next
    one.

## What to record per browser

- Browser name, exact version, OS and version, date (UTC), the two ports used.
- Step 2: the address bar form and the Terminal A lines.
- Step 4: the recorder log lines, verbatim, and the answers "Authorization
  header received: yes or no" and "binding cookie received: yes or no".
- Step 5: the probe log block (cookie value redacted) and "binding cookie
  received at `/s/<token>/probe`: yes or no".
- Step 6: response status, response text, the Terminal A line, prompt shown.
- Step 7: the Terminal A line.
- Steps 8 and 9: status, text, token visible yes/no, Set-Cookie seen yes/no.
- Anything unexpected (extra prompts, redirects, console warnings).

## Pass criteria for option A++

All of these hold for the browser:

1. Step 4: no `Authorization` header and no `rt_lab_` cookie in any recorder
   request at `/` and `/other`.
2. Step 5: the cookie is present at `/s/<token>/probe` (the control).
3. Step 6: the response is 403 and the stand-in Laboratory did not process the
   post.
4. Step 7: `authorization=present` and `binding_cookie=match` with no prompt.
5. Steps 8 and 9: 403, the fixed text, no token in the response.

If criterion 1 fails on a supported browser, option A++ fails there. Report it
as it is. If criterion 2 fails, the control did not work and criterion 1 shows
nothing for the cookie on that browser; report that too.

## Results table template

| Browser | Version | OS | Date (UTC) | Step 2: address bar `/s/<token>/` | Step 4: Authorization or cookie at `/` and `/other` | Step 5: cookie at `/s/<token>/probe` | Step 6 status | Step 6 Terminal A line | Step 7 control | Step 8 new tab at `/` | Step 9 second profile | Pass |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Firefox | | | | | | | | | | | | |
| Chromium | | | | | | | | | | | | |
| Safari (macOS only) | | | | | | | | | | | | |

Paste the recorder log blocks of steps 4 and 5 under the table for each
browser, with the token and the cookie value replaced by placeholders.

The filled-in results are committed as a dated record under `docs/security/`
(design §11.18.5, task 13.9), not in this file. A browser run reaches the Origin
refusal of step 6; the CSRF refusal is shown by the server-side tests only.

## What this protocol does not show

- A recorder that answers 401 with `WWW-Authenticate: Basic` makes the browser
  show a prompt. A user who types the secret there gives it away; that is
  phishing of the secret, not leakage by the browser, and is not tested here.
- Three-part attack (UD-48 section 2 residual 3): a page that learns the token,
  a cookie request, and the secret. Step 5 shows the cookie half only.
- Browsers that do not send `Sec-Fetch-Site`, other browsers, Windows, private
  windows and extensions are not covered unless you run them and record them.
