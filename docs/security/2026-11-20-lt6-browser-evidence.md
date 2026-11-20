---
record: LT-6 browser protocol for option A (HTTP Basic authentication, design §11.18.5)
task: 13.9 (UD-39, UD-45)
date: 2026-11-20
status: Browser protocol passed on Firefox and Google Chrome (Linux). LT-6 stays OPEN (wording of the amendment, review acceptance)
---

# LT-6 browser protocol, 2026-11-20

This record holds item 2 of the LT-6 evidence of design §11.18.5: whether a
browser that has logged in to the Laboratory by HTTP Basic authentication sends
the credentials to a server on another port of `127.0.0.1`, and whether a
cross-origin form post to the Laboratory is refused. It was run with the
stand-in Laboratory `spikes/lt6-basic-auth` (commit `ce53296`, release build),
not with `ruletrace-lab`, which does not exist yet. It does not close LT-6.

## 1. Setup

| Item | Value |
| --- | --- |
| Machine | the user's machine, Linux 7.2.15+kali-amd64 |
| Protocol | `spikes/lt6-basic-auth/BROWSER-PROTOCOL.md`, steps 1 to 6 |
| Stand-in Laboratory | `lt6-basic-auth serve --port 18080`, started by the user in their own terminal (the secret was printed there only and is not in this record) |
| Recorder | `lt6-basic-auth listen --port 18081 --lab-port 18080`, started by the agent; it received no credentials |
| Browsers | Firefox 153.5.1esr (`firefox --version`; user agent `rv:153.0`), fresh profile `firefox -no-remote -profile <new empty directory>`; Google Chrome 158.0.8127.36 (`google-chrome --version`; user agent `Chrome/158.0.0.0`), fresh profile `--user-data-dir=<new empty directory> --no-first-run --no-proxy-server` |
| Time | Firefox 16:24 to 16:30 UTC, Google Chrome 16:35 to 16:38 UTC |
| Sessions | a new stand-in start, and so a new secret, for each browser |

The protocol names Chromium. The run used Google Chrome, which is built on
Chromium but is a different product; the result is recorded for Google Chrome.
Safari was not run (no macOS machine).

## 2. Results

| Browser | Step 2: right origin, credentials present, 200 | Step 4: Authorization header received at the recorder | Step 5: cross-origin POST | Step 6: control, credentials sent without a prompt | Pass |
| --- | --- | --- | --- | --- | --- |
| Firefox 153.5.1esr | yes (16:26:28 GET / 200; 16:26:48 POST /submit 200) | none in 4 requests (`/`, `/favicon.ico`, `/other`, `/`) | 403 `BadOrigin`, Origin `http://127.0.0.1:18081`, `authorization=present`, no prompt by the user's report (16:28:45) | yes (16:29:53 GET / 200, `authorization=present`; no prompt by the user's report) | yes |
| Google Chrome 158.0.8127.36 | yes (16:36:16 GET / 200; 16:36:49 POST /submit 200) | none in 3 requests (`/`, `/favicon.ico`, `/other`) | 403 `BadOrigin`, Origin `http://127.0.0.1:18081`, `authorization=present`, no prompt by the user's report (16:37:49) | yes (16:38:28 GET / 200, `authorization=present`; no prompt by the user's report) | yes |

Pass criteria of the protocol: no `Authorization` header in any recorder
request; the cross-origin POST refused with 403 and not processed; the control
shows the credentials kept. All three hold for both browsers. The user reported
that the page texts and prompts were as expected ("all as you said"); the
prompt observations rest on that report, the raw files do not show them.

Both browsers attached the Laboratory's credentials to the cross-origin POST
(`authorization=present` at 16:28:45 and 16:37:49). Design §11.18.5 expects
this ("the browser also attaches these credentials to cross-site requests to
the Laboratory's origin"), which is why the Origin and CSRF checks stay; the
Origin check refused both. The CSRF refusal is shown by the server-side tests
only, not by this browser run.

## 3. Raw evidence

Committed under `docs/security/lt6-browser-2026-11-20/`:

| File | Content | SHA-256 prefix |
| --- | --- | --- |
| `recorder-firefox.log` | every request line and header the recorder received during the Firefox run | `e0597b9c4cf55701` |
| `recorder-chrome.log` | the same for the Google Chrome run | `cd3ba3fe5df02efb` |
| `lab-firefox.txt` | the stand-in Laboratory's request lines for the Firefox run, as pasted by the user without the secret line | `d55aa7d93b119509` |
| `lab-chrome.txt` | the same for the Google Chrome run | `0f54d1441bfbdb27` |

`grep -ci authorization` on both recorder logs gives 0.

## 4. Limits

- One run per browser, one machine, Linux only; current browser versions
  only. Safari, Windows browsers, private windows and extensions are not
  covered.
- The stand-in is a prototype; `ruletrace-lab` must implement and test the same
  checks (13.4, 13.6).
- A recorder that answers 401 with a Basic challenge makes the browser prompt;
  a user who types the secret there gives it away. That is phishing of the
  secret, not leakage by the browser, and is not tested (protocol, "What this
  protocol does not show").

## 5. Still open for LT-6

1. The user's approval of the amendment wording (UD-45 section 4), its
   application with Old/New records, and the update of the design and
   threat-model places it lists.
2. An independent reviewer's acceptance of the LT-6 mitigation in the threat
   model.
