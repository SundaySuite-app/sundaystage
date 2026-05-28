//! Tauri commands for AI features (Phase 4).
//!
//! `ai_format_lyrics` degrades gracefully: it uses Claude when a key is
//! available and the `ai` feature is compiled in, and otherwise falls back to
//! the local heuristic formatter — always returning a usable result with a
//! warning explaining what happened, never a hard error.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

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
use crate::services::ai::translate::{
    is_supported_target, parse_translation, system_prompt as tr_system_prompt,
    tool_schema as tr_tool_schema, user_content as tr_user_content, TranslationResult,
    TRANSLATE_TOOL_NAME,
};
use crate::services::ai::{
    claude_models, keystore, AiProvider, AiPurpose, AnthropicProvider, ClaudeModel,
    StructuredRequest, DEFAULT_MODEL,
};
use crate::AppState;

#[tauri::command]
pub fn ai_models() -> Vec<ClaudeModel> {
    claude_models()
}

/// Where (if anywhere) an Anthropic key is available.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/AiKeyStatus.ts")]
pub struct AiKeyStatus {
    /// A key is stored in the OS keychain.
    pub stored: bool,
    /// The `ANTHROPIC_API_KEY` env var is set.
    pub env: bool,
}

/// Result of a "test connection" probe.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/AiTestResult.ts")]
pub struct AiTestResult {
    pub ok: bool,
    pub message: String,
}

/// Store the Anthropic API key in the OS keychain.
#[tauri::command]
pub fn ai_key_set(key: String) -> AppResult<()> {
    if key.trim().is_empty() {
        return Err(AppError::Validation("Tom nøkkel.".into()));
    }
    keystore::set_key(key.trim())
}

/// Remove the stored key.
#[tauri::command]
pub fn ai_key_clear() -> AppResult<()> {
    keystore::clear_key()
}

/// Whether a key is available (keychain and/or env).
#[tauri::command]
pub fn ai_key_status() -> AiKeyStatus {
    AiKeyStatus {
        stored: keystore::has_key(),
        env: keystore::has_env_key(),
    }
}

/// Probe the Anthropic API with a tiny forced-tool call to confirm the key +
/// network work. Returns a friendly result rather than erroring.
#[tauri::command]
pub async fn ai_test_connection(model: Option<String>) -> AiTestResult {
    let Some(key) = keystore::resolve(None) else {
        return AiTestResult {
            ok: false,
            message: "Ingen API-nøkkel lagret.".into(),
        };
    };
    let provider = AnthropicProvider::new(key);
    let req = StructuredRequest {
        model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        system: "You are a connectivity probe.".into(),
        user: "Call the tool with ok=true.".into(),
        tool_name: "probe".into(),
        tool_schema: serde_json::json!({
            "type": "object",
            "properties": { "ok": { "type": "boolean" } },
            "required": ["ok"]
        }),
        max_tokens: 64,
        purpose: AiPurpose::LyricFormat,
    };
    match provider.complete_structured(req).await {
        Ok(_) => AiTestResult {
            ok: true,
            message: "Tilkobling OK.".into(),
        },
        Err(e) => AiTestResult {
            ok: false,
            message: e.to_string(),
        },
    }
}

#[tauri::command]
pub async fn ai_format_lyrics(
    raw: String,
    api_key: Option<String>,
    model: Option<String>,
) -> AppResult<FormattedSong> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let Some(key) = keystore::resolve(api_key) else {
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
    let key = keystore::resolve(api_key).ok_or_else(|| {
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

/// Translate a block of lyric lines to a target language (Phase 11.2). Needs a
/// key — no offline fallback. Returns one translated line per source line.
#[tauri::command]
pub async fn ai_translate(
    lines: Vec<String>,
    target: String,
    api_key: Option<String>,
    model: Option<String>,
) -> AppResult<TranslationResult> {
    if !is_supported_target(&target) {
        return Err(AppError::Validation(format!(
            "Språk '{target}' støttes ikke for oversettelse."
        )));
    }
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let key = keystore::resolve(api_key).ok_or_else(|| {
        AppError::Validation("Oversettelse krever en Anthropic API-nøkkel.".into())
    })?;

    let provider = AnthropicProvider::new(key);
    let req = StructuredRequest {
        model,
        system: tr_system_prompt(&target),
        user: tr_user_content(&lines),
        tool_name: TRANSLATE_TOOL_NAME.to_string(),
        tool_schema: tr_tool_schema(),
        max_tokens: 2048,
        purpose: AiPurpose::LyricFormat,
    };
    let input = provider.complete_structured(req).await?;
    parse_translation(&input, &lines, &target)
}

#[tauri::command]
pub async fn ai_apply_plan(
    state: State<'_, AppState>,
    library_id: String,
    plan: ServicePlan,
) -> AppResult<Service> {
    apply_plan(&state.db.pool, &library_id, &plan).await
}
