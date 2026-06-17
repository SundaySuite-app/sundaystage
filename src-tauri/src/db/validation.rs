//! Input length caps enforced at the command/repository boundary (A2).
//!
//! Volunteers paste lyrics, titles and notes from anywhere — a runaway clipboard
//! (a whole document, a binary blob) must be rejected with a clear
//! `AppError::Validation` long before it reaches SQLite or the live engine,
//! rather than bloating the DB or freezing the slide breaker mid-service.
//!
//! These are *upper bounds*, deliberately generous — the longest real hymn is a
//! few KB of lyrics, the longest sensible title a sentence. They exist to stop
//! pathological input, not to second-guess legitimate content. Caps count
//! Unicode scalar values (`chars()`), so multibyte text isn't penalised.
//!
//! The repository methods call these so *every* caller (Tauri commands, the
//! plan importer, tests) is guarded by the same rule with no duplication.

use crate::error::{AppError, AppResult};

/// Max song / service title length.
pub const MAX_TITLE: usize = 500;
/// Max lyrics length for one song section.
pub const MAX_LYRICS: usize = 50_000;
/// Max planner notes length (service notes, item notes).
pub const MAX_NOTES: usize = 2_000;
/// Max section label length ("Verse 1", "Pre-Chorus", …).
pub const MAX_SECTION_LABEL: usize = 100;

/// Reject `value` when its character count exceeds `max`. `field` names the
/// offending input so the renderer can point the user at it.
pub fn check_len(field: &str, value: &str, max: usize) -> AppResult<()> {
    let len = value.chars().count();
    if len > max {
        return Err(AppError::Validation(format!(
            "{field} is too long ({len} characters; max {max})"
        )));
    }
    Ok(())
}

/// Cap a song / service title.
pub fn title(value: &str) -> AppResult<()> {
    check_len("title", value, MAX_TITLE)
}

/// Cap one section's lyrics.
pub fn lyrics(value: &str) -> AppResult<()> {
    check_len("lyrics", value, MAX_LYRICS)
}

/// Cap planner notes.
pub fn notes(value: &str) -> AppResult<()> {
    check_len("notes", value, MAX_NOTES)
}

/// Cap a section label.
pub fn section_label(value: &str) -> AppResult<()> {
    check_len("section label", value, MAX_SECTION_LABEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_cap_passes() {
        assert!(title("Amazing Grace").is_ok());
        assert!(lyrics("a\nb\nc").is_ok());
        assert!(notes("remember to dim the lights").is_ok());
        assert!(section_label("Pre-Chorus").is_ok());
    }

    #[test]
    fn at_cap_passes_over_cap_fails() {
        // Exactly at the cap is allowed; one over is rejected.
        assert!(check_len("x", &"a".repeat(MAX_TITLE), MAX_TITLE).is_ok());
        let err = check_len("x", &"a".repeat(MAX_TITLE + 1), MAX_TITLE).unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn each_field_rejects_oversize() {
        assert_eq!(
            title(&"t".repeat(MAX_TITLE + 1)).unwrap_err().code(),
            "validation"
        );
        assert_eq!(
            lyrics(&"l".repeat(MAX_LYRICS + 1)).unwrap_err().code(),
            "validation"
        );
        assert_eq!(
            notes(&"n".repeat(MAX_NOTES + 1)).unwrap_err().code(),
            "validation"
        );
        assert_eq!(
            section_label(&"s".repeat(MAX_SECTION_LABEL + 1))
                .unwrap_err()
                .code(),
            "validation"
        );
    }

    #[test]
    fn counts_chars_not_bytes() {
        // A multibyte char counts as one toward the cap.
        let s = "é".repeat(MAX_TITLE);
        assert!(
            title(&s).is_ok(),
            "{} bytes but {} chars",
            s.len(),
            MAX_TITLE
        );
    }

    #[test]
    fn error_message_names_the_field() {
        let err = section_label(&"x".repeat(MAX_SECTION_LABEL + 1)).unwrap_err();
        assert!(err.to_string().contains("section label"));
    }
}
