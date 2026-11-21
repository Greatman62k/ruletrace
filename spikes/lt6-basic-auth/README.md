# LT-6 spike: Basic credentials with a session path (UD-48, option A++)

Scratch program, outside the RuleTrace workspace (empty `[workspace]` table in
`Cargo.toml`). Standard library only, `#![forbid(unsafe_code)]`. It
implements UD-48 section 3 and nothing from the amendment draft of section 5.

- `serve [--port P]`: stand-in Laboratory. One launch secret per start from
  `/dev/urandom` (Unix only), printed once to standard error with the user
  name `lab` and the address `http://127.0.0.1:<port>/`.
  - Check order: Host; request-target syntax (400 for a dot-segment, a
    backslash, or a percent-encoded `.`, `/` or `\`, hex digits in either
    case); Origin (POST) and `Sec-Fetch-Site` (`same-site` and `cross-site`
    refused); Basic credentials (401); Session_Path and binding cookie (403);
    CSRF token (POST, 403).
  - The first `GET /` (exact target, no query) with valid credentials, no
    `Sec-Purpose` or `Purpose` header and no session starts the session in one
    compare-and-set under a mutex: 303 to `/s/<token>/` and
    `Set-Cookie: rt_lab_<port>=<identifier>; HttpOnly; SameSite=Strict;
    Path=/s/<token>/`. Token and identifier are 16 random bytes each, hex.
  - Served under the session path: the page (`GET /s/<token>/`) and the form
    post (`POST /s/<token>/submit`), with relative URLs only.
  - Every other request with valid credentials gets a fixed 403 text. Neither
    the token nor the identifier is in any 400, 401 or 403 response.
  - The request log (standard error) prints the path with the token replaced
    by `<session>`, and the fields `authorization`, `cookie`, `session_path`
    and `binding_cookie`.
  - Not implemented here, owed by `ruletrace-lab` (task 13.4): the 30-minute
    inactivity rule and any session end. A session lasts until the process
    stops. The Origin check applies to POST only, as in option A; the full
    check of Requirement 37 criterion 4 (any request with a wrong Origin is
    refused; a state-changing request without Origin needs `Sec-Fetch-Site:
    same-origin`) is owed by `ruletrace-lab` (13.4). A cross-origin GET in this
    prototype is stopped by the `Sec-Fetch-Site` rule, or by not knowing the
    Session_Path; a GET with a wrong Origin, valid credentials, the path and the
    cookie is served here.
- `listen --port P --lab-port L [--log FILE] [--probe-token T]`: raw recorder
  serving a page with a cross-origin POST form; logs every request line and
  header verbatim. With `--probe-token T` (32 lowercase hex characters) the
  page also loads `/s/T/probe` from the recorder's own origin, the control
  that shows the browser sends the binding cookie to a server that knows the
  token. Without the option the page is unchanged.

Run the server-side tests (loopback only):

    unshare -rn sh -c 'ip link set lo up && exec "$@"' sh \
      cargo test --manifest-path spikes/lt6-basic-auth/Cargo.toml --locked

The tests use the operating-system randomness on Unix and a counter on other
platforms. The browser evidence is manual: see `BROWSER-PROTOCOL.md`. The
tests do not show what any browser does; they show only that the server
refuses what it must refuse.
