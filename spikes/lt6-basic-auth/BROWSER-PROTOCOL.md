# LT-6 browser protocol (manual)

Purpose: show, on each browser, that HTTP Basic credentials given to the stand-in
Laboratory on one port of `127.0.0.1` are not sent to a server on another port of
`127.0.0.1`, and that a cross-origin form post to the stand-in Laboratory is
refused (design §11.18.5, evidence item 2). The tool has no browser automation, so
you run this by hand and record what you see. Results go in the table at the end.
Do not guess or fill in a browser you did not run.

Platforms: the secret generator reads `/dev/urandom`, so the browser protocol runs
on Linux and macOS only (Firefox, Chromium, Safari). It is not run on Windows.

## What each program does

- `serve` is the stand-in Laboratory. It prints the launch secret once to standard
  error. Every request is checked: Host, then (POST) Origin, then Basic credentials,
  then (POST) the CSRF token. It sets no cookie. It logs one line per request to
  standard error: `authorization=present|absent`, `origin=...` and the outcome.
- `listen` is the raw recorder. It serves one page with a cross-origin form and
  logs every request line and header verbatim, with a UTC timestamp, to standard
  output and to a file.

## Preparation

1. Linux or macOS, with no other server on ports 18080 and 18081 (use others if
   busy, and substitute them everywhere below).
2. Build (standard library only, no downloads):
   `cargo build --release --manifest-path spikes/lt6-basic-auth/Cargo.toml`
   The binary is `spikes/lt6-basic-auth/target/release/lt6-basic-auth`; with a
   different `CARGO_TARGET_DIR` it is under that directory. Call it `lt6` below.
3. Use one browser at a time. Start it with a profile in which you have not saved
   credentials for `127.0.0.1`. Do not let a password manager save the password.
   Use one window for steps 2 to 6 (same browser session throughout).
   Do not restart the browser between steps 2 and 6: a restart can drop the
   cached credentials, and step 4 would then show no header for the wrong reason.
   Make sure no proxy is used for `127.0.0.1` (browser network settings: no proxy,
   or `127.0.0.1` in the bypass list); a proxy would carry the requests instead.
4. Record the browser name, exact version, OS and its version, and the date.
   Firefox: menu, Help, About Firefox. Chromium: `chrome://version`.
   Safari: Safari menu, About Safari. Terminal alternatives: `firefox --version`,
   `chromium --version`, `sw_vers`, `uname -a`.

## Steps (repeat the whole list for each browser)

Use a fresh recorder log file per browser, for example `recorder-firefox.log`.

1. Terminal A, start the stand-in Laboratory:
   `lt6 serve --port 18080`
   It prints the URL and the secret once. Keep the terminal visible; it logs each request.
2. In the browser open `http://127.0.0.1:18080/`. The browser shows its own
   credential prompt. User name `lab`, password the printed secret. Do not tick
   "remember". The page "Stand-in Laboratory" appears. Click Submit once. The
   answer is the text `accepted`. Go back to the form page.
   Record the Terminal A lines for this step (expect `authorization=present` and
   `-> 200 Ok` for the submit).
3. Terminal B, start the recorder on another port of the same host:
   `lt6 listen --port 18081 --lab-port 18080 --log recorder-<browser>.log`
4. In the same browser window (a new tab is fine) open
   `http://127.0.0.1:18081/`. Also open `http://127.0.0.1:18081/other` once
   (a 404 page is expected). The recorder prints and writes every request.
   Question: did any request in the recorder output contain a header named
   `Authorization`? Search the log: `grep -i authorization recorder-<browser>.log`
   (no output means none). First check that the log is not empty: the request
   for `/` must be there, otherwise the visit did not reach the recorder and the
   step must be repeated. Record the full log block of the first request.
5. On the recorder page click "POST to the Laboratory". The browser navigates to
   the stand-in Laboratory. Record: the page text and, if you can read it, the
   status (browser developer tools, Network tab, or the line in Terminal A).
   Expected: status 403, text `forbidden: origin`. Record the Terminal A line for
   this request, which says whether the browser attached an Authorization header
   (`authorization=present` or `absent`) and the Origin it sent. Also record
   whether the browser showed a credential prompt (expected: none).
6. Control for step 4. Open `http://127.0.0.1:18080/` again in the same window.
   Expected: no prompt, and the Terminal A line shows `authorization=present`.
   This shows the browser keeps the credentials and sends them to the origin they
   belong to, so a missing header in step 4 is a real result.
7. Stop both programs (Ctrl-C). Close the browser before testing the next one.

## What to record per browser

- Browser name, exact version, OS and version, date (UTC), the two ports used.
- Step 2 and step 6 Terminal A lines (credentials sent to the right origin).
- Step 4: the recorder log lines, verbatim, and the answer "Authorization header
  received: yes or no".
- Step 5: response status, response text, the Terminal A line, prompt shown yes/no.
- Anything unexpected (extra prompts, redirects, console warnings).

## Pass criteria for option A

All of these hold for the browser:

1. Step 4: no `Authorization` header in any recorder request.
2. Step 5: the response is 403 and the stand-in Laboratory did not process the post.
3. Step 6: `authorization=present` with no prompt (the control).

If criterion 1 fails on a supported browser, LT-6 option A fails there and design
§11.18.5 says option B is designed next. Report it as it is.

## Results table template

| Browser | Version | OS | Date (UTC) | Step 2 (right origin: 200, auth present) | Step 4: Authorization received at recorder | Step 5 status | Step 5 Terminal A line | Step 6 control | Pass |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Firefox | | | | | | | | | |
| Chromium | | | | | | | | | |
| Safari (macOS only) | | | | | | | | | |

Paste the recorder log block of step 4 under the table for each browser.

The filled-in results are committed as a dated record under `docs/security/`
(design §11.18.5, task 13.9), not in this file. A browser run reaches the Origin
refusal of step 5; the CSRF refusal is shown by the server-side tests only.

## What this protocol does not show

- A recorder that answers 401 with `WWW-Authenticate: Basic` makes the browser show
  a prompt. A user who types the secret there gives it away; that is phishing of
  the secret, not leakage by the browser, and is not tested here.
- Other browsers, Windows, private windows and extensions are not covered unless
  you run them and record them.
