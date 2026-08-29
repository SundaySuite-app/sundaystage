# Personvern i SundayStage

_Gjelder SundayStage v0.6.0 og nyere. Norsk først, engelsk under._

SundayStage er lokal-først. Sangene dine, gudstjenestene dine og innstillingene
dine ligger på din egen maskin. Det eneste som kan sendes fra maskinen er
anonyme feil-, kvalitets- og brukstall — og bare hvis du sier ja.

---

## 1. Hva samles inn

Tre kategorier, og ingenting utenfor dem. Du kan når som helst se **nøyaktig**
neste rapport, byte for byte, under **Innstillinger → Avansert → Personvern →
«Vis hva som sendes»**.

Hver rapport har en konvolutt med: skjemaversjon, samtykkeversjon, den
tilfeldige installasjons-ID-en, tidspunktet rapporten ble bygget,
appversjon, operativsystem (`macos`/`windows`/`linux`/`other`), prosessorarkitektur
og språkkoden i menyene (`nb`, `en`, … — aldri landdelen).

**Krasj og feil** — per hendelse: type (`panic`, `task_panic`, `webview_error`,
`unhandled_rejection`, `other`), tidspunkt, appversjon, operativsystem,
feilmelding (maks 200 tegn, vasket for filstier), hvor i koden det skjedde
(maks 120 tegn), hvilken oppgave/komponent (maks 64 tegn), og om det fantes en
stakksporing. Maks 20 per rapport.

**Harde krasj** — hvis programmet blir drept på flekken (segmentfeil, avbrudd,
tom for minne), rekker ingenting av det over å bli skrevet. Da noteres et
**signal**, og det består av nøyaktig fem ting:

| Felt         | Eksempel                            | Hva det er                                                                                              |
| ------------ | ----------------------------------- | ------------------------------------------------------------------------------------------------------- |
| feiltype     | `segv`, `abort`, `ill`, `fpe`, …    | ett ord fra en fast liste på åtte                                                                       |
| feilkode     | `1`                                 | operativsystemets eget tall for feilen                                                                  |
| pekeradresse | `null`, `low`, `nonnull`, `unknown` | **kun** en firedeling — selve adressen noteres aldri                                                    |
| sted         | `app+0x1a2b3c` eller `foreign`      | hvor langt inne i SundayStages egen programfil det skjedde, eller bare «utenfor den» — aldri et filnavn |
| tråd         | `main-thread`, `other-thread`       | om det var hovedtråden eller en annen                                                                   |

Pluss tidspunktet, appversjonen og operativsystemet, akkurat som over.

**Det tas ALDRI et minnedump.** Et minnedump er en kopi av programmets minne, og
minnet inneholder sangteksten som sto på skjermen i det øyeblikket. Det finnes
ingen vasking som gjør en slik fil trygg, så den skrives ikke — heller ikke
lokalt, heller ikke usendt. Av samme grunn noteres ingen stakksporing, ingen
registre og ingen modulnavn.

Signalet skrives lokalt av selve krasjet og gjøres om til en vanlig krasjpost
neste gang du åpner appen. Du kan slå denne fangsten av under **Innstillinger →
Avansert → Personvern → «Fang harde krasj»**. SundayStage overtar heller ikke
krasjet: operativsystemet får det tilbake, så maskinens egen krasjrapport blir
skrevet som før.

**Kvalitet** — én rad per gudstjeneste (live-økt): tidspunkt, varighet i
sekunder, antall bilder som ble vist, antall omstarter av visningsprosessen,
tilkoblingstidsavbrudd, hold-siste-bilde-hendelser, sendefeil, companion-feil,
om nødløsningen ble brukt, om en gammel visningsprosess ble ryddet bort, om
økta endte unormalt, om den ble gjenopprettet, 95-persentilen for forsinkelsen
fra tastetrykk til bildebytte (i millisekunder), en dom (`pass`/`warn`/`fail`)
og en liste med faste årsakskoder. Alt er tall og faste koder. Maks 20 per
rapport.

**Bruk** — tellere fra en lukket liste på 19 navn, som tall siden forrige
rapport: `live.session.started`, `live.session.ended`, `live.session.recovered`,
`live.cue.dispatched`, `output.opened`, `output.fallback.in_process`,
`editor.service.created`, `editor.song.created`, `ai.format.run`,
`ai.search.run`, `library.publish.run`, `library.import.run`,
`bible.verse.projected`, `deck.presented`, `theme.created`,
`companion.connected`, `template.applied`, `update.installed`,
`report.manual.sent`.

**Tekniske innstillinger** — ti felt, alle enten av/på eller et grovt
intervall: antall åpne visninger, antall skjermer (som intervall),
scenevisning på/av, companion på/av, om nødløsningen i egen prosess er i bruk,
bibliotekets størrelse (som intervall), antall egne temaer (som intervall),
om en AI-nøkkel er satt opp, om sky-synk er på, og hvilken oppdateringsring
maskinen følger.

## 2. Hva samles ALDRI inn

Sangtekster, sangtitler, tjenestenavn, filnavn, filstier, enhetsnavn,
personnavn eller e-postadresser sendes aldri. Rapporter sendes aldri mens en
gudstjeneste pågår.

Dette er ikke bare et løfte om oppførsel: databasen har ingen kolonne slike
verdier kan ligge i, all fritekst vaskes for stier både når den skrives og når
den leses, og mottakeren avviser en rapport som likevel skulle inneholde en
absolutt filsti.

## 3. Samtykke

Deling er **av som standard**. Du blir spurt én gang — i oppstartsveiviseren på
en ny installasjon, eller i et lite hjørnekort på en installasjon som allerede
er i bruk. Kortet er aldri et modalvindu, og det dukker aldri opp mens en
gudstjeneste pågår. Lukker du kortet uten å svare, har du ikke svart: du blir
spurt igjen neste gang du åpner appen.

Samtykket er **versjonert**. Utvider vi omfanget med en ny kategori, stopper
sendingen umiddelbart og alle blir spurt på nytt. Et gammelt ja gjelder ikke
for et nytt spørsmål, og et gammelt nei blir aldri lest som et ja.

Skrur du delingen av, tømmes køen og de oppsamlede tellerne. «Av» betyr at det
ikke finnes noe igjen å sende — ikke en pause.

## 4. Anonymitet

Installasjons-ID-en er en tilfeldig UUID som opprettes **først når du sier ja**.
Den er ikke utledet av maskinvare, brukernavn, IP-adresse eller noe annet, og
den kobles aldri til en person eller en menighet. Du kan når som helst be om en
ny ID under Innstillinger → Avansert → Personvern; den gamle blir da bedt
slettet.

## 5. Sletting

**«Slett dataene mine»** i innstillingene ber serveren slette alt som er knyttet
til installasjons-ID-en din, og fjerner den lokale kopien. Knappen virker
**selv om deling er slått av** — den som har sagt nei er den som trenger den
mest. Er maskinen offline, utføres slettingen så snart den er på nett igjen.

## 6. Lagring

Rådata lagres i **90 dager** og slettes deretter automatisk. Aggregater uten
ID-er beholdes lenger. Data lagres i EU (Cloudflare, Vest-Europa).

## 7. Manuelle problemrapporter

«Rapporter et problem» sender teksten du skriver (maks 200 tegn), hvor i
programmet du var (`live`, `editor`, `settings` eller `other`), og de siste
vaskede logglinjene (maks 4 000 tegn). Dialogen viser nøyaktig det som sendes,
før du sender.

Dette kan du gjøre **selv om anonym deling er slått av**. Rapporten sendes da
med en **engangs-ID** som lages i det øyeblikket den sendes og ikke lagres noe
sted — den kan ikke knyttes til maskinen din, til installasjons-ID-en din eller
til en annen rapport. Én slik rapport bærer ingenting annet: ingen tellere,
ingen krasj, ingen kvalitetstall, ingen innstillinger.

## 8. Sangbruksloggen — lokal, og aldri en del av det som sendes

SundayStage fører en logg over hvilke sanger som faktisk sto på
menighetsskjermen, slik at menigheten kan rapportere til TONO og CCLI. Loggen
inneholder sangtittel, opphavsopplysninger, gudstjenestens navn og dato.

Den loggen **ligger på maskinen din og er ikke en del av noe som sendes**.
Sangtitler er innhold, og innhold forlater aldri maskinen — punkt 2 gjelder også
her: ingen teller, ingen feilmelding og ingen problemrapport bærer med seg noe
fra denne loggen.

Eksporten (Innstillinger → Avansert → Sangbruk) skriver en CSV-fil på maskinen
din. Om den fila sendes videre til TONO eller CCLI er noe **du** gjør, i din egen
e-post. Programmet sender den ikke.

Loggen kan slettes helt, når som helst, fra det samme kortet. Bruk eldre enn
**to år** ryddes bort automatisk: TONO- og CCLI-rapportering går i årsløp, og to
år dekker et helt rapportløp pluss ett til.

## 9. Behandlingsansvarlig

SundaySuite. <!-- 👤 bekreft juridisk enhet + kontakt-e-post -->

---

# Privacy in SundayStage

_Applies to SundayStage v0.6.0 and later._

SundayStage is local-first. Your songs, your services and your settings stay on
your own machine. The only thing that can be sent from it is anonymous error,
quality and usage numbers — and only if you say yes.

## 1. What is collected

Three categories, and nothing outside them. You can see **exactly** the next
report, byte for byte, at any time under **Settings → Advanced → Privacy →
"Show what is sent"**.

Every report carries an envelope: schema version, consent version, the random
install ID, when the report was built, app version, operating system
(`macos`/`windows`/`linux`/`other`), CPU architecture and the UI language code
(`nb`, `en`, … — never the region part).

**Crashes and errors** — per event: kind (`panic`, `task_panic`,
`webview_error`, `unhandled_rejection`, `other`), timestamp, app version,
operating system, the error message (max 200 characters, scrubbed of file
paths), where in the code it happened (max 120 characters), which task or
component (max 64 characters), and whether a backtrace existed. Max 20 per
report.

**Hard crashes** — if the program is killed outright (segmentation fault, abort,
out of memory), none of the above gets a chance to be written. A **signal** is
recorded instead, and it consists of exactly five things:

| Field      | Example                             | What it is                                                                                       |
| ---------- | ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| fault type | `segv`, `abort`, `ill`, `fpe`, …    | one word from a fixed list of eight                                                              |
| fault code | `1`                                 | the operating system's own number for the fault                                                  |
| pointer    | `null`, `low`, `nonnull`, `unknown` | **only** a four-way classification — the address itself is never recorded                        |
| site       | `app+0x1a2b3c` or `foreign`         | how far into SundayStage's own program file it happened, or just "outside it" — never a filename |
| thread     | `main-thread`, `other-thread`       | whether it was the main thread or another one                                                    |

Plus the timestamp, app version and operating system, exactly as above.

**No memory dump is ever taken.** A memory dump is a copy of the program's
memory, and that memory holds the lyrics that were on the screen at that moment.
There is no scrubbing that makes such a file safe, so it is not written — not
locally, not unsent. For the same reason no backtrace, no registers and no
module names are recorded.

The signal is written locally by the crash itself and turned into an ordinary
crash record the next time you open the app. You can switch this capture off
under **Settings → Advanced → Privacy → "Capture hard crashes"**. SundayStage
also does not take the crash over: the operating system gets it back, so your
machine's own crash report is still written as before.

**Quality** — one row per service (live session): timestamp, duration in
seconds, number of cues shown, output-process restarts, connection timeouts,
hold-last-frame events, dispatch errors, companion failures, whether the
in-process fallback was used, whether a stale output process was reaped,
whether the session ended abnormally, whether it was recovered, the 95th
percentile of keypress-to-screen latency in milliseconds, a verdict
(`pass`/`warn`/`fail`) and a list of fixed reason codes. All numbers and closed
codes. Max 20 per report.

**Usage** — counters from a closed list of 19 names, as numbers since the last
report: `live.session.started`, `live.session.ended`, `live.session.recovered`,
`live.cue.dispatched`, `output.opened`, `output.fallback.in_process`,
`editor.service.created`, `editor.song.created`, `ai.format.run`,
`ai.search.run`, `library.publish.run`, `library.import.run`,
`bible.verse.projected`, `deck.presented`, `theme.created`,
`companion.connected`, `template.applied`, `update.installed`,
`report.manual.sent`.

**Technical settings** — ten fields, each either on/off or a coarse band: number
of open outputs, number of displays (as a band), stage display on/off, companion
on/off, whether the in-process fallback is in use, library size (as a band),
number of custom themes (as a band), whether an AI key is configured, whether
cloud sync is on, and which update ring the machine follows.

## 2. What is NEVER collected

Lyrics, song titles, service names, file names, file paths, device names,
personal names and email addresses are never sent. Reports are never sent while
a service is running.

This is not only a promise about behaviour: the database has no column such
values could sit in, all free text is scrubbed of paths both when written and
when read, and the receiving endpoint rejects a report that would nevertheless
contain an absolute file path.

## 3. Consent

Sharing is **off by default**. You are asked once — in the first-run wizard on a
new installation, or in a small corner card on an installation already in use.
The card is never a modal, and never appears while a service is running.
Closing it without answering is not an answer: you are asked again next time you
open the app.

Consent is **versioned**. If we widen the scope with a new category, sending
stops immediately and everyone is asked again. An old yes does not cover a new
question, and an old no is never read as a yes.

Turning sharing off purges the queue and the accumulated counters. "Off" means
there is nothing left to send — not a pause.

## 4. Anonymity

The install ID is a random UUID created **only when you say yes**. It is not
derived from hardware, user name, IP address or anything else, and it is never
linked to a person or a church. You can ask for a new one at any time under
Settings → Advanced → Privacy; the old one is then queued for deletion.

## 5. Deletion

**"Delete my data"** in settings asks the server to delete everything tied to
your install ID, and removes the local copy. The button works **even when
sharing is off** — the person who said no is the one who needs it most. If the
machine is offline, the deletion is carried out as soon as it is online again.

## 6. Storage

Raw data is kept for **90 days** and then deleted automatically. Aggregates
without IDs are kept longer. Data is stored in the EU (Cloudflare, Western
Europe).

## 7. Manual problem reports

"Report a problem" sends the text you write (max 200 characters), where in the
app you were (`live`, `editor`, `settings` or `other`), and the last scrubbed
log lines (max 4 000 characters). The dialog shows exactly what is sent, before
you send it.

You can do this **even when anonymous sharing is off**. The report is then sent
with a **one-time ID** generated at the moment of sending and stored nowhere —
it cannot be linked to your machine, to your install ID or to another report.
Such a report carries nothing else: no counters, no crashes, no quality numbers,
no settings.

## 8. The song usage log — local, and never part of what is sent

SundayStage keeps a log of which songs actually reached the congregation screen,
so the church can report to TONO and CCLI. It holds the song title, credits, and
the service's name and date.

That log **stays on your machine and is not part of anything that is sent**.
Song titles are content, and content never leaves the machine — section 2 applies
here too: no counter, no error message and no problem report carries anything
from this log.

The export (Settings → Advanced → Song usage) writes a CSV file on your machine.
Whether that file is then sent to TONO or CCLI is something **you** do, in your
own mail client. The application does not send it.

The log can be deleted in full, at any time, from the same card. Usage older
than **two years** is cleared automatically: TONO and CCLI reporting runs in
annual cycles, and two years covers a full cycle plus one.

## 9. Data controller

SundaySuite. <!-- 👤 confirm legal entity + contact email -->
