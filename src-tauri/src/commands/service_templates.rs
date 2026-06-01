//! Tauri commands for the service template system.
//!
//! Templates are reusable ordered cue-spec lists. Applying a template to an
//! existing service appends each spec as a `service_item` of the appropriate
//! kind; song slots land as announcement items so the operator can swap in
//! real songs later via the queue editor.

use tauri::State;

use crate::db::models::{ServiceItem, ServiceTemplate, ServiceTemplateInput};
use crate::db::repositories::{ServiceRepo, ServiceTemplateRepo};
use crate::error::{AppError, AppResult};
use crate::AppState;

// ── CRUD ──────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn svc_template_create(
    state: State<'_, AppState>,
    input: ServiceTemplateInput,
) -> AppResult<ServiceTemplate> {
    ServiceTemplateRepo::new(&state.db.pool).create(input).await
}

#[tauri::command]
pub async fn svc_template_list(state: State<'_, AppState>) -> AppResult<Vec<ServiceTemplate>> {
    ServiceTemplateRepo::new(&state.db.pool).list().await
}

#[tauri::command]
pub async fn svc_template_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    ServiceTemplateRepo::new(&state.db.pool).delete(&id).await
}

/// Apply a template to an existing service: appends one `service_item` per
/// cue spec. Each spec's label becomes the item's notes field (visible in the
/// queue editor), and the kind is mapped to the nearest allowed DB kind.
/// Returns the list of newly created items.
#[tauri::command]
pub async fn svc_template_apply(
    state: State<'_, AppState>,
    template_id: String,
    service_id: String,
) -> AppResult<Vec<ServiceItem>> {
    let pool = &state.db.pool;
    let tmpl_repo = ServiceTemplateRepo::new(pool);
    let svc_repo = ServiceRepo::new(pool);

    let template = tmpl_repo.get(&template_id).await?;
    let specs = ServiceTemplateRepo::parse_specs(&template)?;

    let mut added = Vec::with_capacity(specs.len());
    for spec in &specs {
        let item_kind = map_spec_kind(&spec.kind)?;
        let position = svc_repo.next_position(&service_id).await?;

        // Use spec.label (with optional planning notes appended) as the
        // item's display notes column — this is how non-song items get labels
        // in the queue editor.
        let notes_text = match spec.notes.as_deref() {
            Some(n) if !n.is_empty() => format!("{} ({})", spec.label, n),
            _ => spec.label.clone(),
        };

        let item = svc_repo
            .add_item(
                &service_id,
                position,
                item_kind,
                None,
                None,
                None,
                None,
                Some(notes_text.as_str()),
            )
            .await?;

        added.push(item);
    }

    Ok(added)
}

/// Map `CueSpec.kind` → `service_item.kind` (must satisfy the schema CHECK).
fn map_spec_kind(spec_kind: &str) -> AppResult<&'static str> {
    match spec_kind {
        "song" => Ok("announcement"), // song slot — operator fills it in later
        "bible" => Ok("gap"),
        "prayer" => Ok("gap"),
        "announcement" => Ok("announcement"),
        "media" => Ok("video"),
        other => Err(AppError::Validation(format!(
            "unknown CueSpec kind '{other}'"
        ))),
    }
}
