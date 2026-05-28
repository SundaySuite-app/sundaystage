//! Phase 10.2 (feature 3) — TONO streaming-licence audit.
//!
//! Norwegian reality: TONO treats a *streamed* performance differently from an
//! in-room one — a separate royalty pool and a separate licence add-on. So when
//! SundayRec is streaming and SundayStage shows a copyrighted song, that usage
//! must be captured for TONO reporting, and the operator should be warned
//! *before* the service if the church lacks the streaming add-on.
//!
//! This module is the pure logic: classify copyright, build the streamed-usage
//! report, and the pre-service advisory. Reading the church's add-on status and
//! sending usage to SundaySong live is deferred (needs a Sunday account); those
//! arrive here as plain inputs so the rules stay fully unit-tested.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One song shown in a service, with the metadata TONO classification needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongUsage {
    pub song_id: String,
    pub title: String,
    pub ccli_song_id: Option<String>,
    pub tono_work_id: Option<String>,
    pub copyright_notice: Option<String>,
}

/// An entry in the TONO streaming report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TonoEntry.ts")]
pub struct TonoEntry {
    pub song_id: String,
    pub title: String,
}

/// Whether a song is copyright-protected (i.e. not public domain) for TONO
/// purposes. Public-domain notice wins; otherwise a CCLI/TONO id or any other
/// copyright notice marks it protected. Unknown ⇒ treated as not protected, to
/// avoid false alarms.
pub fn is_copyrighted(u: &SongUsage) -> bool {
    if let Some(notice) = &u.copyright_notice {
        if notice.to_lowercase().contains("public domain") {
            return false;
        }
        if !notice.trim().is_empty() {
            return true;
        }
    }
    u.ccli_song_id.is_some() || u.tono_work_id.is_some()
}

/// The streamed copyrighted songs that need TONO reporting. Empty when the
/// service wasn't streamed.
pub fn streamed_report(usages: &[SongUsage], was_streaming: bool) -> Vec<TonoEntry> {
    if !was_streaming {
        return Vec::new();
    }
    usages
        .iter()
        .filter(|u| is_copyrighted(u))
        .map(|u| TonoEntry {
            song_id: u.song_id.clone(),
            title: u.title.clone(),
        })
        .collect()
}

/// A pre-service advisory: when streaming is planned and the church lacks the
/// TONO streaming add-on, warn about the copyrighted songs that would be
/// streamed. `None` when there's nothing to flag. Surfaced in the operator UI
/// *before* the service — never during live.
pub fn pre_service_advisory(
    usages: &[SongUsage],
    streaming_planned: bool,
    has_streaming_addon: bool,
) -> Option<String> {
    if !streaming_planned || has_streaming_addon {
        return None;
    }
    let n = usages.iter().filter(|u| is_copyrighted(u)).count();
    if n == 0 {
        return None;
    }
    Some(format!(
        "{n} opphavsrettsbeskyttede sanger blir strømmet, men kirken mangler TONO strømme-tillegg. Sjekk lisensen før tjenesten."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: &str, ccli: Option<&str>, notice: Option<&str>) -> SongUsage {
        SongUsage {
            song_id: id.into(),
            title: id.into(),
            ccli_song_id: ccli.map(Into::into),
            tono_work_id: None,
            copyright_notice: notice.map(Into::into),
        }
    }

    #[test]
    fn public_domain_notice_wins() {
        assert!(!is_copyrighted(&u("a", Some("123"), Some("Public Domain"))));
        assert!(!is_copyrighted(&u("b", None, Some("public domain"))));
    }

    #[test]
    fn ccli_or_notice_marks_protected() {
        assert!(is_copyrighted(&u("a", Some("7059628"), None)));
        assert!(is_copyrighted(&u("b", None, Some("© 2020 Worship Co"))));
    }

    #[test]
    fn unknown_is_not_flagged() {
        assert!(!is_copyrighted(&u("a", None, None)));
        assert!(!is_copyrighted(&u("b", None, Some("   "))));
    }

    #[test]
    fn report_empty_when_not_streaming() {
        let usages = vec![u("a", Some("1"), None)];
        assert!(streamed_report(&usages, false).is_empty());
    }

    #[test]
    fn report_lists_only_streamed_copyrighted() {
        let usages = vec![
            u("hymn", None, Some("Public Domain")),
            u("modern", Some("7059628"), None),
        ];
        let r = streamed_report(&usages, true);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].song_id, "modern");
    }

    #[test]
    fn advisory_only_when_streaming_without_addon_and_copyrighted() {
        let usages = vec![
            u("modern", Some("1"), None),
            u("hymn", None, Some("Public Domain")),
        ];
        // streaming planned, no add-on, one copyrighted → advisory
        assert!(pre_service_advisory(&usages, true, false).is_some());
        // has add-on → none
        assert!(pre_service_advisory(&usages, true, true).is_none());
        // not streaming → none
        assert!(pre_service_advisory(&usages, false, false).is_none());
        // only PD songs → none
        let pd = vec![u("hymn", None, Some("Public Domain"))];
        assert!(pre_service_advisory(&pd, true, false).is_none());
    }
}
