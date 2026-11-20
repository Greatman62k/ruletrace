# LT-6 spike: HTTP Basic authentication (design §11.18.5, option A)

Scratch program, outside the RuleTrace workspace (empty `[workspace]` table in
`Cargo.toml`). Standard library only, `#![forbid(unsafe_code)]`.

- `serve [--port P]`: stand-in Laboratory. One launch secret per start from
  `/dev/urandom` (Unix only), printed once to standard error. No cookie. Every
  request checks Host, then (POST) Origin, then `Authorization: Basic` with a
  compare written to take the same time for every equal-length input (by
  construction; its timing is not measured), then (POST) the CSRF token from the
  served form.
- `listen --port P --lab-port L [--log FILE]`: raw recorder serving a page with a
  cross-origin POST form; logs every request line and header verbatim.

Run the server-side tests (loopback only):

    unshare -rn sh -c 'ip link set lo up && exec "$@"' sh \
      cargo test --manifest-path spikes/lt6-basic-auth/Cargo.toml

The browser evidence is manual: see `BROWSER-PROTOCOL.md`. These tests do not show
what any browser does; they show only that the server refuses what it must refuse.
