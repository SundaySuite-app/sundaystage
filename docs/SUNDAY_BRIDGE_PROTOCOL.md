# Sunday Bridge Protocol — v1.0.0

The local bridge that lets **SundayStage** and **SundayRec** (and, later, other
Sunday-suite apps) discover each other on the same machine and share data. This
document is the authoritative contract; the SundayStage side lives in
`src-tauri/src/services/sundayrec_bridge/`, and the SundayRec side implements
against this doc.

> **Status.** The message types (`protocol.rs`) and the export transforms
> (`export.rs`) are implemented and unit-tested. The transport, discovery, and
> pairing described below are the agreed design — not yet wired — because they
> require a live network and the peer app, which can't be exercised in CI.

## Versioning

`PROTOCOL_VERSION = "1.0.0"`. Exchanged in `ping`/`pong`.

The **only** forward-compatible change is _adding_ a new verb. Existing verbs,
their fields, and their JSON shapes are frozen. A peer that receives an unknown
verb must reply `{"result":"error","message":"unknown verb"}` and keep the
connection alive. Bump the **minor** version when adding verbs; never the major
unless a breaking change is unavoidable (it should not be).

## Transport (planned)

- **HTTP + JSON over loopback** (`127.0.0.1`). Rationale: trivially debuggable,
  no platform-specific named-pipe/Unix-socket branching, and Tauri already
  ships an HTTP stack. A Unix-socket/named-pipe transport can be added later
  behind the same message types if loopback HTTP proves insufficient.
- Each app binds a well-known localhost port (with fallback scan) and announces
  via mDNS/Bonjour `_sundaybridge._tcp`. Discovery is best-effort; a manual
  "connect to localhost:PORT" fallback always exists.

## Pairing

First contact requires **explicit confirmation in BOTH app windows** — the
initiating app shows "Pair with SundayRec?", the peer shows "Allow SundayStage
to connect?". Never silent pairing. A paired token is stored per-app and reused;
either side can revoke.

## Verbs (v1.0.0)

Requests are tagged by `verb`, responses by `result` (snake_case).

| Verb                | Direction   | Payload                       | Response                                |
| ------------------- | ----------- | ----------------------------- | --------------------------------------- |
| `ping`              | either      | —                             | `pong { app, version, capabilities[] }` |
| `recording_started` | Rec → Stage | `recording_id, started_at`    | `ok`                                    |
| `recording_stopped` | Rec → Stage | `recording_id, stopped_at`    | `ok`                                    |
| `cue_advanced`      | Stage → Rec | `offset_ms, title, cue_index` | `ok`                                    |
| `get_recordings`    | Stage → Rec | —                             | `recordings { recordings[] }`           |
| `get_transcript`    | Stage → Rec | `recording_id`                | `transcript { recording_id, text }`     |
| `get_song_history`  | Stage → Rec | —                             | `song_history { songs[] }`              |

`capabilities` SundayStage advertises: `cue_advanced`, `export_srt`,
`chapter_markers`.

### Example

```json
// Stage → Rec, on every cue advance during a live service
{ "verb": "cue_advanced", "offset_ms": 12500, "title": "Amazing Grace — Verse 1", "cue_index": 3 }
// Rec → Stage
{ "result": "ok" }
```

## The two marquee transforms (implemented, Phase 10.2)

These derive purely from SundayStage's live session log — the operator does
nothing extra.

1. **Cue → chapter markers** (`export::chapter_markers`). A new chapter starts
   when the cue's _service item_ changes (song → song → sermon …); slides
   within one song do **not** each become a chapter, and a blackout does not
   split a chapter. Output: `[{ offset_ms, title }]`, streamed live via
   `cue_advanced` and/or handed over post-service.

2. **Lyrics → SRT** (`export::session_to_srt`). Each `Normal` slide cue becomes
   a caption spanning its on-screen window; blackout/logo stretches are gaps;
   adjacent identical text is coalesced. The timeline is anchored at the
   session start so it lines up with the recording.

## Deferred (documented, not built)

- The loopback HTTP server/client, mDNS discovery, and pairing UI.
- **TONO streaming-licence audit** (Phase 10.2 feature 3): tagging each
  copyrighted song-cue with `was_streamed` (queried from SundayRec's streaming
  state), the pre-service advisory when the church's TONO streaming add-on is
  missing (shown _before_ the service, never during), and forwarding tagged
  usage events to SundaySong's `usage_log`. This needs the cue→`song_id` link
  surfaced onto cues plus the SundaySong account connection.
