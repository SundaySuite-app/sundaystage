//! Arrangement repository — ordered, repeatable section sequences (Phase 3.3).
//!
//! A `SongArrangement` is an ordered list of references to a song's
//! `SongSection`s. The same section may appear many times (verse → chorus →
//! verse → chorus), so the sequence is just a list of `section_id`s with a
//! position. Slides are generated from the resolved sections at render time —
//! nothing is duplicated, so editing a section's lyrics updates every place it
//! appears for free.
//!
//! At most one arrangement per song is the default (enforced by the
//! `uniq_arrangement_default` partial unique index in migration 0001).

use sqlx::SqlitePool;

use crate::db::models::{ArrangementItem, SongArrangement, SongSection};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct ArrangementRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ArrangementRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Create an arrangement. The first arrangement for a song becomes the
    /// default automatically.
    pub async fn create(&self, song_id: &str, name: &str) -> AppResult<SongArrangement> {
        let id = new_id();
        let now = now_ms();
        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM song_arrangement WHERE song_id = ?1")
                .bind(song_id)
                .fetch_one(self.pool)
                .await?;
        let is_default = if existing == 0 { 1 } else { 0 };
        sqlx::query(
            r#"
            INSERT INTO song_arrangement (id, song_id, name, is_default, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
        )
        .bind(&id)
        .bind(song_id)
        .bind(name)
        .bind(is_default)
        .bind(now)
        .execute(self.pool)
        .await?;
        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<SongArrangement> {
        sqlx::query_as::<_, SongArrangement>("SELECT * FROM song_arrangement WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "song_arrangement",
                id: id.to_string(),
            })
    }

    pub async fn list(&self, song_id: &str) -> AppResult<Vec<SongArrangement>> {
        let rows = sqlx::query_as::<_, SongArrangement>(
            "SELECT * FROM song_arrangement WHERE song_id = ?1 ORDER BY is_default DESC, name",
        )
        .bind(song_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn rename(&self, id: &str, name: &str) -> AppResult<SongArrangement> {
        let now = now_ms();
        let res =
            sqlx::query("UPDATE song_arrangement SET name = ?1, updated_at = ?2 WHERE id = ?3")
                .bind(name)
                .bind(now)
                .bind(id)
                .execute(self.pool)
                .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "song_arrangement",
                id: id.to_string(),
            });
        }
        self.get(id).await
    }

    /// Delete an arrangement (its items cascade). If it was the default and
    /// other arrangements remain, the oldest survivor is promoted.
    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let arr = self.get(id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM song_arrangement WHERE id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if arr.is_default == 1 {
            sqlx::query(
                r#"
                UPDATE song_arrangement SET is_default = 1
                WHERE id = (
                    SELECT id FROM song_arrangement
                    WHERE song_id = ?1 ORDER BY created_at LIMIT 1
                )
                "#,
            )
            .bind(&arr.song_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Make `arrangement_id` the song's sole default.
    pub async fn set_default(&self, song_id: &str, arrangement_id: &str) -> AppResult<()> {
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE song_arrangement SET is_default = 0, updated_at = ?1 WHERE song_id = ?2",
        )
        .bind(now)
        .bind(song_id)
        .execute(&mut *tx)
        .await?;
        let res = sqlx::query(
            "UPDATE song_arrangement SET is_default = 1, updated_at = ?1 WHERE id = ?2 AND song_id = ?3",
        )
        .bind(now)
        .bind(arrangement_id)
        .bind(song_id)
        .execute(&mut *tx)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "song_arrangement",
                id: arrangement_id.to_string(),
            });
        }
        tx.commit().await?;
        Ok(())
    }

    /// Duplicate an arrangement (copies its item sequence). The copy is never
    /// the default.
    pub async fn duplicate(&self, id: &str) -> AppResult<SongArrangement> {
        let source = self.get(id).await?;
        let items = self.items(id).await?;
        let new = SongArrangement {
            id: new_id(),
            song_id: source.song_id.clone(),
            name: format!("{} (kopi)", source.name),
            is_default: 0,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO song_arrangement (id, song_id, name, is_default, created_at, updated_at)
            VALUES (?1, ?2, ?3, 0, ?4, ?4)
            "#,
        )
        .bind(&new.id)
        .bind(&new.song_id)
        .bind(&new.name)
        .bind(new.created_at)
        .execute(&mut *tx)
        .await?;
        for item in &items {
            sqlx::query(
                "INSERT INTO arrangement_item (arrangement_id, position, section_id) VALUES (?1, ?2, ?3)",
            )
            .bind(&new.id)
            .bind(item.position)
            .bind(&item.section_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.get(&new.id).await
    }

    pub async fn items(&self, arrangement_id: &str) -> AppResult<Vec<ArrangementItem>> {
        let rows = sqlx::query_as::<_, ArrangementItem>(
            "SELECT * FROM arrangement_item WHERE arrangement_id = ?1 ORDER BY position",
        )
        .bind(arrangement_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Replace the arrangement's whole sequence with `section_ids` (position =
    /// index; repeats allowed). Every id must belong to the arrangement's song.
    pub async fn set_items(
        &self,
        arrangement_id: &str,
        section_ids: &[String],
    ) -> AppResult<Vec<ArrangementItem>> {
        let arr = self.get(arrangement_id).await?;
        // Validate every section belongs to this song.
        for sid in section_ids {
            let belongs: Option<String> =
                sqlx::query_scalar("SELECT id FROM song_section WHERE id = ?1 AND song_id = ?2")
                    .bind(sid)
                    .bind(&arr.song_id)
                    .fetch_optional(self.pool)
                    .await?;
            if belongs.is_none() {
                return Err(AppError::Validation(format!(
                    "section {} does not belong to song {}",
                    sid, arr.song_id
                )));
            }
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM arrangement_item WHERE arrangement_id = ?1")
            .bind(arrangement_id)
            .execute(&mut *tx)
            .await?;
        for (idx, sid) in section_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO arrangement_item (arrangement_id, position, section_id) VALUES (?1, ?2, ?3)",
            )
            .bind(arrangement_id)
            .bind(idx as i64)
            .bind(sid)
            .execute(&mut *tx)
            .await?;
        }
        // Touch the arrangement's updated_at.
        sqlx::query("UPDATE song_arrangement SET updated_at = ?1 WHERE id = ?2")
            .bind(now_ms())
            .bind(arrangement_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.items(arrangement_id).await
    }

    /// Resolve the arrangement into its ordered sections (with repeats). This
    /// is what the cue compiler walks to generate slides.
    pub async fn resolved_sections(&self, arrangement_id: &str) -> AppResult<Vec<SongSection>> {
        let rows = sqlx::query_as::<_, SongSection>(
            r#"
            SELECT s.*
            FROM arrangement_item ai
            JOIN song_section s ON s.id = ai.section_id
            WHERE ai.arrangement_id = ?1
            ORDER BY ai.position
            "#,
        )
        .bind(arrangement_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{LibraryInput, SongInput};
    use crate::db::repositories::{LibraryRepo, SongRepo};
    use crate::db::Database;

    async fn fixture() -> (Database, String, Vec<String>) {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let songs = SongRepo::new(&db.pool);
        let song = songs
            .create(SongInput {
                library_id: lib.id.clone(),
                title: "Amazing Grace".into(),
                language: None,
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: None,
            })
            .await
            .unwrap();
        let v1 = songs
            .add_section(&song.id, "verse_1", "v1 line")
            .await
            .unwrap();
        let chorus = songs
            .add_section(&song.id, "chorus", "chorus line")
            .await
            .unwrap();
        (db, song.id, vec![v1.id, chorus.id])
    }

    #[tokio::test]
    async fn first_arrangement_is_default() {
        let (db, song_id, _s) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let a = repo.create(&song_id, "Full version").await.unwrap();
        assert_eq!(a.is_default, 1);
        let b = repo.create(&song_id, "Short version").await.unwrap();
        assert_eq!(b.is_default, 0);
    }

    #[tokio::test]
    async fn set_items_supports_repeats_and_resolves_in_order() {
        let (db, song_id, sections) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let arr = repo.create(&song_id, "Full").await.unwrap();
        let (v1, chorus) = (&sections[0], &sections[1]);
        // verse → chorus → verse → chorus
        repo.set_items(
            &arr.id,
            &[v1.clone(), chorus.clone(), v1.clone(), chorus.clone()],
        )
        .await
        .unwrap();
        let resolved = repo.resolved_sections(&arr.id).await.unwrap();
        assert_eq!(resolved.len(), 4);
        assert_eq!(resolved[0].label, "verse_1");
        assert_eq!(resolved[1].label, "chorus");
        assert_eq!(resolved[2].label, "verse_1");
        assert_eq!(resolved[3].label, "chorus");
    }

    #[tokio::test]
    async fn editing_section_reflects_in_every_arrangement_slot() {
        let (db, song_id, sections) = fixture().await;
        let arr_repo = ArrangementRepo::new(&db.pool);
        let song_repo = SongRepo::new(&db.pool);
        let arr = arr_repo.create(&song_id, "Full").await.unwrap();
        let chorus = &sections[1];
        arr_repo
            .set_items(&arr.id, &[chorus.clone(), chorus.clone()])
            .await
            .unwrap();
        // Edit the chorus once → both slots reflect it.
        song_repo
            .update_section(chorus, "chorus", "EDITED chorus")
            .await
            .unwrap();
        let resolved = arr_repo.resolved_sections(&arr.id).await.unwrap();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().all(|s| s.lyrics == "EDITED chorus"));
    }

    #[tokio::test]
    async fn set_items_rejects_section_from_another_song() {
        let (db, song_id, sections) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let arr = repo.create(&song_id, "Full").await.unwrap();
        let err = repo
            .set_items(&arr.id, &[sections[0].clone(), "ghost-section".to_string()])
            .await
            .unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[tokio::test]
    async fn set_default_moves_the_flag() {
        let (db, song_id, _s) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let a = repo.create(&song_id, "A").await.unwrap();
        let b = repo.create(&song_id, "B").await.unwrap();
        repo.set_default(&song_id, &b.id).await.unwrap();
        let list = repo.list(&song_id).await.unwrap();
        let a2 = list.iter().find(|x| x.id == a.id).unwrap();
        let b2 = list.iter().find(|x| x.id == b.id).unwrap();
        assert_eq!(a2.is_default, 0);
        assert_eq!(b2.is_default, 1);
    }

    #[tokio::test]
    async fn duplicate_copies_item_sequence_but_not_default() {
        let (db, song_id, sections) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let a = repo.create(&song_id, "A").await.unwrap();
        repo.set_items(&a.id, &[sections[0].clone(), sections[1].clone()])
            .await
            .unwrap();
        let copy = repo.duplicate(&a.id).await.unwrap();
        assert_eq!(copy.is_default, 0);
        assert!(copy.name.contains("kopi"));
        let items = repo.items(&copy.id).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].section_id, sections[0]);
        assert_eq!(items[1].section_id, sections[1]);
    }

    #[tokio::test]
    async fn deleting_default_promotes_another() {
        let (db, song_id, _s) = fixture().await;
        let repo = ArrangementRepo::new(&db.pool);
        let a = repo.create(&song_id, "A").await.unwrap(); // default
        let b = repo.create(&song_id, "B").await.unwrap();
        repo.delete(&a.id).await.unwrap();
        let b2 = repo.get(&b.id).await.unwrap();
        assert_eq!(b2.is_default, 1, "a survivor is promoted to default");
    }
}
