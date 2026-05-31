# SundayStage — needs Richard (P2c)

Items that cannot be done in the sandbox: they need a real GUI session, a live
network/backend, a device, an API key, or infra. The supporting **pure logic**
for each is built and unit-tested; only the outside-world edge is parked here.

## 1. AI lyric formatting against a real Anthropic key (Phase 4.2)

- **What's done:** the consent gate, model picker, request seam, validated
  response parser, and an offline heuristic fallback all exist and are wired
  (`PasteFormatModal` → `ipc.ai.formatLyrics`). The Rust `AnthropicProvider`
  lives behind the optional `ai` cargo feature; the default build uses the
  local heuristic.
- **Needs Richard:** an Anthropic API key + a manual run to confirm the cloud
  path returns well-formed sections/arrangement and the cost estimate is sane.
  Verify the consent dialog truly precedes the first network call.

## 2. Turn the live bridge on (Phase 3 bridge consumer)

The Stage → Rec live-cue bridge and the Stage → Song usage bridge are wired
into the operator console (`useLiveBridge` in `LivePreview`) but **off by
default**. To enable:

- **A church/tenant id in the frontend.** Backend converged on a church tenant
  (Fase 0) but the id is not yet exposed to the UI. Once it is, build a
  `LiveBridgeContext` (`churchId`, `serviceId`, `serviceDate`, `wasStreamed`,
  and a `songsByItem` map from service-item id → `{ songId, title, variantId }`)
  and pass it to `LivePreview`.
- **A Realtime `publish` transport** (Supabase Realtime broadcast) for the
  `LiveEvent`s. NETWORK-UNVERIFIED.
- **A `usage` transport** (`UsageClientConfig`: SundaySong base URL + tenant
  token) for the usage POSTs. NETWORK-UNVERIFIED.
- **Verify on a live backend** per the rows in `docs/SMOKE-TEST.md`.

The `songsByItem` map is the one missing data join: cues carry a
`service_item_id` but not a catalog `song_id`. A small command that returns the
song id per service item (the planner already knows it) closes this — pure
mapping, no new logic risk.

## 3. Slide-editor canvas interactions (Phase 3.1)

The editing algebra (`src/lib/slideEditor/*`) is pure and thoroughly tested. The
direct-manipulation canvas (pointer drag/resize, guide rendering, multi-select)
is GUI-UNVERIFIED — needs a real window to confirm the snap guides feel right
and the undo/redo hotkeys behave under rapid input.
