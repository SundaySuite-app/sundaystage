//! Repository for service templates (Phase — service template system).
//!
//! Templates are reusable ordered lists of `CueSpec`s stored as JSON.
//! The three built-in templates are seeded here idempotently.

use sqlx::SqlitePool;

use crate::db::models::{CueSpec, ServiceTemplate, ServiceTemplateInput};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct ServiceTemplateRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ServiceTemplateRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ── CRUD ──────────────────────────────────────────────────────────────────

    pub async fn create(&self, input: ServiceTemplateInput) -> AppResult<ServiceTemplate> {
        let id = new_id();
        let now = now_ms();
        let specs_json = serde_json::to_string(&input.cue_specs).map_err(AppError::Json)?;

        sqlx::query(
            r#"
            INSERT INTO service_template
                (id, name, description, cue_specs, is_builtin, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)
            "#,
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&specs_json)
        .bind(now)
        .execute(self.pool)
        .await?;

        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<ServiceTemplate> {
        sqlx::query_as::<_, ServiceTemplate>("SELECT * FROM service_template WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "service_template",
                id: id.to_string(),
            })
    }

    pub async fn list(&self) -> AppResult<Vec<ServiceTemplate>> {
        let rows = sqlx::query_as::<_, ServiceTemplate>(
            "SELECT * FROM service_template ORDER BY is_builtin DESC, created_at ASC",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let affected = sqlx::query("DELETE FROM service_template WHERE id = ?1 AND is_builtin = 0")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();

        if affected == 0 {
            // Could be not found or is_builtin — distinguish.
            let exists: Option<(i64,)> =
                sqlx::query_as("SELECT is_builtin FROM service_template WHERE id = ?1")
                    .bind(id)
                    .fetch_optional(self.pool)
                    .await?;
            match exists {
                None => Err(AppError::NotFound {
                    entity: "service_template",
                    id: id.to_string(),
                }),
                Some((1,)) => Err(AppError::Validation(
                    "Built-in templates cannot be deleted".into(),
                )),
                _ => Err(AppError::NotFound {
                    entity: "service_template",
                    id: id.to_string(),
                }),
            }
        } else {
            Ok(())
        }
    }

    /// Decode the `cue_specs` JSON from a template row.
    pub fn parse_specs(template: &ServiceTemplate) -> AppResult<Vec<CueSpec>> {
        serde_json::from_str(&template.cue_specs).map_err(AppError::Json)
    }

    // ── Built-in seed ─────────────────────────────────────────────────────────

    /// Ensure the three built-in templates exist. Idempotent — safe to call on
    /// every startup.
    pub async fn seed_builtins(&self) -> AppResult<()> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM service_template WHERE is_builtin = 1")
                .fetch_one(self.pool)
                .await?;
        if count.0 >= 3 {
            return Ok(()); // already seeded
        }

        // Truncate any partial seed and redo.
        sqlx::query("DELETE FROM service_template WHERE is_builtin = 1")
            .execute(self.pool)
            .await?;

        self.insert_builtin(
            "Standardgudstjeneste",
            Some("Standard søndagsgudstjeneste med sang, bibel, preken og nattverd"),
            standard_service_specs(),
        )
        .await?;

        self.insert_builtin(
            "Barnegudstjeneste",
            Some("Kort gudstjeneste tilpasset barn"),
            children_service_specs(),
        )
        .await?;

        self.insert_builtin(
            "Konsert",
            Some("Musikkonsert med intro, sang og avslutning"),
            concert_specs(),
        )
        .await?;

        Ok(())
    }

    async fn insert_builtin(
        &self,
        name: &str,
        description: Option<&str>,
        specs: Vec<CueSpec>,
    ) -> AppResult<()> {
        let id = new_id();
        let now = now_ms();
        let specs_json = serde_json::to_string(&specs).map_err(AppError::Json)?;

        sqlx::query(
            r#"
            INSERT INTO service_template
                (id, name, description, cue_specs, is_builtin, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(&specs_json)
        .bind(now)
        .execute(self.pool)
        .await?;

        Ok(())
    }
}

// ── Built-in template definitions ─────────────────────────────────────────────

fn spec(kind: &str, label: &str) -> CueSpec {
    CueSpec {
        kind: kind.into(),
        label: label.into(),
        notes: None,
    }
}

fn spec_with_notes(kind: &str, label: &str, notes: &str) -> CueSpec {
    CueSpec {
        kind: kind.into(),
        label: label.into(),
        notes: Some(notes.into()),
    }
}

/// 15-item standard Sunday service.
fn standard_service_specs() -> Vec<CueSpec> {
    vec![
        spec("announcement", "Velkomst"),
        spec("song", "Lovsang 1"),
        spec("song", "Lovsang 2"),
        spec("prayer", "Åpningsbønn"),
        spec("song", "Lovsang 3"),
        spec("bible", "Bibeltekst"),
        spec("song", "Lovsang 4"),
        spec("announcement", "Preken"),
        spec("prayer", "Forbønn"),
        spec("announcement", "Nattverd — innledning"),
        spec("song", "Nattverdsang"),
        spec("prayer", "Nattverdbønn"),
        spec("song", "Lovsang 5"),
        spec_with_notes("prayer", "Avslutningsbønn", "Velsignelse"),
        spec("song", "Utgangssang"),
    ]
}

/// 8-item children's service.
fn children_service_specs() -> Vec<CueSpec> {
    vec![
        spec("song", "Barnesang 1"),
        spec("prayer", "Bønn"),
        spec("bible", "Bibelfortelling"),
        spec("announcement", "Aktivitet"),
        spec("song", "Barnesang 2"),
        spec("prayer", "Takkebønn"),
        spec("song", "Avslutningssang"),
        spec("announcement", "Info til foreldre"),
    ]
}

/// 12-item concert.
fn concert_specs() -> Vec<CueSpec> {
    vec![
        spec("announcement", "Intro"),
        spec("song", "Sang 1"),
        spec("song", "Sang 2"),
        spec("song", "Sang 3"),
        spec("song", "Sang 4"),
        spec("song", "Sang 5"),
        spec("announcement", "Pause"),
        spec("song", "Sang 6"),
        spec("song", "Sang 7"),
        spec("song", "Sang 8"),
        spec("song", "Sang 9"),
        spec("announcement", "Avslutning"),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn seed_builtins_creates_three_templates() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();

        let templates = repo.list().await.unwrap();
        assert_eq!(templates.len(), 3);
        assert!(templates.iter().all(|t| t.is_builtin == 1));
    }

    #[tokio::test]
    async fn seed_builtins_is_idempotent() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();
        repo.seed_builtins().await.unwrap(); // second call is a no-op
        let templates = repo.list().await.unwrap();
        assert_eq!(templates.len(), 3);
    }

    #[tokio::test]
    async fn standard_template_has_15_specs() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();

        let standard = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.name == "Standardgudstjeneste")
            .unwrap();
        let specs = ServiceTemplateRepo::parse_specs(&standard).unwrap();
        assert_eq!(specs.len(), 15);
    }

    #[tokio::test]
    async fn children_template_has_8_specs() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();

        let t = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.name == "Barnegudstjeneste")
            .unwrap();
        let specs = ServiceTemplateRepo::parse_specs(&t).unwrap();
        assert_eq!(specs.len(), 8);
    }

    #[tokio::test]
    async fn concert_template_has_12_specs() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();

        let t = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.name == "Konsert")
            .unwrap();
        let specs = ServiceTemplateRepo::parse_specs(&t).unwrap();
        assert_eq!(specs.len(), 12);
    }

    #[tokio::test]
    async fn create_user_template_and_get() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);

        let input = ServiceTemplateInput {
            name: "My Template".into(),
            description: Some("A test".into()),
            cue_specs: vec![
                CueSpec {
                    kind: "song".into(),
                    label: "Opener".into(),
                    notes: None,
                },
                CueSpec {
                    kind: "prayer".into(),
                    label: "Prayer".into(),
                    notes: Some("Short".into()),
                },
            ],
        };

        let tmpl = repo.create(input).await.unwrap();
        assert_eq!(tmpl.name, "My Template");
        assert_eq!(tmpl.is_builtin, 0);

        let fetched = repo.get(&tmpl.id).await.unwrap();
        assert_eq!(fetched.id, tmpl.id);
        let specs = ServiceTemplateRepo::parse_specs(&fetched).unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].kind, "song");
        assert_eq!(specs[1].notes.as_deref(), Some("Short"));
    }

    #[tokio::test]
    async fn delete_user_template() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);

        let tmpl = repo
            .create(ServiceTemplateInput {
                name: "Temp".into(),
                description: None,
                cue_specs: vec![],
            })
            .await
            .unwrap();

        repo.delete(&tmpl.id).await.unwrap();
        assert!(repo.get(&tmpl.id).await.is_err());
    }

    #[tokio::test]
    async fn cannot_delete_builtin_template() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        repo.seed_builtins().await.unwrap();

        let builtin = repo.list().await.unwrap().into_iter().next().unwrap();
        let result = repo.delete(&builtin.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_missing_template_errors() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceTemplateRepo::new(&db.pool);
        assert!(repo.delete("does-not-exist").await.is_err());
    }
}
