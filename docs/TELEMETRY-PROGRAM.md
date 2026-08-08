# SundayStage Kvalitetsprogram — «Sett i drift, aldri i veien»

> Fler-ukers program (~5 uker, 10 etapper + løpende runder): automatisk anonym
> telemetri med samtykke, krasjrapportering, manuell «Rapporter et problem» og
> beta-ring-oppdatering. Modellert på SundayRecs kvalitetsprogram (i drift) og
> SundaySyncs v0.2-program (`sundaysync/docs/V02-PROGRAM.md`), og gjenbruker
> deres infrastruktur og lærdommer. **Eier:** Richard. **Dirigent:** Fable per
> etappe; **Opus utfører** (Sonnet kan ta mekanisk i18n/boilerplate). Én etappe
> = én økt/natt; eier sier **«kjør etappe N»**. Vedtatt 2026-08-08; full
> planfil: `~/.claude/plans/planlegg-gjennomf-re-en-adaptive-dolphin.md`.

## Ufravikelige lover

1. **Live-stien er hellig** (kjerneløfte #1). Telemetri på live-stien = kun
   atomiske inkrement/bounded `try_send` (aldri disk/lås/async/Result som må
   håndteres); collector drenerer kun når `AppState.live` er `None`
   (try-lock-miss regnes som live); flush-pumpa hopper over hele beatet under
   live. Bevises med panikkende-sender-test. Unntak: panic-hooken skriver
   skrubbet krasjfil synkront — ved panikk finnes ingen live-sti å verne, og
   output-barnet overlever som egen prosess (hold-last-frame).
2. **Innhold forlater aldri maskinen**: sangtekster/-titler, tjenestenavn,
   filstier, enhetsnavn. Skrubbing FØR disk + Workerens `ABSOLUTE_PATH_RE` som
   siste skanse; en 400 logges lokalt med årsak — aldri stille tap
   (ellipse-lærdommen fra Rec 2026-08-08).
3. **Worker-først-utrulling** ved hver skjemautvidelse: Worker godtar nye felt
   som VALGFRIE, deployes og live-verifiseres — deretter klient-release. Aldri
   motsatt (klienten dropper 400 uten retry = stille permanent datatap). Hver
   fritekstgrense får test som spenner BEGGE repoer.
4. **Recs levende flåte må aldri merke noe**: `/v1/ingest`, `x-sundayrec-key`
   og `/v1/update/{stable,beta}` er FROSNE aliaser for alltid.
5. Hver etappe: fulle gates (fmt, clippy `-D warnings`, cargo test, tsc,
   eslint, vitest, Playwright, `export_bindings`) → PR → CI grønn → merge →
   etapperapport appendert HER → minne oppdatert.
6. Eierbeslutninger løftes ved etappe-START (listet per etappe). Aldri stille.
7. Maks 2–3 parallelle Opus-agenter; innholdstunge agenter skriver inkrementelt.

## Låste eiervalg (2026-08-08)

- **FULL anonym pakke**: krasj + kvalitet/problemer + bruk.
- **Samtykke opt-in**: onboarding-steg for nye installasjoner; ikke-blokkerende
  hjørnekort for eksisterende (ALDRI modal; aldri under live). Innstillinger:
  av/på + «vis hva som sendes» (ekte builder) + «slett mine data» + kø-status.
  Install-id mintes LAZY kun ved aktivt samtykke. `CONSENT_VERSION=1` fra dag
  én (tre-tilstands maskin `Option<ConsentRecord>`, aldri boolean). Sletting er
  IKKE samtykke-portet. 90 d rå-retensjon, EU (WEUR), aldri person/kirke-kobling.
- **Manuell rapport uten stående samtykke: JA, med flyktig engangs-UUID** —
  «Send» er samtykke for akkurat den rapporten; ingen varig id opprettes.
- **Delt `sunday-telemetry`-Worker generaliseres** med app-registry
  (`sundayrec|sundaystage|sundaysync`), per-app-ruter/nøkler/tabellfamilier —
  bygget ÉN gang slik at Sync E8 bare registrerer seg.
- **Beta-ring**: `updates.sundaysuite.app/v1/update/sundaystage/{stable,beta}`;
  ring-cutover skjer FØR telemetrislipp slik at utrullingen selv er
  kill-switch-styrt.

## Nøkkeldesign

**Worker-generalisering (E1):** `src/apps.ts`-registry; nye ruter
`POST /v1/apps/:app/ingest`, `DELETE /v1/apps/:app/install/:id`,
`GET /v1/update/:app/:channel`; generisk `x-write-key` med per-app-secret
(`WRITE_KEY_SUNDAYSTAGE` ny). Per-app TABELLFAMILIER (ikke bred delt tabell):
`ss_events` + barn + `ss_agg_*`; `tables.ts` → familie-registry; purge/delete
itererer familier; coverage-testen skalerer. Ny
`app_update_channels (app, channel, …)` seedet med `INSERT…SELECT` →
byte-identiske svar fra fødselen. Valideringskjernen (`Check`,
`ABSOLUTE_PATH_RE`, ellipse-+1) ut i `src/validate.ts`; `schema.ts`
re-eksporterer så Recs ~116 tester kjører uendret.

**Stage-payload v1 (E4):** konvolutt som Rec (schema=1, consentVersion,
installId, builtAt, appVersion, os, arch, language).
`settings` = 10 lukkede enum/bool/bucket-felt: outputCount, displayCountBucket,
stageDisplayEnabled, companionEnabled, fallbackInProcess, librarySongsBucket,
themeCountBucket, aiConfigured, syncEnabled, updateRing.
`counters` = ~19 navn i lukket allow-list: live.session.{started,ended,recovered},
live.cue.dispatched, output.{opened,fallback.in_process},
editor.{service.created,song.created}, ai.{format.run,search.run},
library.{publish.run,import.run}, bible.verse.projected, deck.presented,
theme.created, companion.connected, template.applied, update.installed,
report.manual.sent.
`quality` = én rad per live-økt: at, durationSec, cueCount, outputChildRestarts,
connectTimeouts, watchdogHolds, dispatchErrors, companionFailures, fallbackUsed,
staleChildReaped, abnormalEnd, recovered, cueLatencyP95Ms, verdict
(pass|warn|fail) + lukket reasons[] (output-restarted | hold-last-frame |
abnormal-end | dispatch-errors | slow-cues | companion-failures | fallback-used
| clean). Alle signalene beregnes allerede i dag og kastes.
`crashes` = Recs form (kind, at, appVersion, os, message ≤200, location ≤120,
task ≤64, backtracePresent) med stage-kinds: panic, task_panic, webview_error,
unhandled_rejection, other. Frontend-feil gjennom SAMME skrubbede ring.
Output-barnedød er IKKE krasj (→ quality).
`reports` (cap 3) = at, context (live|editor|settings|other), message ≤200,
logTail ≤4000 (skrubbet klient-side ved skriving OG lesing, screenet
server-side; dialogen forhåndsviser EKSAKTE utgående bytes).
Caps: body 64 kB, crashes 20, quality 20. `STAGE_OPTIONAL_PAYLOAD_KEYS = []`
fra dag én med avledet subset-test.

## Etappene

### E1 — Worker: app-dimensjon + app-skopede ringer _(sunday-telemetry)_

👤 Ved start: mint `WRITE_KEY_SUNDAYSTAGE` (`wrangler secret put`), bekreft
app-id-strengene og GitHub-repo-mappingen.
Registry, nye ruter + frosne aliaser, `x-write-key`, migr
`0006_app_update_channels` (copy-seed; gammel tabell står én uke), `update.ts`
→ (app, channel) — handleren ser fortsatt aldri `Request`; `promote.ts`/
`channel.ts` får `app` (default sundayrec → eksisterende admin-skript virker).
Tester: app×kanal-matrise, alias-byte-ekvivalens, promoterings-isolasjon.
**Gate: live kill-switch-drill mot Recs EKTE flåte-URL-er** (byte-identisk
begge URL-former, pause→204 ≤60 s, resume, kryss-app-isolasjon) — sjekkliste i
etapperapporten.

### E2 — Stage over på ringene + beta-ring _(sundaystage; slipp v0.5.0)_

👤 Ved start: publiser + godkjenn promotering; verifiser self-update på egen
maskin.
`tauri.conf.json` endpoints → `…/v1/update/sundaystage/stable` (pubkey
uendret); beta-toggle i SettingsPage (System) → Rust bygger updater med
`/beta`-endepunkt runtime; release.yml: **`uploadUpdaterJson`** (ekte
input-navn — `includeUpdaterJson` ignoreres stille) + **NSIS-only på
`-beta.`-tagger** (MSI kan ikke uttrykke prerelease); pinnet test på semver
beta→stable-retningen. Behold latest.json på GitHub-releasen t.o.m. v0.5.0
(0.4.0-flåtens siste GitHub-hopp; ringen tar over fra 0.5.0). i18n ×7.
Kill-switch-drill for stage-ringene.

### E3 — Lokalt observasjonsgrunnlag _(sundaystage; ingen tag, ingenting sender)_

ERSTATT `services/crash.rs` med `src-tauri/src/telemetry/`-familie à la Rec:
`scrub.rs` (sti-skrubber; fixture-hjelperens eksakte strenger blir E4s
kryss-repo-probe), `crash_ring.rs` (ring 20, skrubb-før-disk, catch_unwind,
OnceLock-dir). Fillogging: `tracing-appender` m/ størrelsestak + `log_tail(n)`
som skrubber ved LESING også; audit av ALLE tracing-kallsteder for
innholdslekkasje + pinnede tester. Frontend-fangst: `src/lib/errorReporting.ts`
(onerror, unhandledrejection, ErrorBoundary på OperatorWorkspace-roten) → IPC →
ring. Tellere (SQLite via repo-konvensjon) m/ inkrement-sømmer listet i
rapporten. Kvalitets-collector per lov 1, matet fra `output/process.rs`
(ChildStatus.restarts, connect-timeouts, watchdog), dispatch-/companion-
sømmene, oppstartsrekonstruksjon (stale child reaped / WAL funnet →
abnormalEnd-rad for forrige økt). Cue-latens: seq-korrelert dispatch→OutputAck
inn i fast atomisk histogram (≤10/25/50/100/250/>250 ms), p95 ved øktslutt —
fallback dispatch→broadcast m/ ærlig forbehold hvis ack ikke kan korreleres
billig. Live-vern-testene: panikkende sender, buffer-under-live-integrasjon,
ingen blokkerende metode i collector-API-et.

### E4 — Worker: sundaystage-skjema v1 _(sunday-telemetry; MÅ deployes før E5/E6)_

`src/validate.ts`-kjerne ut (Recs tester, inkl. truncation, kjører uendret),
`src/schema/sundaystage.ts`, migr `0007_sundaystage_events` +
`0008_sundaystage_aggregates` (kolonner-ikke-JSON, NOT NULL), familie-registry,
purge/delete/coverage utvidet, summary + queries.sql stage-seksjoner.
**Kryss-repo-suiter:** `test/seam-probe-stage.test.ts` matet av E3-skrubberens
EKSAKTE output; trunkeringstester for message/logTail/appVersion/location
(ellipse-+1); logTail-cap + sti-screen; verstefall-body-fixture.
Gate: deploy + live-curler (stage-fixture→202, ukjent felt→400,
rec-regresjon→202) i rapporten.

### E5 — Klient: samtykkemaskin, utboks, sender _(sundaystage; ingen tag)_

Porter fra Rec (tilpass, ikke del crate — lagring ulik; avvik dokumenteres):
samtykkemaskin (`Option<ConsentRecord>`, CONSENT_VERSION=1, absent-means-no,
stale-grant-not-a-grant; SQLite via ny telemetry-repository), lazy install-id
KUN ved aktivt samtykke, regenerate + pendingDeletions-park (sletting drenerer
uten samtykke), payload-builder + preview (ÉN builder for wire/preview/
rapport), vannmerke-drenering (idempotent), utboks (50 drop-eldst,
MAX_ATTEMPTS 6, stige 1 m→24 t, classify: 2xx drop / 429+408 transient / andre
4xx PERMANENT drop m/ lokal årsakslogg / 5xx transient), http_sender
(`option_env!("SUNDAYSTAGE_TELEMETRY_URL")` + `…_WRITE_KEY`; None → ingen
sender, dev-builds inerte), supervised pumpe m/ live-gate-beat. IPC
`telemetry.*` + ts-rs. Gammel `crash_reporting.json` pensjoneres og
auto-innvilger IKKE samtykke (lokal fangst-opt-in ≠ nettverkssamtykke).
Tester: overgangene, drain-idempotens, classify-tabell, live-gate,
preview==wire-bytes, no-consent-no-id, deletion-without-consent.

### E6 — Samtykke-UX + første telemetrislipp _(sundaystage; v0.6.0-beta.1, KUN beta)_

👤 Ved start: (1) samtykketekst v1 + behandlingsansvarlig-linje (dekker
aggregater OG flyktig-id-rapporter); (2) `gh secret set` URL+WRITE_KEY;
(3) personvern-copy godkjent (min. en+no).
Onboarding: WelcomeScreen får steg-maskineri (TutorialOverlay STEPS-mønsteret)
— steg 2 = samtykkespørsmålet, likeverdige knapper, lenke til full tekst.
Eksisterende installasjoner: hjørne-toast (aldri modal, aldri når live er
Some). SettingsPage: «Personvern»-kort erstatter Advanced-krasjkortet (toggle,
live preview m/ ekte builder, kø-status, slett-mine-data, install-id +
regenerer). «Rapporter et problem»: dialog (fritekst m/ 200-tegns teller,
skrubbet logghale-preview = eksakte utgående bytes, context-enum, flyktig
engangs-UUID uten stående samtykke), nås fra Settings + kommandopalett.
i18n ×7 + paritet; Playwright: onboarding-steget, toggle-flyt, rapportdialog.
PRIVACY.md + `docs/TELEMETRY.md` (ærlig offentlig beskrivelse).
Slipp: v0.6.0-beta.1 (NSIS-only win) → promote KUN beta (eier eneste medlem).
Gate: første ekte payload fra eierens maskin synlig i `/v1/admin/summary`.

### E7 — Beta-herding, feilrunde 1 _(begge repoer)_

Sjekkliste i rapport: øvings-live-økt → korrekt quality-rad (drep
output-barnet midt i → restart telt, verdict warn); indusert panikk → ring →
sendt etter samtykke; sletting ende-til-ende (UI → Worker DELETE → summary
bekrefter); manuell rapport lesbar server-side; ekte logghale manuelt audert
for innhold; rate-limit-oppførsel; samtykke AV midt i kø stopper sending.
Fikser → v0.6.0-beta.2 via ringen; Worker-fikser følger valgfritt-først.

### E8 — Stabil utrulling + re-prompt i felt _(promote v0.6.0 → stable)_

👤 Ved start: eksplisitt godkjenning av stable-promotering.
Overvåkingsuke: daglige summary-lesinger; verifiser at toasten dukker opp
etter oppdatering og ALDRI under live; ops-dokumentasjon ferdigstilles
(sunday-telemetry/README + queries.sql stage-kapittel).

### E9 — Feilrunde 2, datadrevet _(patch-slipp v0.6.x via ringene)_

Triage av første ekte data: topp krasjsignaturer, verdict-fordeling,
felt-cue-p95 mot 50 ms-budsjettet (første EKTE måling av kjerneløftet),
rapporttekster → topp 3–5 fikser → beta → stable. Skjemajusteringer:
Worker-først, `STAGE_OPTIONAL_PAYLOAD_KEYS`, subset-tester.

### E10 — Kontinuerlig drift

Stage inn i suitens felles månedsrytme (Rec E11 / Sync E12): telemetri/
krasj-triage → backlog, ring-helse, retensjonsverifisering, avhengighets-/
advisory-review, kvartalsvis kill-switch-drill, nøkkelhygiene. Leveranse:
runbook-seksjon her.

**Ukeplan:** uke 1: E1+E2 · uke 2: E3+E4 · uke 3: E5+E6 · uke 4: E7+E8 ·
uke 5: E9 (etter én ukes felt-soak) · E10 løpende.

## Risikoer (med mottiltak)

- **Ring-cutover mot levende Rec-flåte** → copy-seed + frosne aliaser +
  byte-identitets-drill før E1 lukkes; gammel tabell står én release.
- **logTail = farligste fritekstlinje** → trippelt vern: ingen-innhold-i-
  logger-regelen (E3-audit m/ tester), klient-skrubb ved skriving OG lesing,
  Workerens ABSOLUTE_PATH_RE + eksakte-bytes-preview i dialogen.
- **0.4.0-flåtens GitHub-hopp** → latest.json beholdes på GitHub t.o.m. v0.5.0.
- **Samtykke-støy en søndag** → toast sjekker live-state; aldri modal;
  e2e-pinnet.
- **Skjemadrift/stille tap** → hver fritekstgrense kryss-repo-testet;
  valgfritt-først står som lov øverst i dette dokumentet.

## Etappestatus

| Etappe                            | Status | Dato       | PR                                       |
| --------------------------------- | ------ | ---------- | ---------------------------------------- |
| Kartlegging + program vedtatt     | ✅     | 2026-08-08 | —                                        |
| E1 Worker app-dimensjon + ringer  | ✅     | 2026-08-08 | lokal merge `27a485f` (repo uten remote) |
| E2 Stage på ringene (v0.5.0)      | ✅     | 2026-08-08 | #42+#43; v0.5.0 Latest + promotert       |
| E3 Lokalt observasjonsgrunnlag    | ✅     | 2026-08-08 | #46; 3 innholdslekkasjer tettet          |
| E4 Worker stage-skjema v1         | ✅     | 2026-08-09 | Worker `f3f4c51` deployet; drill grønn   |
| E5 Klient: samtykke/utboks/sender | ⬜     |            |                                          |
| E6 Samtykke-UX + v0.6.0-beta.1    | ⬜     |            |                                          |
| E7 Beta-herding, feilrunde 1      | ⬜     |            |                                          |
| E8 Stabil utrulling               | ⬜     |            |                                          |
| E9 Feilrunde 2, datadrevet        | ⬜     |            |                                          |
| E10 Kontinuerlig drift            | ⬜     |            |                                          |

## Stående eierposter (løftes ved merket etappe, aldri stille)

- E1: mint `WRITE_KEY_SUNDAYSTAGE`; bekreft app-id-er.
- E2: publiser v0.5.0 + godkjenn promotering; self-update-verifisering.
- E6: samtykketekst + behandlingsansvarlig-linje; `gh secret set` URL+nøkkel;
  personvern-copy.
- E8: eksplisitt stable-godkjenning.
- Uavhengig av programmet: Apple-avtale-reaksept (notarisering) — gjelder hele
  suiten.

---

## Etapperapport E1 — 2026-08-08 ✅

**Levert:** app-dimensjonen i `sunday-telemetry` (lokal main `27a485f`, 5 commits):
lukket register (`src/apps.ts`), ruter `/v1/apps/:app/ingest`,
`/v1/apps/:app/install/:id`, `/v1/update/:app/:channel` m/ generisk
`x-write-key` (per-app-secrets; `WRITE_KEY_SUNDAYSTAGE` mintet → keychain
«SundayStage telemetry write key»), frosne Rec-aliaser gjennom SAMME kodesti,
migr `0006_app_update_channels` m/ copy-seed (NOT EXISTS-gardert), gammel
tabell står én uke som FROSSET øyeblikksbilde. 200 tester (139 gamle uendret i
substans + nye suiter: alias-byte-ekvivalens, app×kanal-matrise, nøkkelmatrise,
promoterings-isolasjon, migrasjonsform). Deployet (versjon `e7b871f3`),
migrasjon applisert atomisk.

**Live-drill (alle grønne):** byte-identitet legacy vs ny form for rec
stable+beta mot pre-migrasjons-baseline · stage/sync-ringer 204, ukjent
app/ring 404 · kill-switch: pause rec-beta (legacy default-app-form) → 204 på
5 s; under pause: rec stable identisk, stage-ringer isolert; resume (eksplisitt
app-form) → 200 byte-identisk på 5 s · nøkler: stage-nøkkel → 404
`ingest_not_configured` (auth passert, presis nekt til E4), feil nøkkel 401,
umintet sync-nøkkel 401 (deny-on-unset).

**Funn under etappen:**

1. ⚠️ **Ingen triggere i D1-migrasjonsfiler, noensinne** — wranglers
   REMOTE-splitter deler statements på HVERT `;`, også inne i triggerens
   BEGIN…END; selv én trigger sist i fila feiler med `incomplete input`
   (bevist mot prod; miniflare svelger det → lokalt grønt lyver).
   Første forsøk rullet atomisk tilbake (ingen delvis skade). Regelen er nå
   vakt-testet repo-bredt (`test/migration-0006.test.ts`). Konsekvens:
   `update_channels` er frossen, IKKE speilet — break-glass går KUN mot
   `app_update_channels` (queries.sql omskrevet); rollback i overgangsuka
   krever re-promotering av ALLE ringer promotert etter cutover.
2. ⚠️ **Parallell-økt-kollisjon:** Sync-økta hadde bygget en konkurrerende
   app-dimensjon (`e8/sundaysync-app-dimension`, app som payload-felt, egen
   0006). Eier avgjorde: E1-registeret vinner. 👤 **Sync-økta må rebase sin
   E8 på ny main**: renummerer migrasjonen til 0007+, registrerer
   `ingest`-oppføringen for sundaysync i `apps.ts`, bruker `x-write-key` +
   `WRITE_KEY_SUNDAYSYNC` (mintes da). Delt arbeidstre (`audit/wire-seams` m/
   foreldede ucommittede kopier) står urørt etter eiervalg.
3. ⚠️ Worker-repoet har INGEN remote (kun lokal git) — samme risiko som
   nettsiden hadde før 07-16. Anbefalt eierpost: sikre på GitHub (privat).
4. Deploy-propagering: de nye rutene trenger ~60 s (edge-cache) før
   byte-sammenlikning er meningsfull — første drill-sveip ga falske avvik.

**Avvik fra brief (alle begrunnet i commits):** coverage-testens
unntaksliste +6 linjer (dens egen feilmelding krever det) · `promoted_at`
INTEGER unix-ms (0003s faktiske type) · ukjent app = 404 på sti-ruter, 400 i
admin-body (speiler unknown_channel) · `/v1/admin/channels` beholder Recs
toppnivåfelt og legger `apps[]` ved siden av (eksisterende jq-uttrykk virker).

---

## Etapperapport E2 — 2026-08-08 ✅

**Levert:** ring-updater (PR #42, main `798294c`) + v0.5.0 UTE (PR #43, tag
bygget, publisert som Latest, promotert til `sundaystage/stable`).
Oppdateringssjekken flyttet til Rust — `UpdaterBuilder::endpoints` er eneste
runtime-søm (JS-pluginens CheckOptions kan ikke overstyre endepunkter);
signaturverifisering/nedlasting/installasjon urørt plugin-kode, pubkey uendret.
Kanalvalg (stable/beta) persistert etter flaggfil-mønsteret, toggle i
Innstillinger → Avansert, i18n ×7 (49 oppføringer). 204 = «oppdatert» pinnet.
Semver-retning pinnet ved å kjøre pluginens EGEN komparator over dens EGEN
deserialiserer (0.6.0-beta.1→0.6.0 JA, omvendt NEI) — Recs
prerelease-stripping-feil ikke importert. release.yml: `uploadUpdaterJson`
(ekte input — `includeUpdaterJson` var stille ignorert), NSIS-only +
`prerelease: true` på `-beta.`-tagger (beta kan aldri bli GitHub-Latest for
0.4.0-flåten). Gates: vitest 382 (+14), cargo 487 (+8), clippy/fmt/tsc/
eslint/prettier/Playwright grønt.

**Live-drill (alle grønne):** GitHub-feed serverer 0.5.0 (0.4.0-flåtens siste
hopp) · promote → ringen serverer manifest JSON-identisk med GitHubs
latest.json · beta-ring urørt 204 · kill-switch: pause → 204 på 5 s,
rec-ringene upåvirket under pausen, resume → byte-identisk på 5 s.

👤 **Gjenstår (eier):** self-update-verifisering på egen maskin — 0.4.0 →
0.5.0 via GitHub, deretter test ringen: promoter en fremtidig beta, flipp
kanalvelgeren, relansér. Følgesak (chore): `@tauri-apps/plugin-updater`
npm-pakken + `updater:default`-capability er ubrukt etter Rust-flyttingen —
fjernes i en senere deps-runde.

---

## Etapperapport E3 — 2026-08-08 ✅

**Levert (PR #46, main `7d208cd`):** alt som senere skal på ledningen finnes nå
lokalt — skrubbet, ring-avgrenset og bevist ute av stand til å røre live-stien.
Ingenting sender noe. `src-tauri/src/telemetry/` erstatter `services/crash.rs`:
`scrub.rs` (helhets-token-skrubbing — Recs halvvasket-sti-feil ikke importert;
**kryss-repo-fixturer committet i `src-tauri/telemetry-scrub-fixtures.json`**,
14+1 deterministiske caser → E4s seam-probe), `crash_ring.rs` (ring 20,
skrubb-før-disk, catch_unwind, ellipse-+1), `logfile.rs` (egen 5×2 MB
størrelsesrotasjon — tracing-appender bevisst DROPPET: roterer kun på tid,
NonBlocking er lossy-eller-blokkerende; Recs E2-valg), 19-navns lukket
teller-enum (serde-navnet ER wire-strengen), live-gated kvalitets-collector.
Frontend: ErrorBoundary + onerror/unhandledrejection → samme ring. Migr
`sql/0007_telemetry.sql`. Cargo 568 (+81), vitest 394 (+12).

**Tracing-audit: 3 EKTE innholdslekkasjer funnet og tettet** (alle kunne sende
sangtekst til logg): AppErrors Json-variant formatterer verdien serde feilet på
(= sangteksten) · output-barnets dispatch-feil siterte innholdet · io::Error
kan navngi socketen. Hvert kallsted nå pinnet av makro-parsende test m/
begrunnet unntaksliste.

**Live-vernet bevist fem veier:** panikkende-sink-injeksjon (live=Some → null
skriv), try-lock-miss = live, idempotent vannmerke, feilet-skriv-rekø, og
strukturell test som leser quality.rs og feiler på enhver blokkerende form i
LiveSafe-flaten. Cue-latens: seq-korrelert dispatch→ACK (CAS mot
dobbelt-skjerm-dubletter), fast 6-bøtte-histogram, ærlig 250 ms-metning.

**Gjenstår fra etappen (mekaniske følgesaker, listet i PR):** ~8 teller-sømmer
(editor/ai/bible/theme/template m.fl.), `deck.presented` = designbeslutning,
`companion.connected` venter på Phase 9-søm. ~~⚠️ Windows-pidfile-gap
(pre-eksisterende: pidfile på named-pipe-sti → stale-deteksjon inert på win)
→ egen chip.~~ **LUKKET** — pidfiler flyttet til `<app-data>/pidfiles/` på
BEGGE plattformer (`fix/pidfile-dir`); gammel unix-plassering skannes/ryddes
én utgivelse til. Drain-trigger er håndplassert (live_end/output_close/
update_install/oppstart); supervised pumpe m/ live-gate-beat er E5-leveranse.

---

## Etapperapport E4 — 2026-08-08/09 (natt) ✅

**Levert** (`sunday-telemetry` main `f3f4c51`, deployet `879641b4`, migr
0008+0009 applisert): valideringskjernen ut i `src/validate.ts` (Recs tester
uendret), `src/schema/sundaystage.ts` bygget mot E3s FAKTISKE klientformer
(8 dok-vs-kode-avvik løst — koden vant; bl.a. `schema` per krasjpost godtatt
som valgfri m/ default 1, verdict NOT NULL, cueLatency ikke pinnet til
250-metningen), `ss_events`-familien + `ss_agg_*` (kolonner, NOT NULL, ingen
triggere — vakten skjerpet til å kreve at den SER alle filer), parameterisert
ingest, familie-registry i tables/purge/delete/coverage, summary+queries.
331 tester (E4s 296 + Syncs 35), kryss-repo-fixturer verifisert mot
stage-checkouten via sjekksum-pinnet kopi + `check:fixtures`-skript.

**Midt i etappen merget Sync-økta sin E8 til main** — agenten flettet etter
plan: E8s mer generelle `TableFamily` (eventFk+grandchildTables) vant structen,
E4s registry+fold-map består, schema-sync repekt til validate.ts. INGEN
fremmede tester endret (byte-verifisert). 0007=sync, 0008/0009=stage.
purge_runs fikk per-app-kolonner (summen kan ikke si HVILKEN app).

**Live-drill:** stage-fixture → 202 · ukjent felt → 400 m/ presis issue ·
delete-by-id → per-tabell-kvittering over hele ss_-familien · rec-fixture
gjennom LEGACY-aliaset → 202 + legacy-delete OK (ekte regresjonsbevis) ·
alle ringer friske etter deploy. Testdata slettet via delete-endepunktet.

**⚠️ Krav til E5 (fra funn):** verstefall-payload = 53 505 B = 82 % av
64 kB-taket MED 2-byte-tekst — 4-byte-skalarer sprenger det. Senderen MÅ
byte-måle serialisert body FØR enqueue (413 droppes like stille som 400).
Og: QualityRows lokale felt (id, dedupe_key) er `unknown_field` på wire —
loud-testet. **Repo-nytt:** `sunday-telemetry` er nå PRIVAT på GitHub
(`SundaySuite-app/sunday-telemetry`, alle grener pushet etter secretskann).
