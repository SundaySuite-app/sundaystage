# SundayStage «Stå på andres skuldre» — program

> Fire arbeidsspor vedtatt 2026-08-09 etter en OSS-research-runde (6 spor +
> lokal verifisering mot eiers installerte ProPresenter 6/7, EasyWorship og
> FreeShow). Modellert på telemetriprogrammet: **Fable** dirigerer, **Opus**
> tar tungt, **Sonnet** mekanisk; én etappe = én økt; eier sier **«kjør spor
> X etappe N»**. Full syntese: research-dossierene (R1/R1b/R2/R3/R4/R5).
> **Lisenslov: GPL/AGPL = kun idéer; adopsjon kun MIT/Apache-2.0/BSD/MPL,
> verifisert i faktisk LICENSE-fil. THIRD-PARTY.md ved første adopsjon.**

## Behold (research fant oss bedre enn/på nivå med forbildene)
Krasjisolasjon + watchdog på output-prosessen · telemetri-stacken ·
Supabase-broadcast-transporten (revurder ved 500 samtidige lesere) · fitText ·
tema/mal-kaskaden · sync.rs LWW. Disse byttes IKKE.

## Ufravikelige lover (arvet)
1. Live-stien er hellig. 2. Innhold forlater aldri maskinen. 3. Worker-først
ved skjemautvidelse. 4. Hver etappe: fulle gates → PR → CI grønn → merge →
etapperapport her → minne. 5. Eierbeslutninger ved etappe-START. 6. Maks 2–3
parallelle Opus.

---

## Spor A — Kvikk-vinnere + sikkerhet
Sikkerhet først; deretter billige kvalitetsløft. Rekkefølge:
- **A1 🔴 single-instance-vakt** (AKUTT — to prosesser på samme SQLite + to
  output-barn; PÅGÅR 08-09). tauri-plugin-single-instance (MIT).
- **A2 output-lås (⌘L) + clear-undo** (FreeShow-idé; blackout er på Escape uten
  angre i dag).
- **A3 scene-følger-blackout-flagg** (Quelea-idé).
- **A4 seksjonshopp-hurtigtaster** (OpenLP key-sekvens-idé) — bygg på
  react-hotkeys-hook (jukselapp auto-genereres; overlapper D2).
- **A5 window-state hovedvindu** (tauri-plugin MIT; KUN hovedvindu).
- **A6 crash-handler som signalkilde** (Embark MIT — fanger segfault/abort/OOM;
  aldri minne, lov 2 intakt; minidump kan ALDRI sendes).
- **A7 song_usage-logg** (fundament for CCLI/TONO-rapport — spor B/senere).
- **A8 OpenLyrics-EKSPORT** (skriv selv; ingen crate finnes; innlåsingsfiks).
- **A9 operatørside over /v1/admin/summary**.
- **A10 web: BroadcastChannel leder-fane + exponential-backoff**.

## Spor B — Import-suite (byttevennlighet)
Alle → `FormattedSong`-sømmen (`apply_formatted_song`). Praisenter (BSD-3) har
porterbare parsere + fixturer. Datamodell: valgfritt `chord_lines`
(per-linje `{chord,text}`) ved siden av autoritative `lyrics`.
- **B1 delt RTF-dekoder** (kreves av EasyWorship, .pro6, .pro7) — bygg først.
- **B2 EasyWorship** (ren SQLite; testes mot eiers 223 sanger). rusqlite. S.
- **B3 FreeShow .show** (ren JSON; eiers Velkommen.show). S.
- **B4 ProPresenter .pro6** (XML — LETTERE enn .pro7; eiers 62 norske filer;
  CCLI i attributter, base64-RTF). roxmltree/quick-xml (MIT). S.
- **B5 OpenLyrics import+eksport + SongSelect/CCLI** (Praisenter-port). S/M.
- **B6 ProPresenter .pro7** (protobuf; offentlig MIT-skjema, prost;
  ⚠️ aldri ekstraher fra eiers binær). M.
- **B7 bibel-referanseparser** (openbibleinfo, MIT) → erstatt håndlagd tabell.

## Spor C — Bibelkorpus
- **C1 korpus-seed** fra scrollmapper/bible_databases (kode MIT): KJV+ASV (en),
  Bibelen 1930 bokmål + nynorsk 1921 (PD bekreftet 2 veier). Map på FTS5.
  ⚠️ sjekk hver utgaves lisens (CrossWires annoterte KJV er GPL).
- **C2 nedlaster** README lover (verifiser sjekksum; ikke bunt hele korpuset i
  binæren hvis størrelse taler mot).
- **C3 Worker-proxy** for språk utover PD (FreeShow-mønster: ingen nøkkel i
  klient, attribusjon flyter ned; API.Bible krever re-sync ≤30 d).

## Spor D — UX-ombygging (PRESENTERES FØR KODING)
Eier kaller dagens UX svak; høy risiko akseptert. IKKE canvas-motor (ingen
driver ekte tekst i canvas; tldraw/Polotno proprietære). Behold DOM.
👤 **Ved START: Fable presenterer en konkret redesign-skisse + etappe-plan for
eier-godkjenning før noen kode skrives.** Kandidat-etapper:
- **D1 fundament-swap**: react-resizable-panels v4 + @dnd-kit/sortable (frys v6)
  + react-hotkeys-hook v5 (useKey:true for no/de/pl).
- **D2 dobbeltklikk-tekstredigering på canvas** (største enkeltløft; editor =
  `src/features/decks/`).
- **D3 lag-basert output** (bakgrunn/tekst/overlay/lyd + per-lag-clear —
  «dropp teksten, behold videoen»; reimplementer FreeShow/ProPresenter-mønster).
  Største funksjonelle vinner; L; egen etappe-serie.

## Ikke vedtatt ennå (kandidater klare, R4/R5)
Pro-output: OSC (rosc MIT) + MIDI (midir MIT) modne; NDI utsatt (web-display =
80 % via OBS Browser Source); ProPresenter-stage-display-API senere.
Framtid: CRDT-samarbeid (yrs+Hocuspocus MIT).

## 👤 Åpen eier-beslutning
Lisens: ingen repo har LICENSE-fil; README «TBD AGPL-3.0» vs sunday-platform
`UNLICENSED` — løs før offentlig lansering. GPL=idé-regelen holder dører åpne.

## Etappestatus
- [~] A1 single-instance-vakt — PÅGÅR 08-09
- [ ] Alt annet — venter «kjør spor X etappe N»
