# SundayStage — manual smoke tests

Each row is a behaviour whose **logic** is unit-tested but whose **I/O edge**
(GUI / network / device) has not run in this environment. Run these on a real
machine before trusting the feature on a Sunday.

The convention mirrors the rest of the suite: pure logic is verified in CI, the
wiring that touches the outside world is annotated `// GUI-UNVERIFIED` /
`// NETWORK-UNVERIFIED` in the source and listed here.

| Area                        | What to verify                                                                                                                                                                                                                                     | Source seam                                                         | Status             |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ------------------ |
| Slide-editor canvas         | Drag / resize a text block on the canvas; snap guides appear at frame edges, centre, and sibling edges; ⌘Z / ⌘⇧Z undo/redo a move, an add, a delete, and a theme apply.                                                                            | `src/lib/slideEditor/*` (algebra tested), canvas UI                 | GUI-UNVERIFIED     |
| Stage → Rec live cues       | Go live with a `LiveBridgeContext` + a real Realtime `publish` transport; SundayRec receives `service.live`, a `cue.advanced` per move, `now_playing` on each song change, `service.ended`. Sequence numbers are strictly increasing with no gaps. | `src/lib/useLiveBridge.ts` `forward()` → `publishLiveEvent`         | NETWORK-UNVERIFIED |
| Stage → Song usage          | With a real `usage` transport, advancing to a song once POSTs exactly one usage event to SundaySong `/v1/usage/log`; scrubbing back to the same song does **not** re-POST (idempotency key stable).                                                | `src/lib/useLiveBridge.ts` `forward()` → `postUsageEvent`           | NETWORK-UNVERIFIED |
| AI lyric formatting (cloud) | Paste raw lyrics, accept the consent dialog, run with a real Anthropic key; sections + arrangement come back formatted. (Offline heuristic path already works without a key.)                                                                      | `src/features/library/PasteFormatModal.tsx` → `ipc.ai.formatLyrics` | NEEDS-RICHARD      |
| SundayPlan import           | Load a real exported SundayPlan `.json`; matched songs link, unknown titles become stubs, scripture lands as a placeholder, warnings surface.                                                                                                      | `src-tauri/.../services/sundayplan.rs` (parse/map tested), file IO  | GUI-UNVERIFIED     |

## Notes on the live bridge

- The bridge is **off by default**: `LivePreview` is mounted without a
  `bridgeContext`, so `useLiveBridge` no-ops. The pure decision layer
  (`src/lib/liveBridge.ts`) is fully unit-tested regardless.
- To turn it on, a caller passes a `LiveBridgeContext` (church id + service date
  - `songsByItem` map) and `bridgeTransports` (`publish` and/or `usage`). The
    frontend has **no church/tenant id source yet** (backend Fase 0 only), so this
    stays off until that lands — see `docs/NEEDS-RICHARD.md`.
- Transport failures are swallowed inside `forward()`: the bridge must never be
  able to crash the live output (core promise #1).
