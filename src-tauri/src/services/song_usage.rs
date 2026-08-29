//! A7 — sangbruksloggen: hva menigheten FAKTISK fikk se.
//!
//! Norske menigheter rapporterer sangbruk til TONO, og til CCLI for det som er
//! lisensiert der. En rapport man skal stå inne for tåler ikke gjetning, så
//! dette modulet har én jobb: avgjøre hvilke sanger som virkelig sto på
//! menighetsskjermen, og hvor lenge.
//!
//! ## Hvorfor den leser loggen, ikke tastetrykkene
//!
//! Alt her er avledet av [`LiveSession::log`] — den listen dispatcheren
//! allerede fører, gjennom nøyaktig samme [`timeline`] som SRT-eksporten og
//! kapittelmarkørene bruker. Det gir tre ting på én gang:
//!
//!   1. **Live-stien får null ekstra arbeid.** Ingen SQLite-skriving, ingen
//!      kanal, ingen allokering i sende-veien: loggraden `dispatch` allerede
//!      pusher ER kildematerialet. Regnestykket kjøres først når økta er over.
//!   2. **«Faktisk brukt» blir sant per konstruksjon** — se under.
//!   3. **Ingen kopi av logikken.** Tre funksjoner som svarer på «hva sto på
//!      skjermen når» kan ikke være uenige når de leser samme tidslinje.
//!
//! ## Hva «faktisk brukt» betyr her
//!
//! | Tilfelle | Svar | Hvorfor |
//! |---|---|---|
//! | Sangen ble bare forhåndsvist | ikke brukt | forhåndsvisning når aldri `dispatch`, så den finnes ikke i loggen |
//! | Sangen lå i planen, men ble aldri sendt | ikke brukt | ingen tidslinjepunkt peker på cuen — null synlig tid |
//! | Sangen ble sendt mens utgangen var låst | ikke brukt | låsen avviser handlingen i `outputGuard` **før** `ipc.live.dispatch`, så Rust ser den aldri, og loggen får ingen rad |
//! | Blackout midt i sangen | **brukt** | blackout-strekket teller ikke som synlig tid, men strekkene rundt gjør det, og de er samme bruk |
//! | Rask gjennombla for å finne noe | ikke brukt | under terskelen ([`MIN_VISIBLE_MS`]) |
//!
//! Merk at «utgangen var låst» ikke er et eget spesialtilfelle her — det ER
//! tilfellet «ble aldri sendt». Det er poenget med at låsen er en policy på
//! `LiveAction` og ikke på en knapp: en handling som ikke kom gjennom låsen,
//! kom heller aldri til Rust, og kan derfor ikke gjøre seg til en loggrad.
//!
//! Én ting modulet med vilje IKKE later som: at den vet om projektoren var
//! koblet til. Frames som ble rendret uten skjerm i andre enden teller som
//! brukt. Det er riggen sin sak, ikke loggen sin.

use std::collections::HashMap;

use crate::db::models::{ServiceItemSong, SongUsageRow};
use crate::services::cue_list::Cue;
use crate::services::live_session::{LiveSession, OutputState};
use crate::services::sundayrec_bridge::export::timeline;

/// Hvor lenge en sang må ha holdt menighetsutgangen, til sammen gjennom hele
/// gudstjenesten, før den føres som brukt.
///
/// 20 sekunder. Begrunnelsen er de to feilene terskelen står mellom: en
/// operatør som blar gjennom planen for å finne noe bruker brøkdeler av et
/// sekund per slide og under et par sekunder per sang, mens den korteste ekte
/// bruken — ett refreng lagt opp som svar etter en bønn — er langt over et halvt
/// minutt. Terskelen er lagt nærmere gjennomblaen enn den korteste sangen, for
/// under­rapportering til TONO er en verre feil enn over­rapportering.
pub const MIN_VISIBLE_MS: i64 = 20_000;

/// Hvor lenge ett sammenhengende strekk må vare for å telles som en egen
/// «gang» i [`UsedSong::show_count`]. Et strekk på under fem sekunder er en
/// operatør som passerte, ikke en gjentakelse.
pub const MIN_RUN_MS: i64 = 5_000;

/// Hvor lenge loggen tas vare på. To år.
///
/// TONO- og CCLI-rapportering går i årsløp: rapporten for et år skrives i det
/// neste, og en purring eller en korrigert rapport kan komme etter det igjen.
/// 24 måneder dekker et helt rapportløp pluss ett til. Mer enn det er
/// oppbevaring uten formål — og dette er menighetens innhold.
pub const RETENTION_DAYS: i64 = 730;

/// Hvor lenge én post i gudstjenesteplanen faktisk holdt utgangen.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemVisibility {
    pub service_item_id: String,
    /// Samlet synlig tid i ms.
    pub visible_ms: i64,
    /// Første og siste gang posten sto på skjermen (unix ms).
    pub first_at: i64,
    pub last_at: i64,
    /// Sammenhengende strekk på minst [`MIN_RUN_MS`].
    pub runs: i64,
}

/// En sang som beviselig ble brukt, klar til å føres i loggen.
#[derive(Debug, Clone, PartialEq)]
pub struct UsedSong {
    pub song_id: String,
    pub title: String,
    pub visible_ms: i64,
    pub first_at: i64,
    pub last_at: i64,
    /// Minst 1. Sangen som ble tatt opp igjen etter prekenen står som 2.
    pub show_count: i64,
}

/// Hvor lenge hver plan-post holdt menighetsutgangen i denne økta.
///
/// `ended_at` lukker det siste strekket — økta har ingen loggrad for at den
/// sluttet, så uten den ville sangen som sto på skjermen da gudstjenesten var
/// ferdig fått null tid.
///
/// Regelen for hva som teller er én linje: **utgangen må ha vist cuen**. Et
/// tidslinjepunkt med blackout, logo eller operatørmelding viser overstyringen,
/// ikke sangen, og bidrar derfor ingen tid — men det avslutter heller ikke
/// strekket, for en blackout midt i en sang er et avbrudd i den samme bruken,
/// ikke starten på en ny.
pub fn item_visibility(session: &LiveSession, ended_at: i64) -> Vec<ItemVisibility> {
    let pts = timeline(session);
    let mut acc: HashMap<String, ItemVisibility> = HashMap::new();
    // HashMap-iterasjon har ingen rekkefølge; gudstjenesten har det. Vi husker
    // rekkefølgen postene først kom på skjermen i.
    let mut order: Vec<String> = Vec::new();
    // Den posten som holder utgangen akkurat nå, og hvor lenge strekket har
    // vart. Blackout rører den ikke.
    let mut current: Option<(String, i64)> = None;

    for i in 0..pts.len() {
        let start = pts[i].at;
        let end = if i + 1 < pts.len() {
            pts[i + 1].at
        } else {
            ended_at
        };
        let span = (end - start).max(0);

        // Overstyrt utgang: menigheten så blackout/logo/melding, ikke cuen.
        if pts[i].output != OutputState::Normal {
            continue;
        }
        // Bare slide-cuer hører til en plan-post; blackout-/logo-/pause-cuer
        // har ingen sang bak seg.
        let Some(Cue::ShowSlide { source, .. }) = session.cue_list.get(pts[i].index) else {
            continue;
        };
        let item_id = &source.service_item_id;

        // Nytt strekk? Lukk det forrige og åpne et nytt.
        let continues = matches!(&current, Some((open_id, _)) if open_id == item_id);
        if continues {
            if let Some((_, run_ms)) = current.as_mut() {
                *run_ms += span;
            }
        } else {
            close_run(&mut acc, current.take());
            current = Some((item_id.clone(), span));
        }

        let entry = acc.entry(item_id.clone()).or_insert_with(|| {
            order.push(item_id.clone());
            ItemVisibility {
                service_item_id: item_id.clone(),
                visible_ms: 0,
                first_at: start,
                last_at: start,
                runs: 0,
            }
        });
        entry.visible_ms += span;
        entry.first_at = entry.first_at.min(start);
        entry.last_at = entry.last_at.max(start + span);
    }
    close_run(&mut acc, current);

    order
        .into_iter()
        .filter_map(|id| acc.remove(&id))
        .filter(|v| v.visible_ms > 0)
        .collect()
}

/// Regnskapsfør et avsluttet strekk som en «gang» hvis det varte lenge nok.
fn close_run(acc: &mut HashMap<String, ItemVisibility>, run: Option<(String, i64)>) {
    let Some((item_id, run_ms)) = run else {
        return;
    };
    if run_ms < MIN_RUN_MS {
        return;
    }
    if let Some(entry) = acc.get_mut(&item_id) {
        entry.runs += 1;
    }
}

/// Slå plan-poster sammen til sanger, og slipp bare gjennom de som passerte
/// [`MIN_VISIBLE_MS`].
///
/// Terskelen måles på SUMMEN for sangen, ikke på hvert strekk: en sang som ble
/// sunget i to omganger på tolv sekunder hver ble brukt, selv om ingen av
/// omgangene alene ville passert. Og to plan-poster som peker på samme sang
/// (åpningssang og reprise) blir én sang med to ganger — én rad, slik en
/// rapport skal ha den.
///
/// `songs_by_item` er den samme oppslagstabellen `ServiceRepo::get_songs_by_item`
/// gir bibliotek-broen; poster som ikke er sanger (skriftlesning, kunngjøring,
/// gap) er rett og slett ikke i den, og faller derfor ut av seg selv.
pub fn used_songs(
    items: &[ItemVisibility],
    songs_by_item: &HashMap<String, ServiceItemSong>,
) -> Vec<UsedSong> {
    let mut acc: HashMap<String, UsedSong> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for item in items {
        let Some(song) = songs_by_item.get(&item.service_item_id) else {
            continue;
        };
        let entry = acc.entry(song.song_id.clone()).or_insert_with(|| {
            order.push(song.song_id.clone());
            UsedSong {
                song_id: song.song_id.clone(),
                title: song.title.clone(),
                visible_ms: 0,
                first_at: item.first_at,
                last_at: item.last_at,
                show_count: 0,
            }
        });
        entry.visible_ms += item.visible_ms;
        entry.first_at = entry.first_at.min(item.first_at);
        entry.last_at = entry.last_at.max(item.last_at);
        entry.show_count += item.runs;
    }

    order
        .into_iter()
        .filter_map(|id| acc.remove(&id))
        .filter(|s| s.visible_ms >= MIN_VISIBLE_MS)
        // En sang som passerte terskelen sto på skjermen minst én gang, selv om
        // hvert enkelt strekk var kortere enn MIN_RUN_MS.
        .map(|mut s| {
            s.show_count = s.show_count.max(1);
            s
        })
        .collect()
}

/// Den lokale sivile datoen et tidspunkt hører til, `YYYY-MM-DD`.
///
/// Lokal, ikke UTC: en julaftensmesse som starter 23:00 norsk tid hører til den
/// 24., og det er den datoen rapporten skal vise. Faller tilbake til UTC hvis
/// tidssonen ikke kan leses.
pub fn service_date(at_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    if let chrono::LocalResult::Single(dt) = Local.timestamp_millis_opt(at_ms) {
        return dt.format("%Y-%m-%d").to_string();
    }
    chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

// ── Rapporten eier sender inn ───────────────────────────────────────────────

/// Kolonnene, i den rekkefølgen TONO/CCLI-skjemaene spør etter dem.
const CSV_HEADER: [&str; 9] = [
    "Dato",
    "Gudstjeneste",
    "Tittel",
    "Opphavsperson",
    "CCLI-nummer",
    "TONO-verknummer",
    "Copyright",
    "Antall bruk",
    "Mangler",
];

/// Hvilke opplysninger raden IKKE har.
///
/// Rapporten skal ikke late som. Et tomt CCLI-felt kan bety «sangen er ikke
/// CCLI-lisensiert» eller «vi vet det ikke» — og for den som skal signere
/// rapporten er det to helt forskjellige ting. Kolonnen sier hva som mangler,
/// slik at eier kan fylle det inn i biblioteket og kjøre eksporten på nytt.
fn missing_fields(row: &SongUsageRow) -> String {
    let mut missing: Vec<&str> = Vec::new();
    if row.author.as_deref().unwrap_or("").trim().is_empty() {
        missing.push("opphavsperson");
    }
    if row.ccli_song_id.as_deref().unwrap_or("").trim().is_empty() {
        missing.push("CCLI-nummer");
    }
    if row.tono_work_id.as_deref().unwrap_or("").trim().is_empty() {
        missing.push("TONO-verknummer");
    }
    if row
        .copyright_notice
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        missing.push("copyright");
    }
    missing.join(", ")
}

/// Ett tekstfelt, sitert.
///
/// Alt siteres — også det som ikke trenger det — fordi en sangtittel er
/// menighetens tekst og kan inneholde hva som helst, inkludert semikolon,
/// anførselstegn og linjeskift.
///
/// Felt som begynner med `=`, `+` eller `@` får en apostrof foran: Excel og
/// LibreOffice tolker dem som formler. `-` står med vilje urørt — en tittel kan
/// begynne med tankestrek, og å skrive om eierens sangtittel i en rapport han
/// skal signere er verre enn risikoen.
fn csv_field(value: &str) -> String {
    let guarded = match value.chars().next() {
        Some('=') | Some('+') | Some('@') => format!("'{value}"),
        _ => value.to_string(),
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// Bygg CSV-en for et sett loggrader.
///
/// Semikolon som skilletegn og UTF-8 BOM foran: det er kombinasjonen norsk
/// Excel åpner riktig uten importveiviser, og «operatøren fikk æøå til å se ut
/// som søppel» er ikke en feil å arve inn i en rapport.
pub fn to_csv(rows: &[SongUsageRow]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&CSV_HEADER.map(csv_field).join(";"));
    out.push_str("\r\n");
    for row in rows {
        let fields = [
            csv_field(&row.service_date),
            csv_field(&row.service_name),
            csv_field(&row.title),
            csv_field(row.author.as_deref().unwrap_or("")),
            csv_field(row.ccli_song_id.as_deref().unwrap_or("")),
            csv_field(row.tono_work_id.as_deref().unwrap_or("")),
            csv_field(row.copyright_notice.as_deref().unwrap_or("")),
            row.show_count.max(1).to_string(),
            csv_field(&missing_fields(row)),
        ];
        out.push_str(&fields.join(";"));
        out.push_str("\r\n");
    }
    out
}

/// Filnavnet en eksport får: `sangbruk-2026-01-01--2026-03-31.csv`.
pub fn export_file_name(from_ms: i64, to_ms: i64) -> String {
    format!(
        "sangbruk-{}--{}.csv",
        service_date(from_ms),
        service_date(to_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::{CueList, CueSource, SlideContent};
    use crate::services::live_session::LiveAction;

    /// A cue belonging to service item `item`.
    fn cue(id: &str, item: &str) -> Cue {
        Cue::ShowSlide {
            cue_id: id.to_string(),
            slide_content: Box::new(SlideContent {
                section_label: None,
                text_lines: vec![format!("line {id}")],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
                appearance: None,
            }),
            theme_id: None,
            template_id: None,
            source: CueSource {
                service_item_id: item.to_string(),
                item_cue_index: 0,
                display_label: id.to_string(),
            },
        }
    }

    /// A session whose cues belong, in order, to the given service items.
    fn session(items: &[&str]) -> LiveSession {
        let cues = items
            .iter()
            .enumerate()
            .map(|(i, item)| cue(&format!("c{i}"), item))
            .collect();
        LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues,
            },
            0,
        )
    }

    fn songs(pairs: &[(&str, &str)]) -> HashMap<String, ServiceItemSong> {
        pairs
            .iter()
            .map(|(item, song)| {
                (
                    (*item).to_string(),
                    ServiceItemSong {
                        song_id: (*song).to_string(),
                        title: format!("Sang {song}"),
                        variant_id: None,
                    },
                )
            })
            .collect()
    }

    fn ms(seconds: i64) -> i64 {
        seconds * 1_000
    }

    // ── hva som er brukt ────────────────────────────────────────────────────

    /// Den grunnleggende: cuen showet faktisk sto på får tiden.
    #[test]
    fn tiden_gar_til_cuen_som_faktisk_sto_pa_skjermen() {
        let mut s = session(&["a", "b"]);
        s.dispatch(LiveAction::Next, ms(60)); // a sto 0–60 s, b fra 60 s
        let vis = item_visibility(&s, ms(120));
        assert_eq!(vis.len(), 2);
        assert_eq!(vis[0].service_item_id, "a");
        assert_eq!(vis[0].visible_ms, ms(60));
        assert_eq!(vis[1].service_item_id, "b");
        assert_eq!(vis[1].visible_ms, ms(60));
    }

    /// En sang som lå i planen, men som showet aldri kom til, har null tid.
    ///
    /// Dette er også svaret på «utgangen var låst»: output-låsen avviser
    /// handlingen i `outputGuard` før `ipc.live.dispatch`, så Rust ser den
    /// aldri og loggen får aldri et punkt som peker på cuen. En blokkert økt er
    /// bokstavelig talt en økt uten loggrader.
    #[test]
    fn en_sang_som_aldri_ble_sendt_er_ikke_brukt() {
        // Ingen dispatch i det hele tatt — som når hver handling ble avvist.
        let s = session(&["a", "b", "c"]);
        let vis = item_visibility(&s, ms(600));
        assert_eq!(vis.len(), 1, "bare cuen showet sto på: {vis:?}");
        assert_eq!(vis[0].service_item_id, "a");

        let used = used_songs(&vis, &songs(&[("a", "s1"), ("b", "s2"), ("c", "s3")]));
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].song_id, "s1");
    }

    /// Blackout midt i en sang avlyser ikke bruken — og deler den ikke i to.
    #[test]
    fn blackout_midt_i_sangen_avlyser_ikke_bruken() {
        let mut s = session(&["a"]);
        s.dispatch(LiveAction::Blackout, ms(15)); // svart fra 15 s
        s.dispatch(LiveAction::Blackout, ms(75)); // tilbake 75 s
        let vis = item_visibility(&s, ms(120));
        assert_eq!(vis.len(), 1);
        // 0–15 s + 75–120 s = 60 s synlig; det svarte minuttet teller ikke.
        assert_eq!(vis[0].visible_ms, ms(60));
        assert_eq!(vis[0].runs, 1, "blackout deler ikke bruken i to");

        let used = used_songs(&vis, &songs(&[("a", "s1")]));
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].show_count, 1);
    }

    /// …men blackout er ikke synlig tid. En sang som ble svartlagt etter fem
    /// sekunder ble ikke brukt.
    #[test]
    fn blackout_teller_ikke_som_synlig_tid() {
        let mut s = session(&["a"]);
        s.dispatch(LiveAction::Blackout, ms(5));
        let vis = item_visibility(&s, ms(3600));
        assert_eq!(vis[0].visible_ms, ms(5));
        assert!(used_songs(&vis, &songs(&[("a", "s1")])).is_empty());
    }

    /// Logo og operatørmelding overstyrer utgangen på samme måte.
    #[test]
    fn logo_og_melding_teller_ikke_som_synlig_tid() {
        for override_action in [
            LiveAction::ShowLogo,
            LiveAction::ShowMessage {
                text: "Barnevakt til rom 2".into(),
            },
        ] {
            let mut s = session(&["a"]);
            s.dispatch(override_action, ms(5));
            let vis = item_visibility(&s, ms(3600));
            assert_eq!(vis[0].visible_ms, ms(5));
            assert!(used_songs(&vis, &songs(&[("a", "s1")])).is_empty());
        }
    }

    /// En gjennombla for å finne noe er ikke bruk.
    #[test]
    fn rask_gjennombla_telles_ikke() {
        let mut s = session(&["a", "b", "c", "d", "e"]);
        for i in 1..5 {
            s.dispatch(LiveAction::Next, ms(i)); // ett sekund per post
        }
        // …og så tilbake til starten, der showet blir stående.
        s.dispatch(LiveAction::GoTo { index: 0 }, ms(5));
        let vis = item_visibility(&s, ms(600));
        let used = used_songs(
            &vis,
            &songs(&[
                ("a", "s1"),
                ("b", "s2"),
                ("c", "s3"),
                ("d", "s4"),
                ("e", "s5"),
            ]),
        );
        assert_eq!(used.len(), 1, "bare den vi ble stående på: {used:?}");
        assert_eq!(used[0].song_id, "s1");
    }

    // ── én rad per sang, med gjentakelsen bevart ────────────────────────────

    /// Sangen som ble tatt opp igjen etter prekenen: én rad, to ganger.
    #[test]
    fn en_gjentatt_sang_gir_en_rad_med_to_ganger() {
        let mut s = session(&["a", "b", "a2"]);
        s.dispatch(LiveAction::Next, ms(120)); // a 0–120, b fra 120
        s.dispatch(LiveAction::Next, ms(600)); // b 120–600, a2 fra 600
        let vis = item_visibility(&s, ms(720));

        // To plan-poster peker på samme sang.
        let map = songs(&[("a", "s1"), ("b", "s2"), ("a2", "s1")]);
        let used = used_songs(&vis, &map);
        assert_eq!(used.len(), 2, "to sanger, ikke tre poster: {used:?}");
        let s1 = used.iter().find(|u| u.song_id == "s1").expect("s1");
        assert_eq!(s1.show_count, 2, "gjentakelsen er synlig");
        assert_eq!(s1.visible_ms, ms(240));
        assert_eq!(s1.first_at, 0);
        assert_eq!(s1.last_at, ms(720));
    }

    /// Terskelen måles på summen for sangen, ikke på hvert strekk: to omganger
    /// på tolv sekunder hver er 24 sekunder, og det er bruk.
    #[test]
    fn terskelen_er_summen_over_hele_gudstjenesten() {
        let mut s = session(&["a", "b", "a2"]);
        s.dispatch(LiveAction::Next, ms(12)); // a: 12 s
        s.dispatch(LiveAction::Next, ms(300)); // b: 288 s
        let vis = item_visibility(&s, ms(312)); // a2: 12 s
        let used = used_songs(&vis, &songs(&[("a", "s1"), ("b", "s2"), ("a2", "s1")]));
        let s1 = used
            .iter()
            .find(|u| u.song_id == "s1")
            .expect("24 s til sammen holder");
        assert_eq!(s1.visible_ms, ms(24));
        assert_eq!(s1.show_count, 2);
    }

    /// Gulvet: en sang kan passere terskelen uten at noe enkelt strekk var
    /// langt nok til å telles. Den ble likevel brukt — én gang.
    #[test]
    fn en_sang_uten_et_eneste_langt_strekk_star_likevel_som_brukt_en_gang() {
        let vis = vec![ItemVisibility {
            service_item_id: "a".into(),
            visible_ms: ms(25),
            first_at: 0,
            last_at: ms(600),
            runs: 0,
        }];
        let used = used_songs(&vis, &songs(&[("a", "s1")]));
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].show_count, 1);
    }

    /// Poster som ikke er sanger (skriftlesning, kunngjøring) skal ikke føres.
    #[test]
    fn ikke_sangposter_gir_ingen_rad() {
        let mut s = session(&["skriftlesning", "a"]);
        s.dispatch(LiveAction::Next, ms(300));
        let vis = item_visibility(&s, ms(600));
        assert_eq!(vis.len(), 2, "begge sto på skjermen");
        // …men bare sangposten er i oppslagstabellen.
        let used = used_songs(&vis, &songs(&[("a", "s1")]));
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].song_id, "s1");
    }

    /// En økt som aldri ble rørt krediterer likevel cuen den sto på — det er
    /// den menigheten så.
    #[test]
    fn en_urort_okt_krediterer_cuen_den_sto_pa() {
        let s = session(&["a"]);
        let used = used_songs(&item_visibility(&s, ms(45)), &songs(&[("a", "s1")]));
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].visible_ms, ms(45));
    }

    /// …men bare hvis den sto lenge nok. `ended_at` er det som lukker strekket.
    #[test]
    fn en_okt_som_ble_avbrutt_med_en_gang_gir_ingen_rad() {
        let s = session(&["a"]);
        assert!(used_songs(&item_visibility(&s, ms(3)), &songs(&[("a", "s1")])).is_empty());
    }

    /// `ended_at` før siste hendelse (en klokke som gikk baklengs) må ikke gi
    /// negativ tid.
    #[test]
    fn en_klokke_som_gar_baklengs_gir_ikke_negativ_tid() {
        let mut s = session(&["a", "b"]);
        s.dispatch(LiveAction::Next, ms(60));
        let vis = item_visibility(&s, ms(30));
        assert!(vis.iter().all(|v| v.visible_ms >= 0), "{vis:?}");
    }

    /// Tom cue-liste er trygt.
    #[test]
    fn tom_cue_liste_er_trygt() {
        let s = session(&[]);
        assert!(item_visibility(&s, ms(600)).is_empty());
    }

    #[test]
    fn service_date_gir_en_dato() {
        let d = service_date(1_756_000_000_000);
        assert_eq!(d.len(), 10, "{d}");
        assert!(d.starts_with("202"), "{d}");
    }

    // ── rapporten ───────────────────────────────────────────────────────────

    fn row(title: &str) -> SongUsageRow {
        SongUsageRow {
            id: "u1".into(),
            service_id: "svc".into(),
            service_name: "Gudstjeneste".into(),
            service_date: "2026-08-30".into(),
            song_id: "s1".into(),
            title: title.into(),
            author: None,
            ccli_song_id: None,
            tono_work_id: None,
            copyright_notice: None,
            first_shown_at: 0,
            last_shown_at: 1,
            visible_ms: ms(120),
            show_count: 1,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn csv_har_bom_semikolon_og_overskriftene_rapporten_trenger() {
        let csv = to_csv(&[]);
        assert!(csv.starts_with('\u{feff}'), "BOM mangler");
        let header = csv.lines().next().expect("overskrift");
        assert!(header.contains("\"Tittel\";\"Opphavsperson\";\"CCLI-nummer\""));
        assert!(header.contains("\"Copyright\""));
        assert!(header.contains("\"Antall bruk\""));
        assert!(csv.contains("\r\n"), "CRLF");
    }

    #[test]
    fn csv_siterer_semikolon_anforselstegn_og_linjeskift_i_titler() {
        let csv = to_csv(&[row("Han er «Herre»; og Konge\nfor alltid")]);
        let line = csv.split("\r\n").nth(1).expect("datarad");
        assert!(
            line.contains("\"Han er «Herre»; og Konge\nfor alltid\""),
            "{line}"
        );
    }

    /// En importert tittel som begynner med `=` er en formel i Excel.
    #[test]
    fn csv_uskadeliggjor_formler_men_lar_tankestrek_sta() {
        assert!(to_csv(&[row("=1+1")]).contains("\"'=1+1\""));
        assert!(to_csv(&[row("@SUM(A1)")]).contains("\"'@SUM(A1)\""));
        // Tankestrek er en helt vanlig start på en tittel og røres ikke.
        assert!(to_csv(&[row("— Uten navn —")]).contains("\"— Uten navn —\""));
    }

    /// Rapporten sier hva den ikke vet, i stedet for å la feltet stå tomt og
    /// se komplett ut.
    #[test]
    fn csv_navngir_det_som_mangler() {
        let bare = row("Salme");
        assert_eq!(
            missing_fields(&bare),
            "opphavsperson, CCLI-nummer, TONO-verknummer, copyright"
        );

        let mut full = row("Salme");
        full.author = Some("Lina Sandell".into());
        full.ccli_song_id = Some("7059628".into());
        full.tono_work_id = Some("TN-1".into());
        full.copyright_notice = Some("© 2020".into());
        assert_eq!(missing_fields(&full), "");

        // Blanke strenger teller som manglende — en importert tom attributt er
        // ikke en opplysning.
        let mut blank = row("Salme");
        blank.ccli_song_id = Some("   ".into());
        assert!(missing_fields(&blank).contains("CCLI-nummer"));
    }

    #[test]
    fn csv_skriver_en_rad_per_sang_med_antall_bruk() {
        let mut a = row("Åpningssang");
        a.show_count = 2;
        let b = row("Slutningssang");
        let csv = to_csv(&[a, b]);
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 3, "overskrift + to rader: {lines:?}");
        assert!(lines[1].ends_with(";2;\"opphavsperson, CCLI-nummer, TONO-verknummer, copyright\""));
        assert!(lines[2].contains(";1;"));
    }

    #[test]
    fn eksportfilnavnet_baerer_perioden() {
        let name = export_file_name(1_756_000_000_000, 1_764_000_000_000);
        assert!(name.starts_with("sangbruk-202"), "{name}");
        assert!(name.ends_with(".csv"), "{name}");
        assert!(name.contains("--"), "{name}");
    }
}
