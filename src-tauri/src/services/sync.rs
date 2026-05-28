//! Phase 9 — cloud-sync engine (the local-first core).
//!
//! SundayStage is local-first: the local SQLite is always the running app's
//! source of truth, and cloud sync (Supabase) is best-effort background
//! replication — a Sunday Pro feature; the free tier is fully local.
//!
//! What lives here is the part worth getting exactly right and unit-testing:
//! the **conflict-resolution** decision (last-write-wins with conflict
//! flagging), the **sync gate** that enforces the plan's critical constraint
//! (never sync while a service is live), and the **status** the UI shows. The
//! Supabase backend, auth, the outbox/inbox transport, and realtime presence
//! are deferred — they need a network and an account this environment can't
//! provide.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What to do with one entity given local vs. remote timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Nothing changed on either side since the last sync.
    UpToDate,
    /// Only the local copy changed — push it.
    PushLocal,
    /// Only the remote copy changed — pull it.
    PullRemote,
    /// Both changed since the last sync — flag for the user (default to LWW).
    Conflict,
}

/// Decide what to do for one entity. `last_synced` is the `updated_at` value as
/// of the last successful sync (`None` = never synced).
///
/// "Changed" means the side's `updated_at` is newer than `last_synced`. A
/// never-synced entity that exists on both sides with differing timestamps is a
/// conflict (we can't know they're the same edit); identical timestamps are
/// treated as already in agreement.
pub fn resolve(local_updated: i64, remote_updated: i64, last_synced: Option<i64>) -> Resolution {
    if local_updated == remote_updated {
        return Resolution::UpToDate;
    }
    let base = last_synced.unwrap_or(0);
    let local_changed = local_updated > base;
    let remote_changed = remote_updated > base;
    match (local_changed, remote_changed) {
        (true, true) => Resolution::Conflict,
        (true, false) => Resolution::PushLocal,
        (false, true) => Resolution::PullRemote,
        (false, false) => Resolution::UpToDate,
    }
}

/// Last-write-wins tiebreaker for a flagged conflict: `true` = local is newer
/// (keep local), `false` = remote is newer.
pub fn last_write_wins_keeps_local(local_updated: i64, remote_updated: i64) -> bool {
    local_updated >= remote_updated
}

/// The top-bar sync indicator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/SyncStatus.ts")]
pub enum SyncStatus {
    /// No cloud configured — free tier, fully local.
    LocalOnly,
    /// Synced and idle.
    Synced,
    /// Pushing/pulling changes.
    Syncing,
    /// Cloud enabled but no connection.
    Offline,
    /// One or more unresolved conflicts await the user.
    Conflict,
    /// A service is live — sync is suspended until it ends.
    PausedLive,
}

/// Compute the indicator. Order matters: not-configured and live-paused take
/// precedence so the operator is never told "syncing" mid-service.
pub fn compute_status(
    cloud_enabled: bool,
    online: bool,
    is_live: bool,
    pending: u32,
    conflicts: u32,
) -> SyncStatus {
    if !cloud_enabled {
        return SyncStatus::LocalOnly;
    }
    if is_live {
        return SyncStatus::PausedLive;
    }
    if !online {
        return SyncStatus::Offline;
    }
    if conflicts > 0 {
        return SyncStatus::Conflict;
    }
    if pending > 0 {
        return SyncStatus::Syncing;
    }
    SyncStatus::Synced
}

/// The critical constraint: sync only runs when cloud is on, we're online, and
/// **no service is live** (sync must never impact live performance).
pub fn should_sync(cloud_enabled: bool, online: bool, is_live: bool) -> bool {
    cloud_enabled && online && !is_live
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_detects_each_case() {
        // identical → up to date
        assert_eq!(resolve(100, 100, Some(100)), Resolution::UpToDate);
        // only local moved past last sync
        assert_eq!(resolve(200, 100, Some(100)), Resolution::PushLocal);
        // only remote moved
        assert_eq!(resolve(100, 200, Some(100)), Resolution::PullRemote);
        // both moved since last sync → conflict
        assert_eq!(resolve(200, 300, Some(100)), Resolution::Conflict);
    }

    #[test]
    fn never_synced_differing_is_a_conflict() {
        assert_eq!(resolve(50, 70, None), Resolution::Conflict);
        // never synced but identical → already in agreement
        assert_eq!(resolve(50, 50, None), Resolution::UpToDate);
    }

    #[test]
    fn last_write_wins_picks_newer() {
        assert!(last_write_wins_keeps_local(300, 200));
        assert!(!last_write_wins_keeps_local(200, 300));
        // tie keeps local (deterministic)
        assert!(last_write_wins_keeps_local(200, 200));
    }

    #[test]
    fn status_local_only_when_cloud_disabled() {
        // Even mid-service, no cloud means local-only.
        assert_eq!(
            compute_status(false, true, true, 5, 2),
            SyncStatus::LocalOnly
        );
    }

    #[test]
    fn status_pauses_during_live() {
        assert_eq!(
            compute_status(true, true, true, 3, 0),
            SyncStatus::PausedLive
        );
    }

    #[test]
    fn status_precedence_offline_conflict_syncing_synced() {
        assert_eq!(
            compute_status(true, false, false, 0, 0),
            SyncStatus::Offline
        );
        assert_eq!(
            compute_status(true, true, false, 1, 1),
            SyncStatus::Conflict
        );
        assert_eq!(compute_status(true, true, false, 2, 0), SyncStatus::Syncing);
        assert_eq!(compute_status(true, true, false, 0, 0), SyncStatus::Synced);
    }

    #[test]
    fn sync_is_suspended_while_live() {
        assert!(should_sync(true, true, false));
        assert!(
            !should_sync(true, true, true),
            "must not sync during a live service"
        );
        assert!(!should_sync(false, true, false), "no sync without cloud");
        assert!(!should_sync(true, false, false), "no sync while offline");
    }
}
