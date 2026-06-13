//! Phase 6.1 — crash-recovery write-ahead log for the live session.
//!
//! ProPresenter's worst failure mode is a Sunday-morning crash that loses the
//! operator's place. We beat that with an append-only log: when a service goes
//! live we write one **header** line (the compiled cue list + start time), and
//! every operator action thereafter is a single appended JSON line. Appends are
//! tiny and atomic-enough that a crash can only ever lose the very last action,
//! and [`SessionStore::recover`] simply skips a torn trailing line.
//!
//! Crash detection is implicit: a clean shutdown calls [`SessionStore::clear`],
//! so if the log still exists on the next launch, the previous session ended
//! abnormally and can be resumed.
//!
//! The log is plain JSON Lines (not SQLite) so recovery never depends on the
//! database being healthy — the reliability layer must not share a failure
//! domain with the thing it's protecting against.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::services::cue_list::CueList;
use crate::services::live_session::{LiveAction, LiveSession};

/// The first line of the log: enough to reconstruct the session shell before
/// replaying actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryHeader {
    service_id: String,
    started_at: i64,
    cue_list: CueList,
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Convenience: the standard log location inside an app data dir.
    pub fn in_dir(dir: &Path) -> Self {
        Self::new(dir.join("live_session.log"))
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Sidecar file holding the companion broadcaster's next `seq`. Kept beside
    /// the action log (same "don't share a failure domain" philosophy) so crash
    /// recovery can resume the monotonic broadcast stream from the *true*
    /// last-emitted seq.
    fn seq_path(&self) -> PathBuf {
        self.path.with_extension("seq")
    }

    /// Persist the companion broadcaster's next `seq` (best-effort, like the
    /// action log). Recovery needs this because `companion_broadcast` re-pushes
    /// (a phone joining mid-service, a manual re-push) advance `seq` *without*
    /// appending an action, so the action-log length under-counts the real
    /// emitted seq. Seeding recovery from `log_len` alone would restart at or
    /// below a phone's `lastSeq` and re-freeze exactly the phones the resume
    /// mechanism exists to protect.
    pub fn record_seq(&self, next_seq: u32) -> std::io::Result<()> {
        let mut f = File::create(self.seq_path())?;
        write!(f, "{next_seq}")?;
        f.flush()
    }

    /// The persisted next `seq`, if any. `None` when none was ever written (cloud
    /// companion off, or a log from before this sidecar existed) or the file is
    /// torn — callers fall back to the action-log length.
    pub fn recover_seq(&self) -> Option<u32> {
        std::fs::read_to_string(self.seq_path())
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Start (or restart) the log: truncate and write the header line.
    pub fn begin(&self, session: &LiveSession) -> std::io::Result<()> {
        let header = RecoveryHeader {
            service_id: session.service_id.clone(),
            started_at: session.started_at,
            cue_list: session.cue_list.clone(),
        };
        let mut f = File::create(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&header)?)?;
        f.flush()
    }

    /// Append one action. One line, flushed immediately — crash-safe.
    pub fn record(&self, action: &LiveAction) -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(action)?)?;
        f.flush()
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.seq_path());
    }

    /// Reconstruct the session by replaying the log. Returns `None` if there is
    /// no log or the header is missing/corrupt. A torn final action line is
    /// skipped (we only ever lose the last, un-acked action).
    pub fn recover(&self) -> Option<LiveSession> {
        let file = File::open(&self.path).ok()?;
        let mut lines = BufReader::new(file).lines();

        let header_line = lines.next()?.ok()?;
        let header: RecoveryHeader = serde_json::from_str(&header_line).ok()?;

        let mut session = LiveSession::new(header.service_id, header.cue_list, header.started_at);
        for line in lines.map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            // A corrupt/torn line (e.g. a crash mid-write) ends replay safely.
            match serde_json::from_str::<LiveAction>(&line) {
                Ok(action) => session.dispatch(action, header.started_at),
                Err(_) => break,
            }
        }
        Some(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::{Cue, CueList, CueSource, SlideContent};
    use crate::services::live_session::OutputState;

    fn slide_cue(i: usize) -> Cue {
        Cue::ShowSlide {
            cue_id: format!("c{i}"),
            slide_content: Box::new(SlideContent {
                section_label: None,
                text_lines: vec![format!("line {i}")],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
                appearance: None,
            }),
            theme_id: None,
            template_id: None,
            source: CueSource {
                service_item_id: "item".into(),
                item_cue_index: 0,
                display_label: format!("Cue {i}"),
            },
        }
    }

    fn session(n: usize) -> LiveSession {
        let cues = (0..n).map(slide_cue).collect();
        LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues,
            },
            100,
        )
    }

    fn store() -> (tempfile::TempDir, SessionStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::in_dir(dir.path());
        (dir, store)
    }

    #[test]
    fn no_log_means_nothing_to_recover() {
        let (_d, store) = store();
        assert!(!store.exists());
        assert!(store.recover().is_none());
    }

    #[test]
    fn begin_then_record_recovers_exact_position_and_output() {
        let (_d, store) = store();
        let mut live = session(5);
        store.begin(&live).unwrap();

        // Simulate the operator advancing twice then blacking out.
        for action in [LiveAction::Next, LiveAction::Next, LiveAction::Blackout] {
            live.dispatch(action.clone(), 200);
            store.record(&action).unwrap();
        }
        assert_eq!(live.index, 2);
        assert_eq!(live.output, OutputState::Blackout);

        let recovered = store.recover().expect("recoverable");
        assert_eq!(recovered.index, 2);
        assert_eq!(recovered.output, OutputState::Blackout);
        assert_eq!(recovered.view().total, 5);
    }

    #[test]
    fn recovery_skips_a_torn_trailing_line() {
        let (_d, store) = store();
        let live = session(4);
        store.begin(&live).unwrap();
        store.record(&LiveAction::Next).unwrap();
        // Simulate a crash mid-write: append a half-written line.
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&store.path)
                .unwrap();
            write!(f, "{{\"type\":\"go_t").unwrap(); // truncated, no newline
        }
        let recovered = store.recover().expect("recoverable despite torn line");
        // Only the clean Next applied → index 1.
        assert_eq!(recovered.index, 1);
    }

    #[test]
    fn clear_removes_the_log() {
        let (_d, store) = store();
        store.begin(&session(2)).unwrap();
        assert!(store.exists());
        store.clear();
        assert!(!store.exists());
        assert!(store.recover().is_none());
    }

    #[test]
    fn seq_round_trips_and_clear_removes_it() {
        let (_d, store) = store();
        // No seq written yet → caller must fall back to the action-log length.
        assert_eq!(store.recover_seq(), None);
        // A re-push can push the real seq above the action count; recovery must
        // see that true value, not the (smaller) log length.
        store.record_seq(43).unwrap();
        assert_eq!(store.recover_seq(), Some(43));
        // Overwrites, never appends.
        store.record_seq(44).unwrap();
        assert_eq!(store.recover_seq(), Some(44));
        // A clean shutdown clears the seq sidecar too.
        store.clear();
        assert_eq!(store.recover_seq(), None);
    }

    #[test]
    fn corrupt_header_recovers_nothing() {
        let (dir, store) = store();
        std::fs::write(dir.path().join("live_session.log"), "not json\n").unwrap();
        assert!(store.recover().is_none());
    }
}
