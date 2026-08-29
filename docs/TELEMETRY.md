# Telemetry in SundayStage — the honest description

This is the technical companion to [PRIVACY.md](../PRIVACY.md). Where that
document says what is collected, this one says **how**, and — more usefully —
what makes each promise hard to break by accident. The repository is public, so
every claim below can be checked against the code it names.

## The four promises, and where they live in the source

**1. The live path is sacrosanct.** Nothing on the cue path may take a lock, a
file handle or an allocation that could stall a Sunday morning. Collection on
that path is atomic increments and bounded, non-blocking sends
(`src-tauri/src/telemetry/quality.rs`). The collector only writes to SQLite when
`AppState.live` is `None`, and a contended lock counts as live. The send pump
checks the same gate at the top of every beat before it reads anything at all
(`sender.rs::beat`). Proven by a test that injects a sender which PANICS if it
is ever called, with a positive control on the same data so the assertion cannot
pass vacuously.

**2. Content never leaves the machine.** Two independent defences plus a third
at the far end:

- the schema has nowhere to put content — every telemetry column is a number, a
  boolean or a value from a closed enum (`sql/0007_telemetry.sql`);
- all free text (crash messages, log lines, the text you type into a problem
  report) goes through `scrub.rs` **before it reaches disk** and again when it
  is read back, replacing absolute paths with `<path>` and user names with
  `<user>`;
- the receiving Worker screens every string field with its own absolute-path
  regex and refuses the payload otherwise.

A test parses every `tracing::` call site in the tree and fails on any that
interpolates an error value or a message field, because a `serde_json` error
quotes the input it choked on — which for this app is a lyric. That audit found
three real leaks when it was written.

**3. Off is off.** Consent is a three-state record (`consent.rs`): `NeverAsked`,
`Granted`, `Denied`. Absence means "never asked", never "no", and a missing,
malformed or hand-edited record evaluates to not-granted. The install ID is
minted lazily and only on the `granted` branch — an install that declines never
has one. Revoking purges the outbox, the accumulated counters and any unsent
report written under the standing consent.

**4. Nothing is lost silently.** A payload the endpoint refuses permanently is
dropped rather than retried six times — but it is dropped with a log line naming
the reason. When the 64 kB body cap forces a trim, the "already reported"
watermarks are trimmed in LOCKSTEP with the payload, so a deferred record stays
owed. Crash records are the one lossy trim, they go last, and they are logged
when they go.

## The crash we cannot report, and what we send instead

A panic unwinds, so the panic hook runs and writes a record. A **hard crash** —
segfault, abort, out of memory — does not: the process is gone before any Rust
executes. That is the crash a volunteer experiences as "SundayStage just
disappeared", and until A6 it was the one crash telemetry could not see at all.

The industry answer is a minidump, and it is the one answer this app may never
give. **A minidump IS process memory, and this process's memory holds the verse
that was on the congregation's screen.** There is no scrubbing pass that makes
it safe, because there is no structure to scrub. So SundayStage writes none —
not on any platform, not even locally, not even unsent. A file that cannot be
uploaded is still a file that can be attached to an email.

What is captured instead is a **signal**: five values, every one of them either
an integer or a word from a compile-time list.

| Field    | Values                                                                                       |
| -------- | -------------------------------------------------------------------------------------------- |
| `sig`    | `segv` `bus` `ill` `fpe` `abort` `trap` `stack-overflow` `other`                             |
| `code`   | the platform's own numeric fault code                                                        |
| `fault`  | `null` `low` `nonnull` `unknown` — a classification of the faulting address, never its value |
| `site`   | `app+0x<offset>` or `foreign` or `unknown`                                                   |
| `thread` | `main-thread` `other-thread` `unknown-thread`                                                |

`site` is where the module name would ordinarily go, and it deliberately does
not have one: a module basename is a filename, and a filename is the shape law 2
is about. Instead the program counter is compared against SundayStage's own
executable image, resolved once at startup. Inside it, the offset is reported and
is symbolisable against the matching release binary — which is what lets two
machines' crashes be recognised as the same crash. Outside it, the answer is the
literal word `foreign`: we learn it was not our code, and nothing about whose it
was.

The record reaches the wire as an ordinary entry in `crashes[]` with
`kind: other` — no new endpoint, no new transport, and no schema change, because
the deployed Worker's `STAGE_CRASH_KINDS` is a closed set and inventing a value
would earn a 400 that the outbox drops permanently. An aggregate groups on the
`native crash:` prefix in the message.

**Two disciplines make it safe rather than merely well-intentioned.**

_The handler does almost nothing._ It runs in a compromised context — an
async-signal-safe world on Unix, a Mach exception thread on macOS, an
unhandled-exception filter on Windows — so it allocates nothing, takes no lock of
ours, cannot panic, cannot loop, runs at most once, and performs exactly one
bounded `write(2)` of ~150 bytes to a file descriptor opened at startup. If that
file cannot be opened, no handler is installed at all: a handler with nowhere to
write is risk without benefit. All the real work — parsing, scrubbing, writing
the ring entry — happens on the NEXT launch, in ordinary code. A test reads the
source between `HANDLER-SAFE` markers in all four files and fails on `format!`,
`String`, `.lock()`, `.unwrap()`, `tracing::` and their neighbours, because none
of those failures are visible when the same functions are called from a healthy
thread.

_The crash is handed back._ The callback always returns `Handled(false)`, which
restores the previous handler and lets the fault re-trigger. So the OS still
writes its own crash report, Rust still prints its stack-overflow message, and
the process still exits with "killed by SIGSEGV" — everything an outside watcher
relies on. Two tests fault a real child process for real and assert _both_
halves: that the signal was captured, and that the child still died of it.

Capture is local and therefore not consent-gated — same reasoning as the panic
ring. Sending is: an adopted record is an ordinary ring entry by the time the
drain can see it, and the crash watermark set when consent is granted means a
hard crash captured before the operator answered stays on the machine forever.
There is nevertheless an off switch (Settings → Advanced → Privacy → "Capture
hard crashes", default on), because a signal handler is the one part of this
system that changes how the process behaves while it is dying, and an operator
whose rig it upsets should not have to wait for a release.

**Not covered, and no pretence otherwise:** a kernel OOM kill (Linux `SIGKILL`,
macOS jetsam) cannot be caught by any handler, minidump or not — the process is
simply removed. Rust's own allocation-failure path calls `abort()`, so _that_
kind of "out of memory" is captured. Crashes inside the webview are also out of
scope on every platform: WebKit and WebView2 render page content in separate
processes, which is why they do not take SundayStage down in the first place.
The output child is out of scope on purpose too — its death is a quality signal
(`outputChildRestarts`), not a crash, because the projector kept its last frame
throughout.

## The shape of the client

```
counters / quality / crash ring     E3 — local only, always on
        ↓ (live gate)
      SQLite
        ↓ (consent gate)
   payload builder ─────────────► preview (the SAME builder, byte-identical)
        ↓
      outbox (50 rows, 6 attempts, 1 min → 24 h ladder)
        ↓ (consent gate + live gate, re-checked every beat)
   HTTPS POST /v1/apps/sundaystage/ingest
```

One builder serves the wire, the "show what is sent" preview and the manual
report, so the preview cannot describe code that never runs — a test pins the
two byte-for-byte.

The endpoint and its write key are **compile-time** constants
(`option_env!`, `config.rs`). A build without them constructs no sender at all;
telemetry queues locally and goes nowhere. That is deliberate: a runtime setting
would let a shipped app be pointed at any host by anything that could write to
its configuration.

## The three consent stories

| Path                                          | Gate                                                      | Identity                               |
| --------------------------------------------- | --------------------------------------------------------- | -------------------------------------- |
| Ordinary reports (crashes, quality, counters) | standing consent, active                                  | the durable install ID                 |
| "Delete my data"                              | **none** — a deletion is a withdrawal, not a contribution | the retired ID, in a URL, nothing else |
| Manual problem report                         | **the send button itself**                                | a one-shot random UUID, stored nowhere |

The second row is a lesson learned in SundayRec: gating deletion on consent made
it unreachable in exactly the case it existed for (revoke, restart, press
delete). The third is the owner's decision for SundayStage: someone who declined
standing collection can still tell us something broke, and doing so must not
create a durable identity or attach the report to one.

An ephemeral report carries the report and an envelope thin enough to act on it
(app version, OS, architecture, UI language) — no counters, no crashes, no
quality rows, and the settings block at its defaults. A fresh one-shot ID is
generated per SEND, so a retry after a failure uses a different one.

## What "your report is waiting" means

A manual report is marked sent only when it was actually included in a payload —
at enqueue for the standing-consent path, at delivery for the one-shot path. If
the byte cap defers it, it stays owed and the privacy card says so on its own
line. The byte trim must never be the place an operator's hand-written words
disappear.

## Turning it off, and getting rid of it

Settings → Advanced → Privacy:

- **the switch** — one click on, one click off, no confirmation in either
  direction;
- **"Capture hard crashes"** — a SEPARATE switch, because it governs local
  capture rather than sending. It is on by default and turning it off drops the
  signal handler immediately, without a relaunch;
- **"Show what is sent"** — the real builder's bytes, pretty-printed;
- **the queue** — how many payloads wait, how old the oldest is, and separately
  whether a written report is still owed;
- **"Delete my data"** — works regardless of the switch;
- **the install ID**, in full, with a regenerate button that retires it.

## Retention

Raw events: 90 days, then deleted by a scheduled purge. ID-less aggregates are
kept longer. Storage is in the EU (Cloudflare D1, Western Europe).

## Where the code is

| Concern                                  | File                                      |
| ---------------------------------------- | ----------------------------------------- |
| Consent state machine                    | `src-tauri/src/telemetry/consent.rs`      |
| Identity, deletion, drain, preview       | `src-tauri/src/telemetry/client.rs`       |
| The payload — the only thing that leaves | `src-tauri/src/telemetry/payload.rs`      |
| Scrubbing                                | `src-tauri/src/telemetry/scrub.rs`        |
| Outbox + backoff                         | `src-tauri/src/telemetry/outbox.rs`       |
| Live gate + pump + one-shot reports      | `src-tauri/src/telemetry/sender.rs`       |
| Hard crashes (signal source, no dumps)   | `src-tauri/src/telemetry/native_crash.rs` |
| The endpoint (separate repository)       | `sunday-telemetry`                        |

The programme this was built under, stage by stage, is
[`docs/TELEMETRY-PROGRAM.md`](TELEMETRY-PROGRAM.md).
