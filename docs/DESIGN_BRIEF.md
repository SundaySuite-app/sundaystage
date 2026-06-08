# SundayStage — Design Brief for a Graphical Overhaul

> **How to use this document.** Paste this whole brief into a fresh Claude design
> session (claude.ai / artifacts). It is self-contained — you do not need the
> codebase. Your job is to design **the best church live-presentation software in
> the world**, visually. Deliver **two complete visual directions** (A and B,
> defined in §5) so they can be compared side by side, both satisfying the shared
> foundations and the screen specs below.

---

## 1. What SundayStage is, in one paragraph

SundayStage is a **live presentation application for churches** — the software a
volunteer runs from a laptop at the back of the room on Sunday morning to put
song lyrics, Bible verses, and media on the big screen behind the worship band
and pastor. It is the visual stage companion to **SundayRec** (sermon
recording/streaming); together they are the "Sunday" suite. It must feel like a
**broadcast console fused with a beautiful modern desktop app**. Think
ProPresenter's power, QLab's reliability, the polish of Linear / Arc / Raycast,
and the calm of an app a nervous volunteer can run after ten minutes of training.

## 2. Who runs it, and the moment that matters

- **The operator** is usually a **volunteer**, not a pro — possibly a teenager or
  a retiree, often stressed, in a dim room, watching a band and a clock at the
  same time. They have one laptop, sometimes one extra monitor (the projector).
- **The sacred moment:** the congregation is watching the big screen. Whatever is
  "on air" is sacrosanct. The UI may stutter, the editor may be mid-edit — **the
  output must never blank, never show an error, never flash the wrong slide.**
- **Design consequence:** the interface must make the _current_ and _next_ states
  impossible to confuse, make "what's live" unmistakable at a glance from across a
  room, and make destructive/irreversible actions feel different from safe ones.
  Calm, legible, fast. No decoration that competes with the live signal.

## 3. The five promises (and what each means for design)

1. **Never crash on a Sunday morning** → the live signal has its own visual
   language (an "on-air" state) that is always visible and always truthful;
   recovery is graceful, never alarming.
2. **AI does the boring work** (formatting lyrics, finding songs, structuring
   services) → AI surfaces are _assistive and reviewable_, never magic boxes;
   always show what the AI proposed before it is applied.
3. **A volunteer can run it after 10 minutes** → progressive disclosure. The
   resting state is calm and minimal; power is summoned in, not always on-screen.
4. **Free tier genuinely useful forever; Pro genuinely cheap** → Pro markers are
   tasteful, never nag-screens.
5. **Mac and Windows are first-class equals** → no platform-specific chrome that
   looks alien on the other OS; respect native title-bar conventions on both.

## 4. Competitive cues — borrow the best, fix the rest

| App              | Steal this                                                                                             | Improve on this                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| **ProPresenter** | Power, the slide-grid mental model, multi-output, stage display                                        | It's busy, dated in places, has crashed live; we are calmer + more reliable |
| **EasyWorship**  | Approachable service flow                                                                              | Windows-leaning, dated; we are modern + truly cross-platform                |
| **FreeShow**     | Generous free feature set, flexible layouts                                                            | Electron-sluggish, weak AI; we are faster (Tauri) + AI-native               |
| **QLab**         | **Cue-list reliability philosophy**, arm/go discipline, the feeling of a deterministic show controller | Theatre-only, intimidating; we keep the rigor, lose the intimidation        |

The synthesis: **a worship console with QLab's nerve and Linear's polish.**

---

## 5. The two directions to deliver

Produce **both**, clearly labelled, sharing the foundations in §6–§13.

### Direction A — "Refined Sunday" (evolve the existing brand)

Keep the suite identity but push execution to best-in-class. This is the brand
shared with SundayRec, so it must stay recognizable.

- **Gold on deep navy.** Brand = deep navy; accent = warm gold. Dark-first.
- **"Stage-black" operator surfaces.** The slide preview/live monitors sit on a
  near-black "video-monitor" backdrop — they read as broadcast hardware, set
  apart from the navy app chrome. This stays dark even in light mode, so what the
  operator sees on screen matches the dark room.
- **Gold is reserved** for the _on-air signal_ and the _active selection_ — it is
  the most precious ink in the app; do not spend it on ordinary buttons.
- Reproduce the exact token values in §6 so it drops into the existing
  Tailwind v4 `@theme`.

### Direction B — "Fresh" (a bolder rethink)

Explore a new visual language **not bound to gold-on-navy**, while still
satisfying every functional requirement and every shared foundation. Push for
something that could become iconic in the worship-software category. A concrete
_starting_ proposal (you may diverge with rationale):

- A **high-contrast broadcast palette** — true blacks/charcoals with **one
  decisive signal color** (your choice; justify it — e.g. an electric
  coral/red-orange as the universal "live" cue, echoing tally lights, with a cool
  neutral for everything else).
- Sharper, more editorial typography; a stronger grid; more confident use of
  negative space. Could lean more "pro audio/video tool" than "church app".
- Still must keep the **on-air = unmistakable** principle and the dark
  monitor surfaces; the signal color does the job gold does in A.

For both directions, deliver: the **Operator Workspace** in full, a representative
**editor** screen, the **Stage Display**, and a **token sheet**. Then a short
note on which you'd ship and why.

---

## 6. Global visual system

### 6.1 Color (Direction A — exact tokens)

These are OKLCH values for a Tailwind v4 `@theme` block. Direction A must use
them; Direction B replaces the brand ramps but keeps the _semantic role_ names.

```
/* Neutral grayscale (drives dark + light) */
--color-neutral-50:  oklch(0.98 0 0);
--color-neutral-100: oklch(0.96 0 0);
--color-neutral-200: oklch(0.92 0 0);
--color-neutral-300: oklch(0.86 0 0);
--color-neutral-400: oklch(0.70 0 0);
--color-neutral-500: oklch(0.54 0 0);
--color-neutral-600: oklch(0.42 0 0);
--color-neutral-700: oklch(0.32 0 0);
--color-neutral-800: oklch(0.22 0 0);
--color-neutral-900: oklch(0.14 0 0);
--color-neutral-950: oklch(0.08 0 0);

/* Brand — gold on deep navy */
--color-sunday-blue-600: oklch(0.36 0.16 252);  /* primary brand */
--color-sunday-blue-900: oklch(0.13 0.07 252);  /* text on gold */
--color-sunday-blue-950: oklch(0.08 0.04 252);  /* console base */
--color-sunday-gold-400: oklch(0.84 0.16 85);   /* primary accent / ON AIR */
--color-sunday-gold-500: oklch(0.78 0.16 80);

/* Semantic roles — components reference THESE, never raw ramps */
--color-bg:          var(--color-neutral-950);
--color-bg-elevated: var(--color-neutral-900);
--color-bg-surface:  var(--color-neutral-800);
--color-fg:          var(--color-neutral-100);
--color-fg-muted:    var(--color-neutral-400);
--color-border:      var(--color-neutral-800);
--color-accent:      var(--color-sunday-gold-400);
--color-accent-fg:   var(--color-sunday-blue-900);  /* text on the gold */
--color-brand:       var(--color-sunday-blue-600);

/* Operator console / monitor surfaces (stay dark in BOTH modes) */
--color-stage-black: oklch(0.16 0.01 252);   /* the video-monitor backdrop */
--color-on-air:      var(--color-sunday-gold-400);
--color-on-air-ring: oklch(0.84 0.16 85 / 0.55);
--color-console:     var(--color-sunday-blue-950);

/* Status */
--color-success: oklch(0.74 0.18 145);
--color-warning: oklch(0.80 0.16 75);
--color-danger:  oklch(0.65 0.22 27);
--color-info:    oklch(0.70 0.14 245);
```

**Color rules (both directions):**

- The **on-air / live** color is the single loudest thing on screen and appears
  _only_ where something is genuinely live or armed-and-selected.
- **Blackout** is a real, deliberate state — show it as a calm full-dark with a
  small, unmistakable "BLACK" indicator, never as an error.
- **Danger red** is reserved for stop/destroy. Going live is _not_ danger — it is
  the on-air color.
- Light mode must be equally polished; **monitor surfaces stay dark** in light
  mode.

### 6.2 Typography — two parallel scales

UI font: **Inter** (system fallback). Mono: **JetBrains Mono** (chords,
shortcuts, IDs, timecodes).

```
/* UI scale (app chrome) */
--text-ui-xs: 12px;  --text-ui-sm: 14px;  --text-ui-md: 16px;
--text-ui-lg: 18px;  --text-ui-xl: 20px;  --text-ui-2xl: 24px;  --text-ui-3xl: 30px;

/* STAGE scale (content rendered on the projector/TV — optimized to read at distance) */
--text-stage-sm: 32px;  --text-stage-md: 48px;  --text-stage-lg: 64px;
--text-stage-xl: 88px;  --text-stage-2xl: 112px;  --text-stage-3xl: 144px;
```

The **stage scale** governs anything that simulates the projector output (slide
previews, the live monitor, the stage display). Lyrics on a slide are huge,
high-contrast, generously line-spaced. The UI scale governs chrome.

### 6.3 Space, radius, depth, motion

```
--radius-xs:4px --radius-sm:6px --radius-md:8px --radius-lg:12px --radius-xl:16px --radius-2xl:24px
--shadow-soft:     0 1px 2px rgba(0,0,0,.04), 0 2px 8px rgba(0,0,0,.04);
--shadow-popover:  0 8px 24px rgba(0,0,0,.12), 0 2px 6px rgba(0,0,0,.08);
--shadow-elevated: 0 16px 40px rgba(0,0,0,.18);
--ease-out-quart: cubic-bezier(0.25, 1, 0.5, 1);
--duration-fast: 120ms  --duration-base: 200ms  --duration-slow: 320ms
```

- **Density:** Linear/Raycast-dense for chrome (compact rows, tight toolbars),
  but the _monitors_ and _slide grid_ breathe.
- **Depth:** flat surfaces differentiated by the neutral ramp + hairline borders;
  shadows only for true overlays (popovers, modals).
- **Motion:** fast and functional. Cue advance and selection feel **instant**
  (≤120ms). Nothing bouncy on the live path. Overlays fade/scale subtly with
  `--ease-out-quart`. **Never** animate the on-air state in a way that delays the
  truth of what's live.

### 6.4 Iconography

`lucide-react`. Line icons, consistent weight. Transport metaphors people already
know: Play (Go Live), Square (Stop), filled square / `SquareDot` (Black),
`Monitor` (outputs), `Clapperboard` (stage screen), `Search`/`Menu` (browse),
`Keyboard` (shortcuts), `Settings`.

### 6.5 State language (the most important part)

Every interactive surface must clearly express these states; design a visible
vocabulary for each and use it consistently everywhere:

- **Live / on-air** — the gold (A) / signal color (B) glow + ring; a persistent
  "ON AIR" affordance in the transport.
- **Preview / staged** — the _next_ thing, clearly distinct from live (e.g. a
  cool outline / dashed or solid secondary ring). Operators live in the
  Preview→Go loop; this distinction is everything.
- **Armed but disabled** — transport actions (Black/Logo/Jump/Stage/Export) are
  greyed until a live session exists. Show _why_ (disabled, not missing).
- **Blackout** — calm full-dark state, explicit label.
- **Recovery** — a previous session ended abnormally; offer "Resume exactly where
  you were (cue N of M)" vs "Discard", reassuring not alarming.
- **Empty** — every list/grid has a warm, instructive empty state with one
  primary action.

---

## 7. THE core screen — Operator Workspace (the home screen)

This is where 90% of live time is spent. It is **one screen, always present**,
that converges the worship-console layouts of ProPresenter/EasyWorship/FreeShow
"done our way". Resting state = **three clean columns** under a transport bar;
heavier tools are _summoned in_ as overlays/drawers (progressive disclosure).

```
┌ TransportBar ───────────────────────────────────────────────────────────┐
│  ☰  ◷ Service ▾  │      ▶ Go Live   ■ Black   ◉ Logo   ● ON AIR      │ ⤢ Jump  ▦ Stage  ⤓ Export  ▣ Outputs  ⚙ ⌘K │
├──────────────┬───────────────────────────────────┬───────────────────────┤
│ ScheduleRail │            SlideGrid              │   PreviewLivePanel     │
│ (280px)      │            (flex)                 │      (340px)           │
│              │                                   │                        │
│ ▸ Opening    │  ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐         │  ┌──────────────────┐  │
│ ▸ Song 1     │  │  │ │  │ │██│ │  │ │  │  ← grid  │  │   PREVIEW (next) │  │
│   • Welcome  │  └──┘ └──┘ └──┘ └──┘ └──┘ of slide │  └──────────────────┘  │
│ ▸ Scripture  │   each tile = one cue/slide        │  ┌──────────────────┐  │
│ ▸ Sermon     │   live tile = ON AIR ring          │  │   LIVE  ● ON AIR │  │
│ ▸ Closing    │   preview tile = staged ring       │  └──────────────────┘  │
│              │                                   │   ▶ GO   notes / next  │
└─ MediaDrawer (summoned) ──────────────────────────────────────────────────┘
```

### 7.1 TransportBar (top, always visible)

The one strip always present, like every worship console.

- **Left:** browse (☰ opens Library), brand mark, **service picker** (dropdown of
  upcoming services + "New service"). A small clock / service timer.
- **Center:** the **live transport** — a prominent **Go Live ▶** (becomes
  **Stop ■** when live), **Black**, **Logo**, and the **ON AIR** indicator. Only
  show transport actions the engine actually performs — no decorative buttons.
  Black/Logo/Jump/Stage/Export are **armed-gated**: disabled until live.
- **Right:** Jump (⌘J), Stage display, Export, **Outputs** (monitor assignment),
  Settings, theme toggle, sync status ("Local" on free tier), Shortcuts (?).
- The transport must read as the **mission-critical control surface** — slightly
  heavier, more deliberate than the rest of the chrome.

### 7.2 ScheduleRail (left column, 280px)

The ordered plan of the service ("setlist"): grouped items — songs, scripture,
custom decks, media, announcements. Clicking an item **stages** its first slide
(sets preview). The currently-focused item is highlighted. Reorder by drag.
"Edit schedule" opens the fuller Services editor as an overlay. Each item shows a
type icon + title + small meta (key/translation/duration). Calm, scannable,
one-handed.

### 7.3 SlideGrid (center, the heart)

A responsive grid of **slide thumbnails**, one tile per cue. Each tile renders a
true miniature of the slide (using the stage type scale, scaled down) so the
operator recognizes content by _look_, not just label.

- **Live tile:** on-air ring + glow, unmistakable from across a room.
- **Preview/staged tile:** secondary ring.
- Section labels group tiles (Verse 1 / Chorus / Bridge…). Bible verses, deck
  slides, media each render appropriately.
- Click = stage (preview). The grid never _itself_ pushes to the projector —
  nothing reaches the screen without **Go**.
- Keyboard-first: arrows move the preview; Space/Enter/G = Go; Home/End jump ends.

### 7.4 PreviewLivePanel (right column, 340px)

Two stacked **dark monitors** on the stage-black surface — broadcast hardware feel:

- **PREVIEW (top):** the staged/next slide, exactly as it will look on the
  projector. Cool "preview" treatment.
- **LIVE (bottom):** what is _currently on the projector_, with the ON AIR
  treatment. This is the truth source.
- A big **GO ▶** promotes Preview → Live (then auto-stages the next cue — the
  worship flow). Below: service **notes** (sermon outline / tech cues) and a
  "coming next" hint. For Bible cues, a shortcut to open that passage.

### 7.5 MediaDrawer (bottom, summoned)

A horizontally-scrolling strip of media assets (images/video/backgrounds),
toggled in. Drag onto a slide / set as background. Broken-path assets show a
relink badge. Stays out of the way until needed.

---

## 8. Stage Display (the screen the band/pastor sees)

A separate **full-screen** view for on-stage monitors — NOT what the congregation
sees. High-contrast, glanceable from 5+ meters. Switchable **presets**:

- **Worship Leader:** current lyrics huge, next lines below, section label, clock
  - service timer.
- **Musician:** lyrics + chords, current/next, key, tempo.
- **Pastor:** sermon notes / clock / "time remaining" emphasis.

Design all three. Massive type (stage scale), minimal chrome, a prominent clock
and elapsed/remaining **service timer**, and a clear "what's next". Must be
readable in a bright room and a dark room. The on-air content here mirrors the
live frame exactly.

---

## 9. Library & content screens (summoned overlays)

A unified **Library Browser** opened over the console (so you never lose your live
context), with tabs: **Songs · Scripture · Decks · Themes/Design**. Plus a media
manager. Each below.

### 9.1 Songs (Library)

- **List + powerful search** (full-text, instant, <100ms feel) over potentially
  10k songs — must stay fast and scannable. Columns/rows show title, author, key,
  language, last-used, CCLI/TONO flags, tags.
- Filters by tag/key/language. Sort by recently used. A song row previews its
  arrangement at a glance.
- **Import** (drag a ChordPro/OpenSong/OpenLyrics/plain-text file → song) and
  **Paste-to-format** (see AI, §11) are first-class entry points.

### 9.2 Song Editor

Two halves: **content** and **arrangement**.

- **Sections editor:** reusable blocks — Verse 1, Chorus, Bridge, Intro,
  Instrumental, Tag — each with lyrics and optional **ChordPro chords**. Clean,
  text-first editing; section type is labelled and color-coded.
- **Arrangement builder:** drag sections into an **ordered, repeatable** sequence
  (Verse 1 → Chorus → Verse 2 → Chorus → Bridge → Chorus). Support **multiple
  arrangements** per song (Full / Short / Acoustic), one default.
- **Live generated-slide preview:** as you edit, show the slides this song will
  produce, in the current theme. Metadata: key, tempo (BPM), copyright,
  CCLI/TONO ids, language.

### 9.3 Scripture / Bible

Browse by book → chapter → verse; pick a translation (NIV, NB-30, NLB, KJV…);
select a verse range → it becomes a cue. Cached per translation (works offline at
service time). Reading-grade typography for the verse text; clear reference
display ("John 3:16–17, NIV"). A fast reference parser ("joh 3:16-17").

### 9.4 Custom Decks + Slide/Deck Editor (the Figma-like canvas)

For announcements, sermon points, anything not a song or verse.

- A **direct-manipulation canvas** at the projector's aspect ratio: drag/resize
  text + image + shape blocks, with **snap guides**, alignment, multi-select,
  and **undo/redo**. Think a focused, friendly Figma for slides.
- A right-hand **Inspector** for the selected block (font, size, color, align,
  position, background, fit).
- **Theme + Template** controls (see §10). A slide list / filmstrip on one side.
- Must feel precise but not intimidating — a volunteer makes an announcement
  slide in under a minute.

### 9.5 Media manager

A grid of assets (images, video, backgrounds) with type filter, thumbnails,
broken-path badges, and **relink** (find-by-content when a file moved). Import via
native file dialog or drag-drop.

---

## 10. Themes & Templates (orthogonal styling)

- **Theme = colors + fonts.** **Template = layout + positions.** They are
  independent: the "Lyrics Centered" template should look right with any theme.
- A theme/template gallery with live previews. A **token editor** to tweak a
  theme (background, text color, font, shadow, safe-area).
- Built-in themes/templates ship; per-library customizations are copies. Show the
  **cascade** clearly when relevant: a slide inherits theme/template from
  slide → song → library → built-in default, and **never blanks** — a missing id
  falls back to default rather than an empty screen.

---

## 11. AI surfaces (assistive, always reviewable)

The killer features. AI must feel like a **fast, trustworthy assistant**, never a
black box. Always show the proposal before applying; always offer offline
fallback where possible; always disclose when content is sent to an AI provider.

### 11.1 Paste-to-format (the lyric formatter — the marquee feature)

A modal where the operator **pastes raw, messy lyrics** (from anywhere) and the AI
returns a **clean, structured song**: detected sections (verse/chorus/bridge),
stripped chords/markers, collapsed chorus repeats, guessed language. Design:

- Left: raw paste. Right: the **proposed structured result** (sections +
  arrangement) the operator can review and edit before "Apply".
- A **streaming** feel as it formats. A clear "Apply to library" vs "Cancel".
- A graceful **offline heuristic** path when no AI key is set (label it honestly).

### 11.2 Service-planning assistant

From the library, describe a service ("4 songs in D for a communion Sunday, one
quiet, plus John 6") → the AI proposes an ordered **service plan** of _real songs
from this library_ (unknown picks downgrade to notes, never hallucinated). Review
→ Apply creates a real Service with items.

### 11.3 Consent & key storage

A clean first-use **consent dialog** (what's sent, to whom, opt-in), and a Settings
surface to store the API key securely. Tasteful, honest, not scary.

---

## 12. Command palette (⌘K) and Jump (⌘J)

- **⌘K** — a Raycast/cmdk-style palette: navigate anywhere, search songs /
  scripture / services / decks, run actions. Fuzzy, instant, keyboard-driven,
  beautiful. This is a signature surface — make it sing.
- **⌘J** — **live quick-jump**: during a service, fuzzy-jump straight to any cue
  in the current list. Tighter, faster, focused on the live cue list.

---

## 13. Settings, Onboarding, and recovery

- **Settings:** outputs/displays assignment, language (en/no/sv/da/de/fr/pl),
  theme, AI key + consent, sync. Organized, calm, searchable.
- **Onboarding / Welcome:** a warm first-run that seeds demo content (a couple of
  public-domain songs + a playable "Welcome Service") so the app is never empty.
  A short, skippable **tutorial overlay** highlighting Go / Black / Preview→Go and
  the slide grid.
- **Recovery banner:** if a live session ended abnormally, a calm bottom-center
  banner: "Your last live session was interrupted — resume exactly where you were
  (cue N of M)" with Resume / Discard. Reassuring, never an error wall.

---

## 14. Live-performance interaction rules (non-negotiable)

1. **Nothing reaches the projector without an explicit Go.** Clicking only
   _stages_ (preview).
2. **The live output never blanks on UI trouble** — design states assume the
   monitor keeps showing the last frame.
3. **Cue advance feels instant** (≤50ms target; design no animation that delays
   it).
4. **Live vs Preview is never ambiguous** — distinct, learned color/ring language.
5. **Blackout and Logo are one keypress and one click** and always reachable in
   the transport while live.
6. **Destroy/Stop looks different from Go** — danger red vs on-air color.
7. **Keyboard-first**: the whole live flow runs from the keyboard (arrows, Space/
   Enter/G = Go, B = Black, L = Logo, ⌘J = Jump, Home/End, ? = shortcuts). The
   chrome should teach these shortcuts inline.

## 15. Accessibility & distance-reading

- **Stage/projector content**: maximum contrast, huge type, generous leading, safe
  margins; legible from the back row and on a washed-out projector.
- **Operator chrome**: WCAG AA minimum; visible focus rings; the on-air signal
  distinguishable for color-blind users (pair color with a shape/label, e.g. an
  "ON AIR" word + dot, not color alone).
- Full keyboard operability; large hit targets on transport controls (stress +
  dim room + maybe a trackpad).
- Dark-first, but a genuinely polished light mode (monitors stay dark in both).

## 16. Deliverables

For **each** of Direction A and Direction B:

1. **Operator Workspace** — the full home screen (TransportBar + ScheduleRail +
   SlideGrid + PreviewLivePanel), in a **live** state (something on air, something
   staged) and a **resting/empty** state.
2. **Stage Display** — at least the Worship-Leader preset.
3. One **editor** screen — the Slide/Deck canvas _or_ the Song Editor.
4. One **AI modal** — Paste-to-format.
5. The **⌘K** command palette.
6. A **token sheet** (colors as semantic roles, type scales, radii, shadows,
   motion) — for Direction A, reuse the exact values in §6; for B, your own,
   mapped to the same role names so it drops into a Tailwind v4 `@theme`.
7. **State studies** for the shared vocabulary: on-air, preview, armed-disabled,
   blackout, recovery, empty.

Then a short written rationale and a recommendation on which to ship.

## 17. Technical constraints (so the design is buildable)

- Target stack: **Tauri 2 + React 19 + TypeScript + Tailwind CSS v4** with an
  `@theme` token block, **shadcn/ui** primitives (customized), **lucide-react**
  icons, **cmdk** for ⌘K. Design with these in mind — design tokens map to CSS
  custom properties; components are composable primitives.
- **Mac + Windows parity** — don't depend on a macOS-only chrome look; respect
  each OS's window controls.
- Dark-first with full light-mode parity; monitor surfaces dark in both.
- Performance budget mindset: 10k-song library stays instant; nothing in the live
  path is heavy.

## 18. Do / Don't

**Do**

- Make "what's live" the single loudest thing on screen.
- Keep the resting state calm; summon power in.
- Treat the slide monitors as broadcast hardware on a dark surface.
- Teach shortcuts inline; reward the keyboard.
- Make AI proposals reviewable and honest.

**Don't**

- Don't add decorative buttons that don't map to a real action.
- Don't spend the precious accent/on-air color on ordinary UI.
- Don't make Go Live look dangerous, or make Stop look inviting.
- Don't ever design a state where the projector could show an error or blank.
- Don't make a volunteer hunt — the live controls are always one reach away.

---

### Appendix — hotkey reference (reflect these in the chrome)

| Key                     | Action                          |
| ----------------------- | ------------------------------- |
| `→` / `↓`               | Stage next cue (move preview)   |
| `←` / `↑`               | Stage previous cue              |
| `Space` / `Enter` / `G` | **Go** (promote Preview → Live) |
| `B` / `Esc`             | Blackout                        |
| `L`                     | Show logo                       |
| `Home` / `End`          | Stage first / last cue          |
| `⌘J` / `Ctrl-J`         | Live quick-jump to any cue      |
| `⌘K` / `Ctrl-K`         | Command palette                 |
| `?`                     | Keyboard shortcuts              |
