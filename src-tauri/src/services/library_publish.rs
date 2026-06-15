//! One-way library publish (desktop → cloud). **NETWORK-UNVERIFIED.**
//!
//! Folds the local song library into sundaystage-web's denormalised SlideDef
//! shape and POSTs it to `/api/library/publish`, authenticated with a fresh
//! Sunday access token minted from the SHARED session. The browser login lives
//! in SundayRec; this app only READS that session and REFRESHES it (Supabase
//! rotates the refresh token on every refresh, so we must persist the rotated
//! one or the whole suite's session would die). The target church is taken from
//! the cached claims, never chosen here. Web re-validates everything from the
//! JWKS-verified token.
//!
//! The fold (`fold_song`) is pure + unit-tested; the mint + POST are not
//! exercisable without network + a live Sunday account.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sunday_auth::{session, supabase};

use crate::db::models::{Song, SongSection};
use crate::db::now_ms;
use crate::db::repositories::SongRepo;
use crate::error::{AppError, AppResult};

/// Default web origin the desktop publishes to (override with SUNDAY_STAGE_WEB_URL).
const DEFAULT_WEB_URL: &str = "https://stage.sundaysuite.app";
/// Offline-grace for cached claims (matches SundayRec's account.rs).
const CLAIMS_GRACE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// One slide-shaped section (matches the web operator's SlideDef + PublishSong).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PublishSection {
    pub label: Option<String>,
    pub lines: Vec<String>,
}

/// A song as `/api/library/publish` expects it (denormalised, SlideDef-shaped).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PublishSong {
    pub source_song_id: String,
    pub title: String,
    pub sections: Vec<PublishSection>,
    pub ccli_song_id: Option<String>,
    pub tono_work_id: Option<String>,
    pub copyright_notice: Option<String>,
    pub language: String,
    pub default_key: Option<String>,
    pub source_updated_at: i64,
}

#[derive(Debug, Serialize)]
struct PublishPayload {
    church_id: String,
    songs: Vec<PublishSong>,
    deleted: Vec<String>,
}

/// What the renderer gets back from a publish.
#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    pub upserted: i64,
    pub deleted: i64,
    #[serde(rename = "churchId")]
    pub church_id: String,
    #[serde(rename = "songCount")]
    pub song_count: i64,
}

#[derive(Deserialize)]
struct UpsertResponse {
    upserted: i64,
    deleted: i64,
}

/// Fold a stored song + its sections into the web publish shape. PURE.
/// Lyrics split on newlines (trimmed, blanks dropped); a blank label becomes
/// `None`; a section that ends up with no lines is dropped entirely.
pub fn fold_song(song: &Song, sections: &[SongSection]) -> PublishSong {
    let folded = sections
        .iter()
        .map(|s| PublishSection {
            label: {
                let l = s.label.trim();
                if l.is_empty() {
                    None
                } else {
                    Some(l.to_string())
                }
            },
            lines: s
                .lyrics
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
        })
        .filter(|s| !s.lines.is_empty())
        .collect();
    PublishSong {
        source_song_id: song.id.clone(),
        title: song.title.clone(),
        sections: folded,
        ccli_song_id: song.ccli_song_id.clone(),
        tono_work_id: song.tono_work_id.clone(),
        copyright_notice: song.copyright_notice.clone(),
        language: song.language.clone(),
        default_key: song.default_key.clone(),
        source_updated_at: song.updated_at,
    }
}

// ── Impure: auth + transport (NETWORK-UNVERIFIED) ────────────────────────────

struct SupabaseConfig {
    base_url: String,
    anon_key: String,
}

impl SupabaseConfig {
    /// Resolve from runtime env → build-time env → the prod project baked into
    /// `sunday-auth`, so a stock build works with zero config (SundayRec parity).
    fn resolve() -> Option<Self> {
        let base_url = std::env::var("SUNDAY_SUPABASE_URL")
            .ok()
            .or_else(|| option_env!("SUNDAY_SUPABASE_URL").map(str::to_string))
            .unwrap_or_else(|| sunday_auth::SUNDAY_PROD_SUPABASE_URL.to_string())
            .trim()
            .to_string();
        let anon_key = std::env::var("SUNDAY_SUPABASE_ANON_KEY")
            .ok()
            .or_else(|| option_env!("SUNDAY_SUPABASE_ANON_KEY").map(str::to_string))
            .unwrap_or_else(|| sunday_auth::SUNDAY_PROD_SUPABASE_ANON_KEY.to_string())
            .trim()
            .to_string();
        if base_url.is_empty() || anon_key.is_empty() {
            return None;
        }
        Some(Self { base_url, anon_key })
    }

    fn issuer(&self) -> String {
        format!("{}/auth/v1", self.base_url.trim_end_matches('/'))
    }
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("http client: {e}")))
}

fn web_base_url() -> String {
    std::env::var("SUNDAY_STAGE_WEB_URL")
        .ok()
        .or_else(|| option_env!("SUNDAY_STAGE_WEB_URL").map(str::to_string))
        .map(|s| s.trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_WEB_URL.to_string())
}

/// POST a JSON body to a GoTrue endpoint with the required `apikey` header.
async fn send_json(
    config: &SupabaseConfig,
    url: &str,
    body: String,
) -> AppResult<(reqwest::StatusCode, String)> {
    let resp = http_client()?
        .post(url)
        .header("content-type", "application/json")
        .header("apikey", &config.anon_key)
        .header("authorization", format!("Bearer {}", config.anon_key))
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("auth request: {e}")))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("auth response body: {e}")))?;
    Ok((status, text))
}

/// Persist a freshly-refreshed Supabase session to the shared file (the rotated
/// refresh token MUST be saved or the whole suite's session dies). Atomic.
fn persist_session(config: &SupabaseConfig, s: &supabase::SupabaseSession) -> AppResult<()> {
    let cached_claims = session::decode_claims_unverified(&s.access_token).unwrap_or_default();
    let data = session::SessionData {
        schema_version: session::SESSION_SCHEMA_VERSION,
        refresh_token: s.refresh_token.clone(),
        cached_claims,
        claims_expires_at_ms: now_ms() + CLAIMS_GRACE_MS,
        issuer: config.issuer(),
    };
    let path =
        session::default_path().ok_or_else(|| AppError::Internal("sesjons-katalog".into()))?;
    session::write_atomic(&path, &data).map_err(|e| AppError::Internal(e.to_string()))
}

/// Mint a fresh access token by refreshing the shared session. On a dead refresh
/// token returns `reauth_required` (re-login happens in SundayRec) WITHOUT
/// clearing the shared session — this app only reads/refreshes, never logs out.
async fn access_token(config: &SupabaseConfig) -> AppResult<String> {
    let path =
        session::default_path().ok_or_else(|| AppError::Internal("sesjons-katalog".into()))?;
    let current = session::read(&path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Validation("not_signed_in".into()))?;

    let url = supabase::token_endpoint(&config.base_url, "refresh_token");
    let body = supabase::build_refresh_body(&current.refresh_token);
    let (status, text) = send_json(config, &url, body).await?;
    if !status.is_success() {
        return match supabase::classify_refresh_error(&text) {
            supabase::RefreshOutcome::Reauth => Err(AppError::Validation("reauth_required".into())),
            supabase::RefreshOutcome::Retry => Err(AppError::Internal(format!(
                "token refresh returned HTTP {} (transient)",
                status.as_u16()
            ))),
        };
    }
    let refreshed = supabase::parse_session(&text, now_ms())
        .map_err(|e| AppError::Internal(format!("refresh response: {e}")))?;
    persist_session(config, &refreshed)?;
    Ok(refreshed.access_token)
}

/// Publish every (non-deleted) song in `library_id` to the cloud, scoped to the
/// signed-in user's first church. Requires a Sunday login (via SundayRec).
pub async fn publish_library(pool: &SqlitePool, library_id: &str) -> AppResult<PublishResult> {
    // 1. Church from the shared session's cached claims.
    let path =
        session::default_path().ok_or_else(|| AppError::Internal("sesjons-katalog".into()))?;
    let sess = session::read(&path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Validation("not_signed_in".into()))?;
    let church_id = sess
        .cached_claims
        .church_ids
        .first()
        .cloned()
        .ok_or_else(|| AppError::Validation("no_church".into()))?;

    // 2. Fold the local library into the publish payload.
    let repo = SongRepo::new(pool);
    let songs = repo.list(library_id, 100_000, 0).await?;
    let mut payload_songs = Vec::with_capacity(songs.len());
    for song in &songs {
        let sections = repo.sections(&song.id).await?;
        payload_songs.push(fold_song(song, &sections));
    }
    let song_count = payload_songs.len() as i64;

    // 3. Mint a token (refreshes + rotates the shared session).
    let config = SupabaseConfig::resolve()
        .ok_or_else(|| AppError::Internal("supabase ikke konfigurert".into()))?;
    let token = access_token(&config).await?;

    // 4. POST to the web publish endpoint; church comes from OUR claims and is
    //    re-validated server-side against the JWKS-verified token.
    let payload = PublishPayload {
        church_id: church_id.clone(),
        songs: payload_songs,
        deleted: vec![],
    };
    let url = format!("{}/api/library/publish", web_base_url());
    let resp = http_client()?
        .post(&url)
        .header("authorization", format!("Bearer {token}"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("publish request: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => AppError::Validation("reauth_required".into()),
            403 => AppError::Validation("not_authorized_for_church".into()),
            503 => AppError::Internal("publish_not_configured".into()),
            other => AppError::Internal(format!("publish returned HTTP {other}")),
        });
    }
    let parsed: UpsertResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("publish response: {e}")))?;
    Ok(PublishResult {
        upserted: parsed.upserted,
        deleted: parsed.deleted,
        church_id,
        song_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, title: &str) -> Song {
        Song {
            id: id.into(),
            library_id: "lib".into(),
            title: title.into(),
            ccli_song_id: Some("123".into()),
            tono_work_id: None,
            copyright_notice: Some("Public Domain".into()),
            default_key: Some("G".into()),
            tempo_bpm: None,
            language: "no".into(),
            last_used_at: None,
            theme_id: None,
            template_id: None,
            created_at: 1,
            updated_at: 4242,
            deleted_at: None,
        }
    }

    fn section(label: &str, lyrics: &str, order: i64) -> SongSection {
        SongSection {
            id: format!("s{order}"),
            song_id: "x".into(),
            label: label.into(),
            lyrics: lyrics.into(),
            chord_chart: None,
            display_order: order,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn folds_sections_to_trimmed_nonblank_lines() {
        let s = song("song-a", "Stor er din trofasthet");
        let secs = vec![section("Vers 1", "  Linje en \n\n Linje to ", 0)];
        let p = fold_song(&s, &secs);
        assert_eq!(p.source_song_id, "song-a");
        assert_eq!(p.source_updated_at, 4242);
        assert_eq!(p.default_key.as_deref(), Some("G"));
        assert_eq!(p.sections.len(), 1);
        assert_eq!(p.sections[0].label.as_deref(), Some("Vers 1"));
        assert_eq!(p.sections[0].lines, vec!["Linje en", "Linje to"]);
    }

    #[test]
    fn blank_label_becomes_none_and_empty_sections_drop() {
        let s = song("b", "T");
        let secs = vec![
            section("  ", "Halleluja", 0), // blank label → None
            section("Vers", "   \n  ", 1), // no real lines → dropped
        ];
        let p = fold_song(&s, &secs);
        assert_eq!(p.sections.len(), 1);
        assert_eq!(p.sections[0].label, None);
        assert_eq!(p.sections[0].lines, vec!["Halleluja"]);
    }
}
