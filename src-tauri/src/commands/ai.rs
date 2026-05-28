//! Tauri commands for AI features (Phase 4).
//!
//! `ai_format_lyrics` degrades gracefully: it uses Claude when a key is
//! available and the `ai` feature is compiled in, and otherwise falls back to
//! the local heuristic formatter — always returning a usable result with a
//! warning explaining what happened, never a hard error.

use std::collections::HashSet;

use tauri::State;

use crate::db::models::{Service, SongArrangement};
use crate::db::repositories::SongRepo;
use crate::error::{AppError, AppResult};
use crate::services::ai::lyric_format::{
    apply_formatted_song, heuristic_format, parse_format_response, system_prompt, tool_schema,
    FormattedSong, TOOL_NAME,
};
use crate::services::ai::plan::{
    apply_plan, parse_plan_response, system_prompt as plan_system_prompt,
    tool_schema as plan_tool_schema, LibrarySong, ServicePlan, PLAN_TOOL_NAME,
};
use crate::services::ai::{
    claude_models, AiProvider, AiPurpose, AnthropicProvider, ClaudeModel, StructuredRequest,
    DEFAULT_MODEL,
};
use crate::AppState;

#[tauri::command]
pub fn ai_models() -> Vec<ClaudeModel> {
    claude_models()
}

#[tauri::command]
pub async fn ai_format_lyrics(
    raw: String,
    api_key: Option<String>,
    model: Option<String>,
) -> AppResult<FormattedSong> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let key = api_key
        .filter(|k| !k.trim().is_empty())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    let Some(key) = key else {
        let mut f = heuristic_format(&raw);
        f.warnings
            .push("Ingen API-nøkkel — formaterte lokalt uten AI.".into());
        return Ok(f);
    };

    let provider = AnthropicProvider::new(key);
    let req = StructuredRequest {
        model,
        system: system_prompt(),
        user: raw.clone(),
        tool_name: TOOL_NAME.to_string(),
        tool_schema: tool_schema(),
        max_tokens: 2048,
        purpose: AiPurpose::LyricFormat,
    };

    match provider.complete_structured(req).await {
        Ok(input) => match parse_format_response(&input) {
            Ok(f) => Ok(f),
            Err(e) => {
                let mut f = heuristic_format(&raw);
                f.warnings.push(format!(
                    "AI-svar kunne ikke tolkes ({e}) — formaterte lokalt."
                ));
                Ok(f)
            }
        },
        Err(e) => {
            let mut f = heuristic_format(&raw);
            f.warnings
                .push(format!("AI utilgjengelig ({e}) — formaterte lokalt."));
            Ok(f)
        }
    }
}

#[tauri::command]
pub async fn ai_apply_format(
    state: State<'_, AppState>,
    song_id: String,
    formatted: FormattedSong,
) -> AppResult<SongArrangement> {
    apply_formatted_song(&state.db.pool, &song_id, &formatted).await
}

/// Propose a service plan from a free-text brief (Phase 11.2). Planning needs
/// the LLM — there's no offline fallback, so a missing key is a clear error.
#[tauri::command]
pub async fn ai_plan_service(
    state: State<'_, AppState>,
    library_id: String,
    prompt: String,
    api_key: Option<String>,
    model: Option<String>,
) -> AppResult<ServicePlan> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let key = api_key
        .filter(|k| !k.trim().is_empty())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| {
            AppError::Validation("Tjenesteplanlegging krever en Anthropic API-nøkkel.".into())
        })?;

    let songs = SongRepo::new(&state.db.pool)
        .list(&library_id, 300, 0)
        .await?;
    let valid: HashSet<String> = songs.iter().map(|s| s.id.clone()).collect();
    let lib_songs: Vec<LibrarySong> = songs
        .into_iter()
        .map(|s| LibrarySong {
            id: s.id,
            title: s.title,
            key: s.default_key,
        })
        .collect();

    let provider = AnthropicProvider::new(key);
    let req = StructuredRequest {
        model,
        system: plan_system_prompt(&lib_songs),
        user: prompt,
        tool_name: PLAN_TOOL_NAME.to_string(),
        tool_schema: plan_tool_schema(),
        max_tokens: 2048,
        purpose: AiPurpose::ServicePlan,
    };
    let input = provider.complete_structured(req).await?;
    Ok(parse_plan_response(&input, &valid))
}

#[tauri::command]
pub async fn ai_apply_plan(
    state: State<'_, AppState>,
    library_id: String,
    plan: ServicePlan,
) -> AppResult<Service> {
    apply_plan(&state.db.pool, &library_id, &plan).await
}
