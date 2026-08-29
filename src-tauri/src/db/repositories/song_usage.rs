//! A7 — sangbruksloggen: hva menigheten faktisk fikk se, per gudstjeneste.
//!
//! Loggen er grunnlaget for TONO- og CCLI-rapportering. Den er **lokal**: ingen
//! metode her har en nettverksvei, og ingen teller i `telemetry/` leser
//! tabellen. Sangtitler er innhold, og innhold forlater aldri maskinen.
//!
//! Avgjørelsen om HVA som skal føres tas i `services::song_usage`; her ligger
//! bare lagringen — skriv, les periode, rydd og slett.

use sqlx::SqlitePool;

use crate::db::models::{SongUsageEntry, SongUsageRow};
use crate::db::{new_id, now_ms};
use crate::error::AppResult;

pub struct SongUsageRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SongUsageRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Før én sangs bruk i én gudstjeneste.
    ///
    /// Akkumulerer på (gudstjeneste, sang, dato). Det er generalprøven kl. 09:40
    /// og gudstjenesten kl. 11:00 som skal bli én bruk, ikke to — mens den samme
    /// planen brukt neste søndag får sin egen rad, fordi datoen er en annen.
    ///
    /// Metadata-snapshotet oppdateres ved konflikt: kjørte eier importen som
    /// fylte inn CCLI-nummeret mellom prøven og gudstjenesten, er det den nyeste
    /// opplysningen rapporten skal bære.
    pub async fn record(&self, entry: &SongUsageEntry) -> AppResult<()> {
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO song_usage (
                id, service_id, service_name, service_date, song_id, title, author,
                ccli_song_id, tono_work_id, copyright_notice,
                first_shown_at, last_shown_at, visible_ms, show_count,
                created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
            ON CONFLICT (service_id, song_id, service_date) DO UPDATE SET
                service_name     = excluded.service_name,
                title            = excluded.title,
                author           = excluded.author,
                ccli_song_id     = excluded.ccli_song_id,
                tono_work_id     = excluded.tono_work_id,
                copyright_notice = excluded.copyright_notice,
                first_shown_at   = MIN(song_usage.first_shown_at, excluded.first_shown_at),
                last_shown_at    = MAX(song_usage.last_shown_at, excluded.last_shown_at),
                visible_ms       = song_usage.visible_ms + excluded.visible_ms,
                show_count       = song_usage.show_count + excluded.show_count,
                updated_at       = excluded.updated_at
            "#,
        )
        .bind(new_id())
        .bind(&entry.service_id)
        .bind(&entry.service_name)
        .bind(&entry.service_date)
        .bind(&entry.song_id)
        .bind(&entry.title)
        .bind(&entry.author)
        .bind(&entry.ccli_song_id)
        .bind(&entry.tono_work_id)
        .bind(&entry.copyright_notice)
        .bind(entry.first_shown_at)
        .bind(entry.last_shown_at)
        .bind(entry.visible_ms)
        .bind(entry.show_count)
        .bind(now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Loggen for en periode, eldst først — rekkefølgen en rapport leses i.
    ///
    /// Grensa går på `first_shown_at` (når sangen faktisk sto på skjermen), ikke
    /// på når raden ble skrevet.
    pub async fn list_between(&self, from_ms: i64, to_ms: i64) -> AppResult<Vec<SongUsageRow>> {
        let rows = sqlx::query_as::<_, SongUsageRow>(
            r#"
            SELECT * FROM song_usage
            WHERE first_shown_at >= ?1 AND first_shown_at <= ?2
            ORDER BY first_shown_at, title
            "#,
        )
        .bind(from_ms)
        .bind(to_ms)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Hvor mange rader loggen har i alt — det eier ser før han sletter den.
    pub async fn count(&self) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM song_usage")
            .fetch_one(self.pool)
            .await?;
        Ok(n)
    }

    /// Slett hele loggen. Eiers rett, uten forbehold.
    pub async fn clear(&self) -> AppResult<u64> {
        let res = sqlx::query("DELETE FROM song_usage")
            .execute(self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Oppbevaringsgrensa: kast alt som ble brukt før `cutoff_ms`.
    pub async fn prune_before(&self, cutoff_ms: i64) -> AppResult<u64> {
        let res = sqlx::query("DELETE FROM song_usage WHERE first_shown_at < ?1")
            .bind(cutoff_ms)
            .execute(self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn db() -> Database {
        Database::open_in_memory().await.unwrap()
    }

    fn entry(song_id: &str, date: &str, at: i64) -> SongUsageEntry {
        SongUsageEntry {
            service_id: "svc".into(),
            service_name: "Gudstjeneste".into(),
            service_date: date.into(),
            song_id: song_id.into(),
            title: format!("Sang {song_id}"),
            author: None,
            ccli_song_id: None,
            tono_work_id: None,
            copyright_notice: None,
            first_shown_at: at,
            last_shown_at: at + 60_000,
            visible_ms: 60_000,
            show_count: 1,
        }
    }

    #[tokio::test]
    async fn record_then_list_between() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap();
        repo.record(&entry("s2", "2026-01-11", 9_000))
            .await
            .unwrap();

        let all = repo.list_between(0, 10_000).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].song_id, "s1", "eldst først");

        let narrow = repo.list_between(5_000, 10_000).await.unwrap();
        assert_eq!(narrow.len(), 1);
        assert_eq!(narrow[0].song_id, "s2");
        assert_eq!(repo.count().await.unwrap(), 2);
    }

    /// Generalprøven og gudstjenesten samme dag på samme plan er ÉN bruk.
    #[tokio::test]
    async fn samme_gudstjeneste_samme_dag_akkumulerer_i_en_rad() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 9_000))
            .await
            .unwrap(); // 09:40
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap(); // 11:00

        let rows = repo.list_between(0, 100_000).await.unwrap();
        assert_eq!(rows.len(), 1, "én rad: {rows:?}");
        assert_eq!(rows[0].visible_ms, 120_000);
        assert_eq!(rows[0].show_count, 2);
        assert_eq!(rows[0].first_shown_at, 1_000, "tidligste vinner");
        assert_eq!(rows[0].last_shown_at, 69_000, "seneste vinner");
    }

    /// …men den samme planen neste søndag er en NY bruk.
    #[tokio::test]
    async fn samme_plan_ny_dato_gir_ny_rad() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap();
        repo.record(&entry("s1", "2026-01-11", 600_000))
            .await
            .unwrap();
        assert_eq!(repo.count().await.unwrap(), 2);
    }

    /// Snapshotet oppdateres når biblioteket lærer noe nytt om sangen.
    #[tokio::test]
    async fn metadata_snapshotet_oppdateres_ved_ny_bruk() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap();
        let mut with_ccli = entry("s1", "2026-01-04", 2_000);
        with_ccli.ccli_song_id = Some("7059628".into());
        repo.record(&with_ccli).await.unwrap();

        let rows = repo.list_between(0, 100_000).await.unwrap();
        assert_eq!(rows[0].ccli_song_id.as_deref(), Some("7059628"));
    }

    /// Loggen er en protokoll over hva som skjedde: den overlever at sangen
    /// slettes fra biblioteket. Ingen fremmednøkkel, ingen kaskade.
    #[tokio::test]
    async fn loggraden_overlever_at_sangen_slettes() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap();

        // Hardt slett en rad med samme id fra sangtabellen (den finnes ikke der,
        // men poenget er at loggen ikke er koblet til den i det hele tatt).
        sqlx::query("DELETE FROM song WHERE id = ?1")
            .bind("s1")
            .execute(&db.pool)
            .await
            .unwrap();

        let rows = repo.list_between(0, 100_000).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Sang s1", "tittelen er kopiert inn");
    }

    #[tokio::test]
    async fn prune_kaster_bare_det_gamle() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("gammel", "2024-01-07", 1_000))
            .await
            .unwrap();
        repo.record(&entry("ny", "2026-01-04", 900_000))
            .await
            .unwrap();

        assert_eq!(repo.prune_before(500_000).await.unwrap(), 1);
        let rows = repo.list_between(0, i64::MAX).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].song_id, "ny");
    }

    #[tokio::test]
    async fn clear_tommer_hele_loggen() {
        let db = db().await;
        let repo = SongUsageRepo::new(&db.pool);
        repo.record(&entry("s1", "2026-01-04", 1_000))
            .await
            .unwrap();
        repo.record(&entry("s2", "2026-01-04", 2_000))
            .await
            .unwrap();
        assert_eq!(repo.clear().await.unwrap(), 2);
        assert_eq!(repo.count().await.unwrap(), 0);
    }
}
