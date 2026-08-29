-- 0010 — sangbrukslogg: hva menigheten faktisk fikk se (Spor A, A7)
--
-- Norske menigheter må rapportere hvilke sanger de har brukt: til TONO, og til
-- CCLI for det som er lisensiert der. Uten en logg er den rapporten umulig å
-- gjøre riktig — i praksis sitter noen og prøver å huske i etterkant. Denne
-- tabellen er grunnlaget, og alt annet (TONO-rapport, CCLI-rapport) bygger på
-- den.
--
-- LOV 2 (innhold forlater aldri maskinen) gjelder i sin strengeste form her:
-- sangtitler ER innhold. Raden lever lokalt i denne basen, eksporten er en fil
-- eier selv lager og selv sender videre, og INGENTING herfra kan nå
-- telemetrien. Ingen teller, ingen feilmelding og ingen loggsnutt i
-- `src-tauri/src/telemetry/` refererer til denne tabellen.
--
-- ── Kornstørrelse ──────────────────────────────────────────────────────────
-- Én rad per (gudstjeneste, sang, dato) — ikke én per slide.
--
--   * `service_id` alene ville vært feil: en gudstjenesteplan gjenbrukes fra
--     søndag til søndag, og to søndager er to bruk.
--   * dato alene ville vært feil: to gudstjenester samme dag (11:00 og 19:00)
--     er to bruk hvis planene er forskjellige.
--   * generalprøven 09:40 og gudstjenesten 11:00 på SAMME plan samme dag er
--     derimot ÉN bruk. Derfor akkumulerer skriveren (`ON CONFLICT DO UPDATE`)
--     i stedet for å lage en ny rad.
--
-- `show_count` bevarer gjentakelsen inne i én gudstjeneste: sangen som ble tatt
-- opp igjen etter prekenen står som én rad med `show_count = 2`. Det er den
-- opplysningen en rapport kan ha nytte av; én rad per slide er det ikke.
--
-- ── Snapshot, ikke fremmednøkkel ───────────────────────────────────────────
-- Verken `song_id` eller `service_id` er REFERENCES. Det er med vilje: loggen
-- er en historisk protokoll over hva som skjedde, og den skal overleve at
-- sangen slettes fra biblioteket eller at gudstjenesteplanen ryddes bort.
-- Derfor kopieres tittel/CCLI/TONO/copyright inn i raden når den skrives.
-- Rapporten for første kvartal skal fortsatt kunne sendes i april.

CREATE TABLE song_usage (
    id                TEXT PRIMARY KEY,
    -- Gudstjenesten sangen ble brukt i. Ikke en FK — se over.
    service_id        TEXT NOT NULL,
    -- Navnet slik det sto den dagen ("Gudstjeneste", "Kveldsmøte").
    service_name      TEXT NOT NULL,
    -- Lokal sivil dato, YYYY-MM-DD, avledet av når økta faktisk startet.
    -- Lagres som tekst fordi det er datoen rapporten skal vise, ikke et
    -- tidspunkt: en julaftensmesse 23:00 hører til den 24., ikke den 25.
    service_date      TEXT NOT NULL,
    song_id           TEXT NOT NULL,
    -- Snapshot av sangmetadataen på brukstidspunktet.
    title             TEXT NOT NULL,
    author            TEXT,
    ccli_song_id      TEXT,
    tono_work_id      TEXT,
    copyright_notice  TEXT,
    -- Første og siste gang sangen sto på menighetsskjermen (unix ms).
    first_shown_at    INTEGER NOT NULL,
    last_shown_at     INTEGER NOT NULL,
    -- Samlet tid sangen faktisk holdt utgangen. Grunnlaget for at «brukt» skal
    -- bety brukt: en gjennombla på et par sekunder havner aldri her.
    visible_ms        INTEGER NOT NULL,
    -- Antall separate ganger sangen sto på skjermen i denne gudstjenesten.
    show_count        INTEGER NOT NULL,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    UNIQUE (service_id, song_id, service_date)
);

-- Rapporten spør alltid «hva ble brukt i perioden X–Y», og oppbevaringsgrensa
-- sletter alltid «alt eldre enn Z». Begge er rekkeviddespørringer på samme
-- kolonne.
CREATE INDEX idx_song_usage_first_shown ON song_usage(first_shown_at);

-- «Når sang vi denne sist / hvor ofte» — bibliotek-siden senere.
CREATE INDEX idx_song_usage_song ON song_usage(song_id);

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (10, unixepoch() * 1000, 'sangbrukslogg for TONO/CCLI-rapportering (A7)');
