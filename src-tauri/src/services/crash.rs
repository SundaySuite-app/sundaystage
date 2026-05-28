//! Phase 6.1 — opt-in crash reporting.
//!
//! Privacy-first: nothing leaves the machine. When the user opts in, a panic
//! hook writes a small report (timestamp, app version, panic message +
//! location) to `crashes/` in the app data dir, so a stuck Sunday morning
//! leaves a breadcrumb the operator can hand us. Wiring these up to a remote
//! sink (Sentry / an OSS equivalent like GlitchTip) is a follow-up that needs a
//! DSN — the local capture is the part that works offline and respects the
//! "no cloud by default" promise.

use std::path::{Path, PathBuf};

const FLAG_FILE: &str = "crash_reporting.json";
const CRASH_DIR: &str = "crashes";

fn flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FLAG_FILE)
}

pub fn crash_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CRASH_DIR)
}

/// Whether the user has opted in to local crash capture (default off).
pub fn is_enabled(data_dir: &Path) -> bool {
    std::fs::read_to_string(flag_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
        .unwrap_or(false)
}

/// Persist the opt-in choice.
pub fn set_enabled(data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    let body = serde_json::json!({ "enabled": enabled }).to_string();
    std::fs::write(flag_path(data_dir), body)
}

/// Number of crash reports captured so far.
pub fn report_count(data_dir: &Path) -> usize {
    std::fs::read_dir(crash_dir(data_dir))
        .map(|rd| rd.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

/// Delete all captured reports.
pub fn clear_reports(data_dir: &Path) -> std::io::Result<()> {
    let dir = crash_dir(data_dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Render a crash report body. Pure — unit-tested.
pub fn format_report(version: &str, now_ms: i64, message: &str, location: &str) -> String {
    format!(
        "SundayStage crash report\nversion: {version}\nwhen: {now_ms} (unix ms)\nwhere: {location}\nwhat: {message}\n"
    )
}

/// Install a panic hook that captures a report when the user has opted in. The
/// previous hook is chained so default behaviour (stderr/abort) is preserved.
pub fn install_panic_hook(data_dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_enabled(&data_dir) {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown".to_string());
            let now = chrono::Utc::now().timestamp_millis();
            let body = format_report(env!("CARGO_PKG_VERSION"), now, &message, &location);
            let dir = crash_dir(&data_dir);
            if std::fs::create_dir_all(&dir).is_ok() {
                let _ = std::fs::write(dir.join(format!("crash-{now}.txt")), body);
            }
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_round_trips_and_defaults_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_enabled(dir.path())); // default off
        set_enabled(dir.path(), true).unwrap();
        assert!(is_enabled(dir.path()));
        set_enabled(dir.path(), false).unwrap();
        assert!(!is_enabled(dir.path()));
    }

    #[test]
    fn report_body_has_the_key_facts() {
        let r = format_report("1.2.3", 42, "boom", "src/x.rs:1:2");
        assert!(r.contains("version: 1.2.3"));
        assert!(r.contains("42"));
        assert!(r.contains("boom"));
        assert!(r.contains("src/x.rs:1:2"));
    }

    #[test]
    fn count_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(report_count(dir.path()), 0);
        std::fs::create_dir_all(crash_dir(dir.path())).unwrap();
        std::fs::write(crash_dir(dir.path()).join("crash-1.txt"), "x").unwrap();
        assert_eq!(report_count(dir.path()), 1);
        clear_reports(dir.path()).unwrap();
        assert_eq!(report_count(dir.path()), 0);
    }
}
