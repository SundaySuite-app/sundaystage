//! Phase 4 — AI service layer.
//!
//! The foundation for every AI feature. Mirrors the sibling Verbatim app's
//! `services/llm`: the model register, cost estimate, request-body builder and
//! response parser are **pure and unit-tested**; the actual network call lives
//! behind the optional `ai` cargo feature (reqwest + rustls) so the default
//! build compiles with no HTTP stack, no API key, and no network — AI features
//! then fall back to the local heuristic formatter.
//!
//! Build the real client with `cargo build --features ai`.
//!
//! Key handling: the provider takes the API key as a value (BYOK). Reading it
//! from the system keychain (keyring crate) + a consent dialog + a settings
//! screen are follow-ups; for now the command resolves a caller-supplied key or
//! the `ANTHROPIC_API_KEY` env var.

pub mod lyric_format;
pub mod plan;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

/// A Claude model the app can call, with pricing for cost estimates.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/ClaudeModel.ts")]
pub struct ClaudeModel {
    /// API id, e.g. `claude-sonnet-4-6`.
    pub id: String,
    pub display: String,
    /// USD per million input tokens.
    pub input_price_per_mtok: f64,
    /// USD per million output tokens.
    pub output_price_per_mtok: f64,
    /// Heavier/pricier model suited to deep tasks (theme gen, planning).
    pub heavy: bool,
}

/// The models SundayStage offers. Sonnet is the default workhorse; Opus is for
/// heavier reasoning. Prices are list prices at time of writing (see
/// `docs/DECISIONS.md` if they drift).
pub fn claude_models() -> Vec<ClaudeModel> {
    vec![
        ClaudeModel {
            id: "claude-sonnet-4-6".into(),
            display: "Claude Sonnet 4.6".into(),
            input_price_per_mtok: 3.0,
            output_price_per_mtok: 15.0,
            heavy: false,
        },
        ClaudeModel {
            id: "claude-opus-4-7".into(),
            display: "Claude Opus 4.7".into(),
            input_price_per_mtok: 15.0,
            output_price_per_mtok: 75.0,
            heavy: true,
        },
        ClaudeModel {
            id: "claude-haiku-4-5-20251001".into(),
            display: "Claude Haiku 4.5".into(),
            input_price_per_mtok: 1.0,
            output_price_per_mtok: 5.0,
            heavy: false,
        },
    ]
}

/// Default model id for routine tasks like lyric formatting.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Estimate the USD cost of a call. Pure — used for the cost-preview UI.
pub fn estimate_cost(model_id: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let model = claude_models()
        .into_iter()
        .find(|m| m.id == model_id)
        .unwrap_or_else(|| claude_models().into_iter().next().unwrap());
    (input_tokens as f64 / 1_000_000.0) * model.input_price_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * model.output_price_per_mtok
}

/// Why an AI call is being made — tagged for per-purpose cost analytics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/AiPurpose.ts")]
pub enum AiPurpose {
    LyricFormat,
    ThemeGenerate,
    BibleSearch,
    ServicePlan,
}

/// A structured-output request: a system prompt, the user content, and a tool
/// schema Claude must call so the result is machine-parseable.
pub struct StructuredRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub tool_name: String,
    pub tool_schema: serde_json::Value,
    pub max_tokens: u32,
    pub purpose: AiPurpose,
}

/// Build the Anthropic `/v1/messages` request body for a forced tool call.
/// Pure — tested without any network. `tool_choice` forces the named tool so
/// the model returns structured input rather than prose.
pub fn build_messages_body(req: &StructuredRequest) -> serde_json::Value {
    serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "system": req.system,
        "tools": [{
            "name": req.tool_name,
            "description": "Return the structured result.",
            "input_schema": req.tool_schema,
        }],
        "tool_choice": { "type": "tool", "name": req.tool_name },
        "messages": [{
            "role": "user",
            "content": req.user,
        }],
    })
}

/// Extract the forced tool call's `input` object from an Anthropic Messages
/// response. Pure — tested against captured response shapes.
pub fn extract_tool_input(
    resp: &serde_json::Value,
    tool_name: &str,
) -> AppResult<serde_json::Value> {
    let content = resp
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| AppError::Internal("AI-svar mangler 'content'".into()))?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
            && block.get("name").and_then(|n| n.as_str()) == Some(tool_name)
        {
            return block
                .get("input")
                .cloned()
                .ok_or_else(|| AppError::Internal("tool_use-blokk mangler 'input'".into()));
        }
    }
    Err(AppError::Internal(format!(
        "AI-svar inneholdt ingen tool_use for '{}'",
        tool_name
    )))
}

/// Abstraction over an AI backend. Today only Anthropic; an Ollama offline
/// provider can implement the same trait later.
#[async_trait::async_trait]
pub trait AiProvider {
    /// Run a forced-tool-call request and return the tool input JSON.
    async fn complete_structured(&self, req: StructuredRequest) -> AppResult<serde_json::Value>;
}

/// Anthropic Claude provider (BYOK). The transport is compiled only under the
/// `ai` feature; without it, constructing the provider still works but calling
/// it returns a clear "not compiled in" error so callers can fall back.
pub struct AnthropicProvider {
    #[allow(dead_code)]
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

#[cfg(feature = "ai")]
#[async_trait::async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete_structured(&self, req: StructuredRequest) -> AppResult<serde_json::Value> {
        let body = build_messages_body(&req);
        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("AI-forespørsel feilet: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!("AI-API {status}: {text}")));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Kunne ikke lese AI-svar: {e}")))?;
        extract_tool_input(&json, &req.tool_name)
    }
}

#[cfg(not(feature = "ai"))]
#[async_trait::async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete_structured(&self, _req: StructuredRequest) -> AppResult<serde_json::Value> {
        Err(AppError::Internal(
            "AI-klienten er ikke kompilert inn — bygg med `--features ai`".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_in_register() {
        assert!(claude_models().iter().any(|m| m.id == DEFAULT_MODEL));
    }

    #[test]
    fn cost_estimate_scales_with_tokens_and_price() {
        // Sonnet: $3/Mtok in, $15/Mtok out. 1M in + 1M out = $18.
        let c = estimate_cost("claude-sonnet-4-6", 1_000_000, 1_000_000);
        assert!((c - 18.0).abs() < 1e-9, "got {c}");
        // Zero tokens → zero cost.
        assert_eq!(estimate_cost("claude-sonnet-4-6", 0, 0), 0.0);
    }

    #[test]
    fn cost_estimate_unknown_model_falls_back_to_first() {
        let c = estimate_cost("does-not-exist", 1_000_000, 0);
        // First model is Sonnet @ $3/Mtok input.
        assert!((c - 3.0).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn messages_body_forces_the_named_tool() {
        let req = StructuredRequest {
            model: "claude-sonnet-4-6".into(),
            system: "sys".into(),
            user: "hello".into(),
            tool_name: "emit".into(),
            tool_schema: serde_json::json!({ "type": "object" }),
            max_tokens: 1024,
            purpose: AiPurpose::LyricFormat,
        };
        let body = build_messages_body(&req);
        assert_eq!(body["model"], "claude-sonnet-4-6");
        assert_eq!(body["tool_choice"]["type"], "tool");
        assert_eq!(body["tool_choice"]["name"], "emit");
        assert_eq!(body["tools"][0]["name"], "emit");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn extract_tool_input_pulls_the_matching_block() {
        let resp = serde_json::json!({
            "content": [
                { "type": "text", "text": "thinking…" },
                { "type": "tool_use", "name": "emit", "input": { "ok": true } }
            ]
        });
        let input = extract_tool_input(&resp, "emit").unwrap();
        assert_eq!(input["ok"], true);
    }

    #[test]
    fn extract_tool_input_errors_when_absent() {
        let resp = serde_json::json!({ "content": [{ "type": "text", "text": "no tool" }] });
        assert_eq!(
            extract_tool_input(&resp, "emit").unwrap_err().code(),
            "internal"
        );
    }
}
