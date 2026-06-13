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

## 1b. Live translation overlay — AI fill against a real key (Phase 11.2)

- **What's done:** the whole pipeline is built + unit-tested headless. A
  per-service `secondary_language` (queue-editor header). The cue compiler
  pre-resolves all translations at "Go Live" (`CueCompiler::resolve_translations`),
  in priority order: offline `translation_cache` → bundled public-domain Bible
  (`bundled_verse_translation`, keyless) → Anthropic (only with a key, cached).
  Resolved lines ride on `SlideContent.translation_lines` and render under the
  primary in `SlideView` + `StageDisplay`. The live output process makes NO
  network call — it only renders already-filled lines, so the crash-isolation
  contract is intact. Keyless degrades cleanly (cached/bundled still render).
  The request builder (`build_messages_body`) and parser (`parse_translation`,
  `extract_tool_input`) are pure + tested against canned fixtures.
- **Needs Richard:** (1) an Anthropic API key — the `ai` cargo feature must be
  compiled in (`cargo build --features ai`) AND a key resolvable via
  `keystore::resolve` — then verify a lyric service with a non-bundled target
  (e.g. `de`) gets sensible translations cached on first Go-Live and reused
  offline after. (2) a real multi-screen rig to eyeball the secondary line on
  the main output + stage display (font scale/legibility for long German/French
  lines). NETWORK- + RIG-UNVERIFIED.

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
