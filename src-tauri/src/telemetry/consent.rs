//! E5 — the telemetry consent state machine. Pure, and default-closed.
//!
//! ## Why "off" is not one state but two
//!
//! The obvious model is a boolean, and it is wrong. A boolean cannot tell "this
//! operator was asked and said no" apart from "this operator has never been
//! asked", and those two demand opposite behaviour from the UI E6 builds: the
//! first must be left alone forever, the second must be asked exactly once.
//! SundayStage has shipped since v0.1, so most installs already finished
//! onboarding and will never see it again — a boolean would silently mean "no"
//! for every one of them, and the feature would only ever reach fresh installs.
//!
//! So the persisted record is `Option<ConsentRecord>` and the derived state is a
//! three-value [`ConsentStatus`]:
//!
//! ```text
//!                      ┌──────────────┐
//!     no record  ────►  │  NeverAsked  │  needsPrompt = true   active = false
//!                      └──────┬───────┘
//!                             │  telemetry_consent_set(true|false)
//!                ┌────────────┴────────────┐
//!                ▼                         ▼
//!         ┌─────────────┐           ┌────────────┐
//!         │  Granted(v) │           │  Denied(v) │
//!         └─────────────┘           └────────────┘
//!     v == CURRENT:                 v == CURRENT:
//!       needsPrompt = false           needsPrompt = false
//!       active      = true            active      = false
//!
//!     v != CURRENT  (the scope moved since they answered):
//!       needsPrompt = true            needsPrompt = true
//!       active      = FALSE           active      = false
//! ```
//!
//! ## The two rules that make it safe
//!
//! 1. **Absent means no.** A missing record, an unparseable record, a record
//!    from a version this build does not understand — all of them are "not
//!    granted". Nothing about telemetry is ever the default-on branch of a
//!    condition.
//! 2. **A stale grant is not a grant.** If [`CONSENT_VERSION`] is bumped because
//!    the payload started carrying a category the user was not asked about, an
//!    operator who consented to the OLD scope has consented to something that no
//!    longer exists. Sending stops (`active = false`) and the prompt returns —
//!    for a `Denied` record too, because "no" answered a different question.
//!    That is the direction the failure has to fall: a paused report costs a
//!    datapoint, a widened one costs the promise the privacy text made.
//!
//! Everything here is pure. [`crate::telemetry::client`] reads and writes the
//! record, mints the install id, and calls these functions to decide what any of
//! it means.
//!
//! ## Divergence from SundayRec
//!
//! Rec's identical machine lives in a shared core crate and is at
//! `CONSENT_VERSION = 2` (its v2 widened quality data to carry coarse size
//! bands). SundayStage starts at **1** and does not share the crate: the two
//! apps' scopes are different documents describing different payloads, and a
//! shared constant would mean a Rec scope change re-prompting every Stage
//! operator about something Stage does not collect.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The version of the consent SCOPE — bumped when the payload starts carrying a
/// category of data the user was not asked about.
///
/// Deliberately separate from [`super::payload::PAYLOAD_SCHEMA`]: adding a field
/// within a category already agreed to (one more counter, one more number on a
/// quality row) is a schema change, not a scope change, and must not re-prompt
/// anyone. Sending a NEW CATEGORY is a scope change and must.
///
/// Bumping this is never a lone edit. Three things move together or the version
/// is a lie: this constant, the scope described in the privacy text E6 writes,
/// and the re-prompt copy that has to name what was added — otherwise an
/// operator who already said yes is asked a question they cannot tell apart from
/// the one they already answered.
///
/// - **v1** — crashes, quality verdicts, feature counters, technical settings.
pub const CONSENT_VERSION: u32 = 1;

/// The persisted consent record. Absent from storage = never asked.
///
/// Versioned so a future scope expansion can re-prompt rather than assume, and
/// timestamped so the privacy text can honestly say when the choice was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentRecord {
    /// What the operator answered.
    pub granted: bool,
    /// The [`CONSENT_VERSION`] they answered FOR.
    pub version: u32,
    /// Unix ms (UTC) they answered.
    pub decided_at: i64,
}

impl ConsentRecord {
    /// Record an answer at the CURRENT scope version.
    pub fn decide(granted: bool, now_ms: i64) -> Self {
        Self {
            granted,
            version: CONSENT_VERSION,
            decided_at: now_ms,
        }
    }
}

/// The three states the UI has to tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ConsentStatus.ts")]
#[serde(rename_all = "kebab-case")]
pub enum ConsentStatus {
    /// No answer has ever been recorded. NOT the same as "no" — see the module
    /// docs. This is the state every pre-E6 install is in, including the ones
    /// that finished onboarding long ago, and the state this whole stage ships
    /// in: with no UI to answer the question, nothing downstream ever runs.
    NeverAsked,
    /// The operator said yes.
    Granted,
    /// The operator said no. Left alone; never re-asked unless the SCOPE
    /// changes.
    Denied,
}

/// The full consent picture, as E6's settings card and onboarding step need it.
///
/// `status` + `version` are the facts; `needs_prompt` and `active` are the two
/// derived questions callers actually ask, computed here so no caller has to
/// re-implement "a stale grant is not a grant" and get it subtly wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryConsent.ts")]
#[serde(rename_all = "camelCase")]
pub struct TelemetryConsent {
    pub status: ConsentStatus,
    /// The scope version the recorded answer was given for. 0 when never asked.
    pub version: u32,
    /// Unix ms (UTC) the answer was recorded, when there is one.
    #[ts(type = "number | null")]
    pub decided_at: Option<i64>,
    /// [`CONSENT_VERSION`] — what an answer given NOW would be recorded as.
    pub current_version: u32,
    /// Whether the UI should ask. True when never asked, and true again when the
    /// scope has moved since the recorded answer — for a `Denied` record too.
    pub needs_prompt: bool,
    /// Whether telemetry may be collected and sent RIGHT NOW. The only flag the
    /// backend consults; false in every state except a current-version grant.
    pub active: bool,
}

/// Derive the full picture from the persisted record (or its absence).
///
/// The single place the state machine lives. `None` in, `NeverAsked` out — an
/// unreadable or absent record is never a grant.
pub fn evaluate(record: Option<ConsentRecord>) -> TelemetryConsent {
    let Some(r) = record else {
        return TelemetryConsent {
            status: ConsentStatus::NeverAsked,
            version: 0,
            decided_at: None,
            current_version: CONSENT_VERSION,
            needs_prompt: true,
            active: false,
        };
    };
    // A record from a FUTURE version (a downgrade after the operator answered a
    // wider question) is also stale: this build cannot know the answer covers
    // what it sends, so it is not current and does not grant.
    let current_scope = r.version == CONSENT_VERSION;
    TelemetryConsent {
        status: if r.granted {
            ConsentStatus::Granted
        } else {
            ConsentStatus::Denied
        },
        version: r.version,
        decided_at: Some(r.decided_at),
        current_version: CONSENT_VERSION,
        needs_prompt: !current_scope,
        active: r.granted && current_scope,
    }
}

/// Parse a persisted record from its stored JSON. Any failure — absent,
/// malformed, truncated, hand-edited — is [`None`], i.e. never asked, i.e. off.
pub fn parse_record(raw: Option<&str>) -> Option<ConsentRecord> {
    serde_json::from_str::<ConsentRecord>(raw?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absence_is_never_asked_and_never_active() {
        let c = evaluate(None);
        assert_eq!(c.status, ConsentStatus::NeverAsked);
        assert!(
            c.needs_prompt,
            "an install that was never asked must be asked"
        );
        assert!(!c.active, "the default is OFF");
        assert_eq!(c.version, 0);
        assert_eq!(c.decided_at, None);
        assert_eq!(c.current_version, CONSENT_VERSION);
    }

    #[test]
    fn a_current_grant_is_the_only_active_state() {
        let c = evaluate(Some(ConsentRecord::decide(true, 1_800_000_000_000)));
        assert_eq!(c.status, ConsentStatus::Granted);
        assert!(c.active);
        assert!(!c.needs_prompt);
        assert_eq!(c.decided_at, Some(1_800_000_000_000));
        assert_eq!(c.version, CONSENT_VERSION);
    }

    #[test]
    fn a_denial_is_remembered_and_not_re_asked() {
        let c = evaluate(Some(ConsentRecord::decide(false, 1_800_000_000_000)));
        assert_eq!(c.status, ConsentStatus::Denied);
        assert!(!c.active);
        assert!(
            !c.needs_prompt,
            "'no' at the current scope means leave the operator alone"
        );
    }

    #[test]
    fn a_stale_grant_stops_sending_and_re_asks() {
        // The rule that makes a scope expansion safe: consent to the OLD scope
        // is not consent to the new one, so sending stops immediately.
        let stale = ConsentRecord {
            granted: true,
            version: CONSENT_VERSION + 1,
            decided_at: 1,
        };
        let c = evaluate(Some(stale));
        assert_eq!(c.status, ConsentStatus::Granted, "the answer is remembered");
        assert!(!c.active, "but it no longer permits sending");
        assert!(c.needs_prompt, "and the operator is asked the new question");
    }

    #[test]
    fn a_stale_denial_is_also_re_asked_and_carries_not_forward() {
        // The quietest failure a version bump could produce: re-deriving
        // "answered" as "agreed" would turn every recorded NO into a YES for a
        // scope the first question did not even have.
        let stale = ConsentRecord {
            granted: false,
            version: CONSENT_VERSION + 1,
            decided_at: 1,
        };
        let c = evaluate(Some(stale));
        assert_eq!(c.status, ConsentStatus::Denied, "'no' is still 'no'");
        assert!(c.needs_prompt, "'no' answered a different question");
        assert!(!c.active, "a denial can never be upgraded into permission");
    }

    #[test]
    fn no_recorded_denial_is_active_at_any_version() {
        // The sweep the test above is one instance of: `granted: false` has no
        // version at which it permits sending, so no future bump can find a seam
        // where a denial leaks through.
        for version in 0..=CONSENT_VERSION + 3 {
            let c = evaluate(Some(ConsentRecord {
                granted: false,
                version,
                decided_at: 1,
            }));
            assert!(!c.active, "denial at version {version} permitted sending");
            assert_eq!(c.status, ConsentStatus::Denied, "version {version}");
        }
    }

    #[test]
    fn version_one_is_the_current_scope() {
        // A tripwire, not a tautology. Changing this number changes what every
        // installed copy is allowed to send, so it must not be possible to do it
        // as an incidental edit — see CONSENT_VERSION's docs for the two other
        // things that have to move with it.
        assert_eq!(CONSENT_VERSION, 1);
    }

    #[test]
    fn a_grant_from_a_future_scope_does_not_grant() {
        // A downgrade after answering a wider question. This build cannot know
        // the answer covers what IT sends.
        let future = ConsentRecord {
            granted: true,
            version: CONSENT_VERSION + 1,
            decided_at: 1,
        };
        let c = evaluate(Some(future));
        assert!(!c.active);
        assert!(c.needs_prompt);
    }

    #[test]
    fn answering_the_current_question_clears_the_prompt() {
        // The far side of a re-prompt: the new answer has to actually clear it,
        // or the card returns every launch and reads as a bug.
        let yes = evaluate(Some(ConsentRecord::decide(true, 1_800_000_000_001)));
        assert_eq!(yes.version, CONSENT_VERSION);
        assert!(yes.active);
        assert!(!yes.needs_prompt);

        let no = evaluate(Some(ConsentRecord::decide(false, 1_800_000_000_001)));
        assert_eq!(no.version, CONSENT_VERSION);
        assert!(!no.active);
        assert!(!no.needs_prompt, "a fresh 'no' is not re-litigated either");
    }

    #[test]
    fn a_malformed_record_reads_as_never_asked() {
        for raw in [
            None,
            Some(""),
            Some("null"),
            Some("{"),
            Some("{\"granted\":true}"), // missing version/decidedAt
            Some("\"granted\""),
            Some("{\"granted\":\"yes\",\"version\":1,\"decidedAt\":0}"),
            Some("{\"granted\":true,\"version\":\"1\",\"decidedAt\":0}"),
        ] {
            let parsed = parse_record(raw);
            assert_eq!(parsed, None, "{raw:?}");
            assert!(!evaluate(parsed).active, "{raw:?}");
            assert_eq!(
                evaluate(parsed).status,
                ConsentStatus::NeverAsked,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn a_well_formed_record_round_trips() {
        let r = ConsentRecord::decide(true, 1_800_000_000_000);
        let json = serde_json::to_string(&r).expect("serialise");
        assert_eq!(parse_record(Some(&json)), Some(r));
        // camelCase on the wire, so a hand-inspected state row reads like the
        // rest of the bag.
        assert!(json.contains("\"decidedAt\""), "{json}");
    }

    #[test]
    fn every_state_is_reachable_and_exactly_one_is_active() {
        let states = [
            evaluate(None),
            evaluate(Some(ConsentRecord::decide(true, 1))),
            evaluate(Some(ConsentRecord::decide(false, 1))),
            evaluate(Some(ConsentRecord {
                granted: true,
                version: CONSENT_VERSION + 1,
                decided_at: 1,
            })),
        ];
        assert_eq!(
            states.iter().filter(|c| c.active).count(),
            1,
            "exactly one of the four states may permit sending"
        );
    }

    #[test]
    fn the_status_serialises_kebab_case_for_the_ui() {
        assert_eq!(
            serde_json::to_string(&ConsentStatus::NeverAsked).expect("serialises"),
            "\"never-asked\""
        );
    }
}
