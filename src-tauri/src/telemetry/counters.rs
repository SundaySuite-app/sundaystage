//! E3 — the counter vocabulary and its in-memory buffer.
//!
//! ## Why the names are an enum and not strings
//!
//! A counter name is the ONE place a free-form label could smuggle user content
//! into an otherwise purely numeric payload: `export.Gudstjeneste 6. april`
//! looks like telemetry and is actually a service title. So the vocabulary is a
//! CLOSED enum, the wire strings are generated from it, and the IPC surface
//! validates against exactly this list — the boundary cannot widen it either.
//!
//! The wire strings are dotted namespaces (`area.thing.variant`) so the
//! endpoint can aggregate by prefix without knowing every leaf.
//!
//! ## Two tiers of storage
//!
//! [`CounterBuffer`] is an array of atomics — one `fetch_add` per event, no
//! allocation, no lock, callable from anywhere including a cue dispatch. SQLite
//! is where the totals actually live, and the buffer is folded into it only
//! when nothing is live (see [`crate::telemetry::quality::drain_if_quiet`]).
//! Law 1 says a counter may never reach a disk while a service is running, and
//! this split is how that is arranged rather than merely intended.
//!
//! Nothing here sends anything. E5 adds the watermark drain that turns these
//! totals into a payload; E3 only accumulates.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How many counter names exist. Pinned as a constant so the buffer can be a
/// fixed array and so a name added without a matching test update fails to
/// compile rather than silently changing the payload shape.
pub const COUNTER_COUNT: usize = 19;

/// Every counter name that may ever appear on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CounterName.ts")]
pub enum CounterName {
    /// A service went live.
    #[serde(rename = "live.session.started")]
    LiveSessionStarted,
    /// A live session ended cleanly through `live_end`.
    #[serde(rename = "live.session.ended")]
    LiveSessionEnded,
    /// A live session was recovered from the crash-recovery WAL at launch.
    #[serde(rename = "live.session.recovered")]
    LiveSessionRecovered,
    /// One operator cue action was dispatched.
    #[serde(rename = "live.cue.dispatched")]
    LiveCueDispatched,
    /// The live outputs were opened.
    #[serde(rename = "output.opened")]
    OutputOpened,
    /// The outputs opened as in-process windows instead of isolated processes.
    #[serde(rename = "output.fallback.in_process")]
    OutputFallbackInProcess,
    /// A service was created in the editor.
    #[serde(rename = "editor.service.created")]
    EditorServiceCreated,
    /// A song was created in the editor.
    #[serde(rename = "editor.song.created")]
    EditorSongCreated,
    /// The AI lyric formatter ran.
    #[serde(rename = "ai.format.run")]
    AiFormatRun,
    /// The AI service planner / search ran.
    #[serde(rename = "ai.search.run")]
    AiSearchRun,
    /// The local library was published to the cloud.
    #[serde(rename = "library.publish.run")]
    LibraryPublishRun,
    /// Songs were imported into the library.
    #[serde(rename = "library.import.run")]
    LibraryImportRun,
    /// A Bible verse was put on the screen.
    #[serde(rename = "bible.verse.projected")]
    BibleVerseProjected,
    /// A custom deck was presented.
    #[serde(rename = "deck.presented")]
    DeckPresented,
    /// A theme was created.
    #[serde(rename = "theme.created")]
    ThemeCreated,
    /// A companion phone connected.
    #[serde(rename = "companion.connected")]
    CompanionConnected,
    /// A service template was applied.
    #[serde(rename = "template.applied")]
    TemplateApplied,
    /// An update was installed from the ring.
    #[serde(rename = "update.installed")]
    UpdateInstalled,
    /// A manual problem report was sent (E6).
    #[serde(rename = "report.manual.sent")]
    ReportManualSent,
}

/// Every [`CounterName`], in wire order. The single source of truth for the
/// buffer's slot indices, the persisted key set and the tests.
pub const ALL_COUNTERS: [CounterName; COUNTER_COUNT] = [
    CounterName::LiveSessionStarted,
    CounterName::LiveSessionEnded,
    CounterName::LiveSessionRecovered,
    CounterName::LiveCueDispatched,
    CounterName::OutputOpened,
    CounterName::OutputFallbackInProcess,
    CounterName::EditorServiceCreated,
    CounterName::EditorSongCreated,
    CounterName::AiFormatRun,
    CounterName::AiSearchRun,
    CounterName::LibraryPublishRun,
    CounterName::LibraryImportRun,
    CounterName::BibleVerseProjected,
    CounterName::DeckPresented,
    CounterName::ThemeCreated,
    CounterName::CompanionConnected,
    CounterName::TemplateApplied,
    CounterName::UpdateInstalled,
    CounterName::ReportManualSent,
];

impl CounterName {
    /// The dotted wire string. Written out by hand rather than derived from the
    /// variant name: the wire is a contract with the Worker, and a rename
    /// refactor must not be able to change it by accident.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveSessionStarted => "live.session.started",
            Self::LiveSessionEnded => "live.session.ended",
            Self::LiveSessionRecovered => "live.session.recovered",
            Self::LiveCueDispatched => "live.cue.dispatched",
            Self::OutputOpened => "output.opened",
            Self::OutputFallbackInProcess => "output.fallback.in_process",
            Self::EditorServiceCreated => "editor.service.created",
            Self::EditorSongCreated => "editor.song.created",
            Self::AiFormatRun => "ai.format.run",
            Self::AiSearchRun => "ai.search.run",
            Self::LibraryPublishRun => "library.publish.run",
            Self::LibraryImportRun => "library.import.run",
            Self::BibleVerseProjected => "bible.verse.projected",
            Self::DeckPresented => "deck.presented",
            Self::ThemeCreated => "theme.created",
            Self::CompanionConnected => "companion.connected",
            Self::TemplateApplied => "template.applied",
            Self::UpdateInstalled => "update.installed",
            Self::ReportManualSent => "report.manual.sent",
        }
    }

    /// Parse a wire string back. `None` for anything outside the closed set —
    /// which is what makes the allow-list an allow-list.
    pub fn from_wire(raw: &str) -> Option<Self> {
        ALL_COUNTERS.into_iter().find(|c| c.as_str() == raw)
    }

    /// This counter's slot in a [`CounterBuffer`].
    fn slot(self) -> usize {
        // Linear over 19 `Copy` variants: a handful of comparisons, no
        // allocation, and no second table that could drift out of order.
        ALL_COUNTERS
            .iter()
            .position(|c| *c == self)
            .expect("every CounterName is in ALL_COUNTERS")
    }
}

/// One counter's accumulated total, for the read command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CounterTotal.ts")]
#[serde(rename_all = "camelCase")]
pub struct CounterTotal {
    /// Serialises AS the dotted wire string — see [`CounterName`]'s serde
    /// renames — so a reader never has to map the enum to a name.
    pub name: CounterName,
    /// Persisted total plus anything still buffered in memory.
    #[ts(type = "number")]
    pub value: i64,
}

/// The in-memory side: an array of atomics, one slot per name.
///
/// [`note`](Self::note) is the only method the live path calls. It takes
/// `&self`, returns `()`, allocates nothing and cannot fail — a `fetch_add` on
/// a `Relaxed` atomic and nothing else.
#[derive(Debug)]
pub struct CounterBuffer {
    slots: [AtomicU64; COUNTER_COUNT],
}

impl Default for CounterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CounterBuffer {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Record one occurrence. Live-safe: no lock, no allocation, no `Result`.
    ///
    /// `Relaxed` is the right ordering — these counts are summed later and
    /// never used to order anything, so paying for a fence on a cue dispatch
    /// would buy nothing.
    pub fn note(&self, name: CounterName) {
        self.slots[name.slot()].fetch_add(1, Ordering::Relaxed);
    }

    /// Everything accumulated since the last take, RESET to zero.
    ///
    /// Called only from the drain, i.e. only when nothing is live. Zero-valued
    /// counters are omitted so a quiet week writes nothing.
    pub fn take_deltas(&self) -> Vec<(CounterName, u64)> {
        ALL_COUNTERS
            .into_iter()
            .filter_map(|name| {
                let n = self.slots[name.slot()].swap(0, Ordering::Relaxed);
                (n > 0).then_some((name, n))
            })
            .collect()
    }

    /// Everything accumulated since the last take, WITHOUT resetting — so the
    /// read command can show a total that includes not-yet-persisted events.
    pub fn peek(&self) -> Vec<(CounterName, u64)> {
        ALL_COUNTERS
            .into_iter()
            .filter_map(|name| {
                let n = self.slots[name.slot()].load(Ordering::Relaxed);
                (n > 0).then_some((name, n))
            })
            .collect()
    }

    /// Put deltas back after a failed persist, so a transient disk error costs
    /// nothing. Only the drain calls this, and only off the live path.
    pub fn restore(&self, deltas: &[(CounterName, u64)]) {
        for (name, n) in deltas {
            self.slots[name.slot()].fetch_add(*n, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_vocabulary_is_closed_complete_and_unique() {
        assert_eq!(ALL_COUNTERS.len(), COUNTER_COUNT);
        let wires: HashSet<&str> = ALL_COUNTERS.iter().map(|c| c.as_str()).collect();
        assert_eq!(wires.len(), COUNTER_COUNT, "duplicate wire string");
        // Every slot index is distinct and inside the array.
        let slots: HashSet<usize> = ALL_COUNTERS.iter().map(|c| c.slot()).collect();
        assert_eq!(slots.len(), COUNTER_COUNT);
        assert!(slots.iter().all(|s| *s < COUNTER_COUNT));
    }

    #[test]
    fn the_wire_strings_are_exactly_the_programmes_list() {
        // Copied from docs/TELEMETRY-PROGRAM.md's `counters` section. If a name
        // changes on either side, this fails instead of the Worker 400-ing a
        // real payload in the field (law 3, applied to the counter allow-list).
        let expected = [
            "live.session.started",
            "live.session.ended",
            "live.session.recovered",
            "live.cue.dispatched",
            "output.opened",
            "output.fallback.in_process",
            "editor.service.created",
            "editor.song.created",
            "ai.format.run",
            "ai.search.run",
            "library.publish.run",
            "library.import.run",
            "bible.verse.projected",
            "deck.presented",
            "theme.created",
            "companion.connected",
            "template.applied",
            "update.installed",
            "report.manual.sent",
        ];
        let actual: Vec<&str> = ALL_COUNTERS.iter().map(|c| c.as_str()).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn the_serde_representation_is_the_wire_string() {
        // Two spellings of one name is one spelling too many: E5's payload
        // builder serialises the enum, the repository keys rows by `as_str`,
        // and E4's Worker validates against a list. They must be the same
        // characters or the allow-list rejects a counter that exists.
        for c in ALL_COUNTERS {
            assert_eq!(
                serde_json::to_string(&c).expect("serialises"),
                format!("\"{}\"", c.as_str()),
                "{c:?}"
            );
            let back: CounterName =
                serde_json::from_str(&format!("\"{}\"", c.as_str())).expect("round trips");
            assert_eq!(back, c);
        }
        // The PascalCase variant name is NOT accepted — the dotted string is
        // the only spelling on the wire.
        assert!(serde_json::from_str::<CounterName>("\"LiveSessionStarted\"").is_err());
    }

    #[test]
    fn only_known_names_parse() {
        for c in ALL_COUNTERS {
            assert_eq!(CounterName::from_wire(c.as_str()), Some(c));
        }
        // The whole point of the allow-list: a caller cannot invent a name.
        assert_eq!(CounterName::from_wire("export.Gudstjeneste 6. april"), None);
        assert_eq!(CounterName::from_wire("live.session"), None);
        assert_eq!(CounterName::from_wire(""), None);
        assert_eq!(CounterName::from_wire("LIVE.SESSION.STARTED"), None);
    }

    #[test]
    fn noting_accumulates_and_taking_resets() {
        let buf = CounterBuffer::new();
        assert!(buf.peek().is_empty(), "a fresh buffer reports nothing");
        buf.note(CounterName::LiveCueDispatched);
        buf.note(CounterName::LiveCueDispatched);
        buf.note(CounterName::OutputOpened);
        assert_eq!(
            buf.peek(),
            vec![
                (CounterName::LiveCueDispatched, 2),
                (CounterName::OutputOpened, 1),
            ],
            "peek is in wire order and omits zeroes"
        );
        // Peek does not consume.
        assert_eq!(buf.peek().len(), 2);
        let taken = buf.take_deltas();
        assert_eq!(taken.len(), 2);
        assert!(buf.peek().is_empty(), "take resets");
        // …and restore puts them back exactly.
        buf.restore(&taken);
        assert_eq!(buf.peek(), taken);
    }

    #[test]
    fn counters_are_independent_across_slots() {
        let buf = CounterBuffer::new();
        for (i, name) in ALL_COUNTERS.into_iter().enumerate() {
            for _ in 0..=i {
                buf.note(name);
            }
        }
        let got = buf.peek();
        assert_eq!(got.len(), COUNTER_COUNT);
        for (i, (name, n)) in got.iter().enumerate() {
            assert_eq!(*name, ALL_COUNTERS[i]);
            assert_eq!(*n, i as u64 + 1, "slot collision at {name:?}");
        }
    }

    #[test]
    fn noting_is_safe_from_many_threads_at_once() {
        // The live path may call `note` from a command handler while the
        // supervisor calls it from a tokio worker. No lock, no lost counts.
        let buf = std::sync::Arc::new(CounterBuffer::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let buf = std::sync::Arc::clone(&buf);
            handles.push(std::thread::spawn(move || {
                for _ in 0..1_000 {
                    buf.note(CounterName::LiveCueDispatched);
                }
            }));
        }
        for h in handles {
            h.join().expect("thread");
        }
        assert_eq!(buf.peek(), vec![(CounterName::LiveCueDispatched, 8_000)]);
    }
}
