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
- **A7 ✅ song_usage-logg** (fundament for CCLI/TONO-rapport). Levert 08-30 —
  se etapperapporten nederst. Avledet av øktloggen, ikke av tastetrykkene:
  live-stien er uendret.
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
- [x] **A7** sangbrukslogg + TONO/CCLI-eksport — levert 08-30 (se rapport under)
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
`consoleKeys.ts`. Samme gjelder `G` (Go) og `L` (logo): de var bundet fra før og
er det fortsatt, så en seksjon som begynner på G eller L går òg via ⌘J. Alle tre
er bevisst prioritert som transport foran hopp.

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

---

## Etapperapport — A7, «sangbruksloggen», 2026-08-30

Menigheten må fortelle TONO hvilke sanger den har brukt, og CCLI det samme for
det som er lisensiert der. Til nå fantes det ingen logg, så rapporten var
umulig å gjøre riktig — noen satt i etterkant og prøvde å huske. Nå finnes
grunnlaget, og alt annet (TONO-skjema, CCLI-skjema, «når sang vi denne sist»)
kan bygges på det.

**Loggen leser øktloggen, ikke tastetrykkene.** Det viktigste valget i etappen.
`LiveSession` fører allerede en `log` — én rad per dispatch med tidspunkt,
cue-indeks og utgangstilstand — og SRT-eksporten og kapittelmarkørene leser den
gjennom `export::timeline`. A7 leser den **samme** tidslinja. Det gir tre ting
på én gang:

1. **Live-stien er urørt.** Ingen ny kanal, ingen SQLite-skriving, ingen
   allokering i sende-veien: raden dispatcheren allerede pusher ER
   kildematerialet. Regnestykket kjøres først når økta er over — på et tidspunkt
   der projektoren allerede er svart. `live_dispatch` har ikke fått én ny linje.
2. **Ingen kopi av logikken.** Tre funksjoner som svarer på «hva sto på skjermen
   når» kan ikke bli uenige når de leser samme `timeline`. `timeline` gikk fra
   privat til `pub(crate)` — det var hele integrasjonen.
3. **En tapt loggrad koster ingenting.** Et krasj før økta avsluttes taper
   loggen for den ene gudstjenesten — og gjenopprettingsstien fanger til og med
   det, siden en gjenopprettet økt føres når den avsluttes eller forkastes.

**«Faktisk brukt» — hvordan hvert tilfelle faller ut.**

| Tilfelle                         | Svar       | Hvorfor                                                                                          |
| -------------------------------- | ---------- | ------------------------------------------------------------------------------------------------ |
| Bare forhåndsvist                | ikke brukt | forhåndsvisning når aldri `dispatch`, så den finnes ikke i loggen                                |
| Lå i planen, aldri sendt         | ikke brukt | ingen tidslinjepunkt peker på cuen — null synlig tid                                             |
| Sendt mens utgangen var **låst** | ikke brukt | låsen avviser i `outputGuard` **før** `ipc.live.dispatch`, så Rust ser den aldri                 |
| **Blackout midt i sangen**       | **brukt**  | det svarte strekket teller ikke som synlig tid, men strekkene rundt gjør det — og det er én bruk |
| Rask gjennombla for å finne noe  | ikke brukt | under terskelen                                                                                  |

Det siste er terskelen, og den er det eneste stedet et skjønn måtte tas: **en
sang må ha holdt menighetsutgangen i minst 20 sekunder til sammen gjennom
gudstjenesten.** Begrunnelsen er de to feilene tallet står mellom — en operatør
som blar gjennom planen bruker under et par sekunder per sang, mens den korteste
ekte bruken (ett refreng lagt opp som svar etter en bønn) er langt over et halvt
minutt. Tallet er lagt nærmere gjennomblaen, fordi **under**rapportering til TONO
er en verre feil enn overrapportering.

At låsen faller ut som «ble aldri sendt» og ikke som et eget spesialtilfelle, er
A2 sin fortjeneste: låsen er en policy på `LiveAction`, ikke på en knapp, så en
handling som ikke kom gjennom den kom heller aldri til Rust. Det er nå pinnet
med en egen test i `outputGuards.test.tsx` som sier akkurat det — både
`live.start` og `live.dispatch` må være urørt etter at en volontør har prøvd
alt — slik at en framtidig rute som sniker seg forbi enten vakt feiler _der_,
med begrunnelsen.

**Én rad per sang per gudstjeneste per dato.** Ikke per slide, og ikke per
plan-post: to poster som peker på samme sang (åpningssang og reprise) blir én
rad. Nøkkelen har med **datoen** fordi en gudstjenesteplan gjenbrukes fra søndag
til søndag — `service_id` alene ville gjort to søndager til én bruk. Samtidig
**akkumulerer** skriveren på den nøkkelen, så generalprøven 09:40 og
gudstjenesten 11:00 på samme plan blir én bruk og ikke to.

Gjentakelsen er likevel bevart: `show_count` teller sammenhengende strekk på
minst fem sekunder, så sangen som ble tatt opp igjen etter prekenen står som én
rad med `2`. Blackout deler ikke et strekk i to — et avbrudd i den samme bruken
er ikke en ny bruk.

**Raden er et snapshot, ikke en fremmednøkkel.** Verken `song_id` eller
`service_id` er `REFERENCES`. Tittel, CCLI-nummer, TONO-verknummer, copyright og
opphavsperson kopieres inn når raden skrives, slik at rapporten for januar
fortsatt kan sendes i april selv om sangen er slettet fra biblioteket i februar.
Det er testet: raden overlever at sangen forsvinner.

**Rapporten sier hva den ikke vet.** CSV-en har en egen `Mangler`-kolonne som
navngir de opplysningene raden ikke har («opphavsperson, CCLI-nummer»). Et tomt
CCLI-felt kan bety «sangen er ikke CCLI-lisensiert» eller «vi vet det ikke», og
for den som skal signere rapporten er det to helt forskjellige ting. Kortet viser
det samme per sang før eier lager fila. UTF-8 BOM + semikolon, som er det norsk
Excel åpner uten importveiviser; titler som begynner med `=`, `+` eller `@` får
en apostrof foran (formelinjeksjon), mens `-` står urørt — å skrive om eierens
sangtittel i en rapport han skal signere er verre enn risikoen.

`Opphavsperson` leser den ekte `song_author`→`person`-relasjonen. Den er tom i
praksis i dag: importørene folder forfatterkreditten inn i `copyright_notice`
(SongSelect og .pro6 gir den som én kredittblokk), og redigereren har ikke noe
felt for den ennå. Kolonnen er koblet til det rette stedet, og `Mangler` sier
fra i mellomtiden — i stedet for å hente kreditten ut av copyright-feltet og
late som det er en strukturert opplysning.

**Personvern.** Loggen er lokal. Ingenting i `src-tauri/src/telemetry/` er rørt,
ingen ny teller kjenner tabellen, og skrivefeilen logges med et sømmerke og
**ingen** tekst — nettopp fordi loggens innhold er sangtitler og loggtaila kan
lastes opp. Eksporten skriver en fil i appens egen rapportmappe (ikke
Dokumenter/Nedlastinger, som macOS har trukket tilgangen til tre ganger i denne
suiten) og eier får en knapp som åpner mappa. Eier kan slette hele loggen fra
samme kort. **Oppbevaringsgrense: to år** — TONO- og CCLI-rapportering går i
årsløp, rapporten for et år skrives i det neste, og en purring eller korrigering
kan komme etter det igjen; 24 måneder dekker et helt rapportløp pluss ett til.
Ryddingen skjer der nye rader oppstår, altså når en gudstjeneste avsluttes.
`PRIVACY.md` har fått et eget punkt om loggen, på norsk og engelsk.

**Minimal flate.** Ett kort i Innstillinger → Avansert, over personvernkortet:
tre hurtigvalg (hittil i år / i fjor / siste kvartal), to datofelt, en liste over
hva perioden faktisk inneholder, «Lag CSV-fil», «Åpne mappen» og «Slett hele
loggen». Ingen layout er rørt — større UX hører til spor D og designcanvaset som
venter på eier.

**Migrasjon:** `sql/0010_song_usage.sql`. Enkle, delelige setninger — én
`CREATE TABLE`, to indekser, én `INSERT` i `schema_migrations`. **Ingen
triggere** (jf. suitens D1-lærdom). Ingen nye avhengigheter.

**Gates:** vitest 591 → 601 (+9 på periode-regningen, +1 som pinner
lås↔logg-koblingen), cargo 860 → 894 (+34: 20 rene på hva som er brukt og på
CSV-en, 8 på repositoriet, 5 ende-til-ende gjennom de ekte kommandoene, 1 på
eksportfila), Playwright 9 (uendret), clippy/fmt/prettier/eslint/tsc rene.

**Utsatt, med vilje:**

- **CSV-overskriftene er norske** selv om appen har sju språk. Mottakerne her er
  TONO og CCLI Norge; en tysk menighet som rapporterer til GEMA trenger et annet
  skjema uansett, ikke bare en oversatt overskriftsrad.
- **Loggen vet ikke om projektoren var koblet til.** En frame som ble rendret
  uten skjerm i andre enden teller som brukt. Det er riggens sak, ikke loggens,
  og å gjette på det ville gjort rapporten mindre sann, ikke mer.
- **`src/lib/usageEmitter.ts` er ikke slått på.** Den er en _sky_-bro til
  SundaySong (`/v1/usage/log`), med transporten avslått i `OperatorWorkspace`.
  A7 er den lokale loggen, og de to skal ikke forveksles: 👤 en beslutning om
  skybroen hører til eier og en egen etappe.
- **Ingen bibliotek-side «sist brukt / hvor ofte»** ennå, selv om
  `idx_song_usage_song` finnes for den.

👤 **Riggtest:** kjør en ekte gudstjeneste, avslutt den, og se at kortet viser de
sangene som faktisk ble sunget — og ingen av dem du bare bladde forbi. Sjekk at
CSV-fila åpner rett i norsk Excel (æøå, semikolon), at «Åpne mappen» finner fram
på både Mac og Windows, og at en sang du _forhåndsviste_ men aldri sendte ikke
står i lista.

👤 **Åpen beslutning:** 20 sekunder er terskelen. Hvis det viser seg at korte
bordvers og liturgiske svar faller ut i praksis, er tallet én linje i
`services/song_usage.rs` (`MIN_VISIBLE_MS`).
