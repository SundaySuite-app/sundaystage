# Deprecated — superseded by SundayStage Web

This static companion PWA (the in-repo follow-along screen for phones) has been
**replaced by [SundayStage Web](https://stage.sundaysuite.app)** — a hosted
Next.js app (repo `sundaystage-web`) with the same goal but a real realtime
transport, a 6-digit join code, and both a fullscreen display mode (`/d/<code>`)
and a phone follow-along mode (`/f/<code>`).

The desktop app pushes live frames to it through the `webShare` hook (the
"Del over nettverk" button in the live console). These files are kept only for
reference and will be removed once the desktop integration ships.
