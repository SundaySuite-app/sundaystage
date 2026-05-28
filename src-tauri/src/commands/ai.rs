//! Tauri commands for AI features (Phase 4).
//!
//! `ai_format_lyrics` degrades gracefully: it uses Claude when a key is
//! available and the `ai` feature is compiled in, and otherwise falls back to
//! the local heuristic formatter — always returning a usable result with a
//! warning explaining what happened, never a hard error.

use tauri::State;

use crate::db::models::SongArrangement;
use crate::error::AppResult;
use crate::services::ai::lyric_format::{
    apply_formatted_song, heuristic_format, parse_format_response, system_prompt, tool_schema,
    FormattedSong, TOOL_NAME,
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
        f.warnings.push("Ingen API-nøkkel — formaterte lokalt uten AI.".into());
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
                f.warnings
                    .push(format!("AI-svar kunne ikke tolkes ({e}) — formaterte lokalt."));
                Ok(f)
            }
        },
        Err(e) => {
            let mut f = heuristic_format(&raw);
            f.warnings.push(format!("AI utilgjengelig ({e}) — formaterte lokalt."));
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
