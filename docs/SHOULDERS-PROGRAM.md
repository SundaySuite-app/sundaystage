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

- **A1 ✅ single-instance-vakt** (AKUTT — to prosesser på samme SQLite + to
  output-barn). tauri-plugin-single-instance (MIT). Levert 08-09 (PR #62).
- **A2 ✅ output-lås (⌘L) + clear-undo** (FreeShow-idé). Levert 08-29 sammen med
  A3 — se etapperapporten nederst. Blackout flyttet fra Escape til ⇧B i samme
  runde.
- **A3 ✅ scene-følger-blackout-flagg** (Quelea-idé). Levert 08-29. Standard AV:
  bandet beholder teksten når salen svartlegges.
- **A4 ✅ seksjonshopp-hurtigtaster** (OpenLP key-sekvens-idé, reimplementert
  selv). Levert 08-29 — se etapperapporten nederst. Jukselappen genereres nå
  fra `consoleKeys.ts`. Bygd på dagens tastetabell, ikke react-hotkeys-hook:
  fundament-byttet hører til D1.
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
  - react-hotkeys-hook v5 (useKey:true for no/de/pl).
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

- [x] **A1** single-instance-vakt — levert 08-09 (PR #62)
- [x] **A2 + A3** vernene rundt «klikk = live» — levert 08-29 (se rapport under)
- [x] **A4** seksjonshopp-hurtigtaster — levert 08-29 (se rapport under)
- [x] **B1** delt RTF-dekoder — 08-10 (PR #64)
- [x] **B2** EasyWorship-import — 08-10 (PR #66)
- [x] **B3** FreeShow .show-import — 08-10 (PR #67)
- [x] **B4** ProPresenter .pro6-import — 08-10 (PR #68)
- [x] **B5** OpenLyrics/SongSelect + eksport — 08-10 (PR #69)
- [x] **C1** bibelkorpus-nedlaster + 66-bokskanon — 08-10 (PR #65)
- [ ] Alt annet — venter «kjør spor X etappe N»

> ⚠️ Nummerering: eiers designrunde 08-29 omtalte «vernene»-etappen som
> «A4+A5». Innholdet er **A2 + A3** i lista over. Doklistas **A4** er nå
> levert; **A5** (window-state) er fortsatt uåpnet.

---

## Etapperapport — A2 + A3, «vernene», 2026-08-29

Fem endringer, alle atferd i dagens layout. Ingen ombygging: designcanvaset
(spor D) eier den jobben.

**1. Output-lås (⌘L / Ctrl+L).** En synlig bryter først i transportlinja, gull
og «LÅST» når den er på. Låsen er en policy på `LiveAction`
(`features/workspace/outputGuard.ts`), sjekket ett sted — inne i `dispatch` —
pluss `startSession`, som er den andre veien til projektoren. Alt annet
(klikk, mellomrom, Jump-modalen, meldingspopoveren, nettverksfjernkontrollen,
«vis nå» etter bibeltillegg) går allerede gjennom `dispatch`, så en ny knapp kan
ikke komme utenom låsen uten å finne opp en ny vei til Rust. Sjekken er én
tag-sammenlikning på et objekt vi allerede holder: ingen allokering, ingen I/O.
**Blackout og Nullstill slipper alltid gjennom** — nødstopp rangerer over lås.
Blokkert handling gir rist på låseknappen (deaktivert ved `prefers-reduced-motion`)
og en setning som sier hvorfor.

**2. Gjenopprett etter tømming (⌘Z / Ctrl+Z).** Nullstill fanger overstyringen
den er i ferd med å kaste (`clearUndo.ts`) og tilbyr den tilbake i 7 sekunder
med nedtelling og en ekte knapp. Bufferen er ett lite objekt i minnet — aldri
disk, aldri telemetri; det er menighetsvendt innhold. Tilbudet faller bort i det
noe annet flytter showet videre, for «gjenopprett» må bety skjermen operatøren
husker, ikke en overstyring lagt oppå en annen cue.

**3. Blackout flyttet fra Escape til ⇧B.** Escape er refleksen for å lukke en
dialog, og den pekte rett på menighetsskjermen. Escape lukker nå det dokkede
biblioteket og ellers ingenting. Bar `B` ble borte sammen med Escape: den nye
tasten er en bevisst to-fingers-akkord. 👤 **Verdt en eierbekreftelse** — bar `B`
er ProPresenter/FreeShow-vane, og kan legges tilbake med én linje hvis ønsket.

**4. A3 — sceneskjermen følger ikke blackout.** Nytt flagg i Innstillinger →
Utgang-visning, **standard AV**: bandet beholder teksten når salen svartlegges,
med et lite «Salen er svartlagt»-merke så ingen tror projektoren døde. Gjelder
både scenevisningen i appen og scene-/confidence-vinduene (render-hendelsen bærer
et `stageFrame` ved siden av `frame`). Én kjent kant: et scenevindu som åpnes
_midt i_ en blackout følger hovedutgangen til neste render-hendelse.

**5. Forhåndsvisningen dimmes ikke lenger.** Program og Forhåndsvisning er
likeverdige; gullringen og ● LIVE sier allerede hvem som er på lufta.

**Skjøtefiks på kjøpet.** Tastaturtabellen lå duplisert i
`tests/integration/workspace.test.ts` som en håndskrevet `resolveKey`-kopi —
testen målte speilet, ikke konsollet. Tabellen bor nå i `consoleKeys.ts`,
konsollet kaller den, og testene tester funksjonen. Samme runde fjernet et ekte
kappløp: `sessionRef` ble synket i en `useEffect` (én commit for sent), så en
Nullstill trykket rett etter en melding leste forrige økt og fant ingenting å
tilby tilbake — nettopp tilfellet angrefunksjonen finnes for.

**Gates:** vitest 430 → 502, cargo 860 (uendret), Playwright 9, clippy/fmt/
prettier/eslint/tsc rene. Ingen nye avhengigheter.

👤 **Riggtest:** ⌘L/Ctrl+L på begge plattformer, ⇧B under en ekte gudstjeneste,
⌘Z innen 7 s, og sceneflagget med et faktisk andre-skjerm-oppsett.

---

## Etapperapport — A4, «seksjonshopp», 2026-08-29

Bandet tar refrenget igjen. Operatøren har ett sekund, og i dag går det med til
å bla eller å åpne en modal. Nå skriver hun `R`, og refrenget står på skjermen.

**Tastesekvensen.** Bokstaver + tall, tolket mot sangen som spiller:

| Sekvens | Betyr                                                      |
| ------- | ---------------------------------------------------------- |
| `V2`    | Vers 2                                                     |
| `R`     | Refreng (`C` virker òg — filas eget ord)                   |
| `P`     | Pre-refreng                                                |
| `S`     | Stikk / Slutt — hva sangen nå faktisk kaller seksjonen sin |
| `↵`     | Ta hoppet en ventende sekvens tilbyr                       |
| `Esc`   | Avbryt sekvensen                                           |

**Bokstavene kommer fra dataene, ikke fra en tabell.** En seksjon svarer på
første ord i etiketten cuen bærer (`Verse 1`, `Chorus`) _og_ på første ord i
den samme etiketten på operatørens språk (`Vers 1`, `Refreng`). Derfor virker
`V`/`R` på norsk, `V`/`C` på engelsk, og en importert seksjon ingen har en
oversettelse for — `Stikk`, som EasyWorship/.pro6/FreeShow faktisk legger igjen
— nås som `S` uten at noen tabell må kjenne ordet. En håndskrevet mapping ville
vært feil første gang en menighet skrev sin egen etikett.

**Sekvensen er aldri usynlig.** Hvert tastetrykk vises i en brikke over
konsollet, sammen med seksjonen den peker på akkurat nå. Treffer den ingenting,
sier den det («ingen seksjon her») — «ingenting skjedde» er det ene svaret en
operatør midt i en gudstjeneste ikke kan handle på. Esc avbryter, og sekvensen
tømmer seg selv etter 1,2 s: et glemt `V` ville ellers stille gjort neste
tastetrykk til noe annet enn operatøren trodde.

**Hoppet fyrer i det sekvensen bare kan bety én ting.** `V2` går umiddelbart.
`R` går umiddelbart når sangen har ett refreng — og venter synlig på `↵` når
den har to, eller når `V` fortsatt kan bli `V2`. Ingen debounce-forsinkelse i
det vanlige tilfellet, og ingen halvferdig sekvens som fyrer av seg selv.

**Tre valg som betyr noe i praksis:**

1. **Omfanget er sangen som spiller, aldri hele planen.** Hver sang har et Vers
   1; `V` må bety _denne_ sangens. Hopp på tvers av sanger er ⌘J.
2. **Et gjentatt refreng løses forover.** Arrangementet spiller refrenget tre
   ganger, og cue-lista har tre kjøringer av det. Å sende showet tilbake til den
   _første_ ville sett riktig ut på projektoren og vært feil ett tastetrykk
   senere, for «neste» ville da spilt vers 2 om igjen. `R` velger kjøringen
   showet står i, ellers den neste, og runder først til toppen av sangen når det
   ikke er noe igjen framover.
3. **Hoppet går gjennom `dispatch`.** Et hopp er en vei til menighetsskjermen
   som alle andre, så output-låsen fra A2 fanger det uten å vite at det finnes.
   Testet eksplisitt mot det ekte konsollet: låst utgang → `ipc.live.dispatch`
   blir aldri kalt, og operatøren får setningen som sier hvorfor.

**Norsk tastatur.** Bindingene leser tegnet layouten faktisk produserte
(`e.key`), ikke en amerikansk fysisk-tast-antakelse — samme regel som resten av
tabellen. æ/ø/å adresserer en seksjon som enhver annen bokstav.

**Jukselappen genereres nå.** `?`-modalen leste en håndskrevet kopi av
tastetabellen. Kopien lå i forrige etappes tester og løy; denne lå i UI-et og
kunne gjøre det samme. Tabellen bor nå i `consoleKeys.ts` som `CONSOLE_SHORTCUTS`
med de _bokstavelige_ tastetrykkene hver rad reklamerer for, testene spiller dem
gjennom `resolveConsoleKey` og krever at raden snakker sant, og en egen test
krever at hver handling konsollet kan gjøre er dokumentert. Det avslørte
umiddelbart at Esc-lukker-biblioteket aldri hadde stått på lista.

**👤 Bar `B` står fortsatt ledig** — som bestilt. Prisen er at seksjoner som
begynner på B ikke kan tastes: «Bro»/«Bridge» nås av ⌘J, ikke av en bokstav.
Sier eier at bar `B` skal bli blackout, blir det slik; sier eier at broen er
viktigere, faller `B` inn i sekvensene med én linje. Begge deler er én linje i
`consoleKeys.ts`.

**Utsatt, med vilje:** hintlinja over slide-gridet («Klikk = på skjermen ·
piltaster blar · V2 hopper til Vers 2») er _ikke_ lagt inn. Den er layout, og
layout tilhører spor D og designcanvaset som venter på eier. Brikken og `?`
dekker oppdagelsen i mellomtiden. Ingen nye avhengigheter; react-hotkeys-hook
(D1) er ikke tatt inn.

**Gates:** vitest 502 → 591 (+89: 31 rene resolver-tester, 41 nye
tastetabell-tester, 14 mot det ekte konsollet, 3 på jukselappen), cargo 860
(uendret — ingen Rust-endring), Playwright 9, prettier/eslint/tsc rene.

👤 **Riggtest:** `V2`/`R` under en ekte gudstjeneste med norsk tastatur, på både
Mac og Windows; en sang med to refreng (`R` skal vente, `R2` skal treffe); en
importert sang med menighetens egne etiketter; og `?`-lista mot det tastene
faktisk gjør på riggen.
