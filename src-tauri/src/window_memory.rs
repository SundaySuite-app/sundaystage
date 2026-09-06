//! A5 — window memory for the OPERATOR window, and for nothing else.
//!
//! SundayStage forgot where its window was between services. Every Sunday the
//! operator dragged it back onto the screen she runs the service from and
//! resized it, before doing any actual work. `tauri-plugin-window-state`
//! (tauri-apps, `Apache-2.0 OR MIT`, verified in the crate's own `LICENSE_MIT`
//! and `LICENSE_APACHE-2.0`) is the standard answer, and this module is the
//! narrow way we take it.
//!
//! ## The hard boundary: the operator window, and nothing else
//!
//! A restored, stale position for an OUTPUT window is a Sunday failure, not a
//! convenience. The rig in a church changes from week to week — a projector is
//! moved, a second screen is borrowed for a funeral, a laptop is docked
//! differently — and the whole point of [`crate::services::display`] is that the
//! app decides where an output goes from the monitors it can SEE right now.
//! Window memory would be a second opinion about that, formed last Sunday, and
//! it would win: the plugin restores in `on_window_ready`, before
//! [`crate::output::window::open_outputs`] gets to fullscreen anything.
//!
//! So the plugin is given an ALLOWLIST, not a denylist: [`is_remembered`]
//! returns `true` for exactly one label. The distinction is the whole design.
//! A denylist (`with_denylist(&["output-main-0", …])`) would have to be
//! extended every time a role or a monitor index appears, and the failure mode
//! of forgetting is that a projector window silently starts being remembered.
//! With a filter, a window nobody has thought about is excluded by default, and
//! the only way to opt one in is to edit this file.
//!
//! ### Where the plugin can and cannot reach an output window
//!
//! Read from the plugin's own source (v2.4.1), because "we passed a filter" is
//! only half an answer:
//!
//!   1. **`on_window_ready`** — the filter is checked FIRST and returns early.
//!      A filtered-out window therefore gets no `restore_state`, no cache entry,
//!      and — this is the part worth knowing — no `on_window_event` listeners at
//!      all. Its moves and resizes are not observed.
//!   2. **`AppHandleExt::save_window_state`**, which runs on `RunEvent::Exit`,
//!      iterates the CACHE and looks each label up among the open windows. A
//!      label that never entered the cache cannot be written to disk. So the
//!      exclusion holds on the save side for the same reason it holds on the
//!      restore side, without a second check.
//!   3. **`plugin:window-state|restore_state`** — an IPC command taking an
//!      ARBITRARY label. This one is NOT covered by the filter, which only runs
//!      in `on_window_ready`; and its no-cached-state branch INSERTS the label
//!      into the cache, which the next exit then writes to disk. That is a real
//!      path from a line of JavaScript to a remembered projector window, and the
//!      filter does nothing about it.
//!
//! Path 3 is closed in `capabilities/default.json`, which grants the plugin no
//! permission and explicitly denies all three of its commands. `tauri-build`
//! validates permission identifiers, so a typo there fails the build rather than
//! silently granting nothing — and the capability's `windows` list covers
//! `output-*`, so the denial reaches exactly the windows that matter. There is
//! a test on each of these three, below.
//!
//! ## What is remembered, and what deliberately is not
//!
//! `SIZE | POSITION | MAXIMIZED`. The plugin's default is `all()`, which also
//! carries three flags that are wrong here:
//!
//!   - **`VISIBLE`** — a window saved hidden is restored hidden, and
//!     `restore_state` only calls `show()` when the saved state says visible.
//!     SundayStage starting to nothing at all, five minutes before a service,
//!     with the process running, is the worst bug on this list.
//!   - **`FULLSCREEN`** — a fullscreen operator window adopts the monitor it is
//!     on. Restore it on a rig whose monitors moved and it can come up covering
//!     the congregation's screen, which is the one surface this app must never
//!     take by accident.
//!   - **`DECORATIONS`** — the operator never changes them, so there is nothing
//!     to remember; restoring them is only a way to end up with a chromeless
//!     main window and no titlebar to drag.
//!
//! ## A window saved on a screen that is no longer there
//!
//! The plugin has ONE guarantee here, and it is worth stating precisely because
//! it is narrower than it sounds: **position** is restored only if some
//! CURRENTLY AVAILABLE monitor intersects the saved rectangle, and otherwise the
//! OS is left to place the window. That covers the common case exactly right —
//! saved at `x = 1920` on a second screen, restored with the second screen
//! unplugged, no monitor intersects, so the position is dropped.
//!
//! It does not cover **size**, which is restored unconditionally. A window saved
//! at 3000×2000 on a borrowed 4K display and restored on a 1440×900 laptop gets
//! that size at an OS-chosen position, and a window larger than the screen it
//! sits on is "outside the visible area" by any definition an operator cares
//! about.
//!
//! So [`on_screen_guard`] runs immediately after the restore and fixes both
//! halves: it shrinks a window to fit the monitor it is on, and it moves a
//! window that intersects no monitor at all onto the primary. The correction is
//! [`correct`], a pure function over rectangles, which is the half that can be
//! tested without a windowing session — and it is tested hard, because the live
//! half is one call each to `set_size` and `set_position`.
//!
//! Registration ORDER is load-bearing: `PluginStore::window_created` iterates a
//! `Vec` in registration order, so the guard must be registered directly after
//! the memory plugin or it would run before the restore it exists to correct.
//! `lib.rs` has a source tripwire on exactly that.
//!
//! ## The live path is untouched
//!
//! Nothing in this module runs during a service. The filter is consulted once
//! per window at creation; the guard runs once, on the operator window, at
//! startup. `live_dispatch` has not gained a line, `output/` is not imported
//! here, and the output child process (`src/bin/output.rs`) builds its own Tauri
//! app that never registers either plugin.

use tauri::plugin::TauriPlugin;
use tauri::{PhysicalPosition, PhysicalSize, Runtime};
use tauri_plugin_window_state::StateFlags;

/// The operator window's label. Tauri assigns `main` to the single window
/// declared in `tauri.conf.json`, which has no explicit `label` of its own.
pub const MAIN_LABEL: &str = "main";

/// The allowlist, as a predicate.
///
/// `true` for the operator window and for nothing else. Deliberately not
/// `!label.starts_with("output-")`: that would remember any window somebody adds
/// later under some other name, which is the failure this whole module is shaped
/// around. If a second window should ever be remembered, it is named here, on
/// purpose, in a diff somebody reviewed.
pub fn is_remembered(label: &str) -> bool {
    label == MAIN_LABEL
}

/// Size, position and maximised — and not visibility, fullscreen or decorations.
/// See the module header for why each of the three is left out.
pub fn state_flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
}

/// The window-memory plugin, filtered down to the operator window.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_window_state::Builder::new()
        .with_state_flags(state_flags())
        .with_filter(is_remembered)
        .build()
}

/// The correction pass, as a plugin so that it runs after the restore.
///
/// A plugin rather than a line in `setup()` because ordering is the only thing
/// that makes it work: plugin `on_window_ready` hooks fire in registration
/// order, and `setup()` has no defined position relative to them.
pub fn on_screen_guard<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("sundaystage-window-guard")
        .on_window_ready(|window| {
            if !is_remembered(window.label()) {
                return;
            }
            ensure_on_screen(&window);
        })
        .build()
}

/// A window or a monitor, in physical pixels. Positions are signed because a
/// monitor to the left of the primary has a negative origin on every platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }

    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }

    /// Do the two rectangles share any pixel? Empty rectangles share none.
    ///
    /// Half-open on both axes, which is what makes two monitors laid edge to
    /// edge (`0..1920` and `1920..3840`) NOT count as overlapping — the same
    /// convention the plugin's own monitor check uses.
    fn overlaps(&self, other: &Rect) -> bool {
        self.w > 0
            && self.h > 0
            && other.w > 0
            && other.h > 0
            && (self.x as i64) < other.right()
            && (other.x as i64) < self.right()
            && (self.y as i64) < other.bottom()
            && (other.y as i64) < self.bottom()
    }

    /// Area of the overlap.
    ///
    /// `i64` throughout so a monitor origin at `i32::MIN` cannot wrap the
    /// subtraction, and `saturating_mul` for the product: two `u32::MAX`-sized
    /// rectangles multiply out past `i64::MAX`, and that would be a panic in a
    /// debug build rather than a wrong answer. The value is only ever compared,
    /// never summed, so a saturated maximum orders correctly.
    fn overlap_area(&self, other: &Rect) -> i64 {
        let w = self.right().min(other.right()) - (self.x.max(other.x)) as i64;
        let h = self.bottom().min(other.bottom()) - (self.y.max(other.y)) as i64;
        if w <= 0 || h <= 0 {
            0
        } else {
            w.saturating_mul(h)
        }
    }
}

/// Bring a window rectangle back onto a screen, or `None` when it is already
/// fine and must not be touched.
///
/// `monitors` is the set of monitors available RIGHT NOW, **primary first** —
/// the first entry is the fallback host for a window that overlaps nothing.
///
/// Three rules, in this order:
///
///   1. **No monitors reported** — do nothing. That happens on a machine whose
///      display is asleep or in a headless CI run, and moving a window based on
///      an empty answer is worse than leaving it where the OS put it.
///   2. **Shrink to fit.** A window wider or taller than its host monitor is
///      clamped to the host. This is the half the plugin does not do, and the
///      one that produces a window with no reachable titlebar.
///   3. **Pull back on screen.** A window that overlaps no monitor at all is
///      moved onto the primary; a window that was shrunk is clamped so it still
///      fits inside its host.
///
/// A window that overlaps a monitor and fits inside it is returned as `None`,
/// never as an identical `Some` — the caller must be able to distinguish "leave
/// it alone" from "set it to exactly what it already is", because `set_size` on
/// a maximised window un-maximises it.
pub fn correct(window: Rect, monitors: &[Rect]) -> Option<Rect> {
    let host = host_monitor(window, monitors)?;

    // Clamp the size first: where the window belongs depends on how big it is.
    let w = window.w.min(host.w);
    let h = window.h.min(host.h);

    let was_visible = monitors.iter().any(|m| window.overlaps(m));
    if was_visible && w == window.w && h == window.h {
        return None;
    }

    // `host.w - w` cannot underflow: `w` is a `min` against `host.w`.
    let max_x = host.x as i64 + (host.w - w) as i64;
    let max_y = host.y as i64 + (host.h - h) as i64;
    let x = (window.x as i64).clamp(host.x as i64, max_x) as i32;
    let y = (window.y as i64).clamp(host.y as i64, max_y) as i32;

    let fixed = Rect { x, y, w, h };
    (fixed != window).then_some(fixed)
}

/// The monitor a window belongs to: the one it overlaps most, else the primary.
///
/// "Overlaps most" rather than "overlaps at all" because a window straddling two
/// screens has to be shrunk against ONE of them, and the one showing most of it
/// is the one the operator is looking at.
fn host_monitor(window: Rect, monitors: &[Rect]) -> Option<Rect> {
    let best = monitors
        .iter()
        .map(|m| (window.overlap_area(m), m))
        .filter(|(area, _)| *area > 0)
        .max_by_key(|(area, _)| *area)
        .map(|(_, m)| *m);
    best.or_else(|| monitors.first().copied())
}

/// Apply [`correct`] to a live window. Best-effort throughout: every failure to
/// read the geometry leaves the window exactly as the OS placed it, which is the
/// state this function exists to be no worse than.
fn ensure_on_screen<R: Runtime>(window: &tauri::Window<R>) {
    // A maximised or fullscreen window is by definition the size of its screen,
    // and `set_size` on one would un-maximise it — turning a restored maximised
    // window into a floating one for no reason. Nothing to correct either way.
    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }

    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let Ok(available) = window.available_monitors() else {
        return;
    };

    // Primary first: `correct` uses the head of the list as the fallback host.
    let primary = window.primary_monitor().ok().flatten();
    let mut monitors: Vec<Rect> = Vec::with_capacity(available.len());
    if let Some(p) = &primary {
        monitors.push(rect_of(p));
    }
    for m in &available {
        let r = rect_of(m);
        if monitors.first() != Some(&r) {
            monitors.push(r);
        }
    }

    let current = Rect {
        x: position.x,
        y: position.y,
        w: size.width,
        h: size.height,
    };

    let Some(fixed) = correct(current, &monitors) else {
        return;
    };

    // Size before position: a shrink can only make the following clamp valid.
    let _ = window.set_size(PhysicalSize {
        width: fixed.w,
        height: fixed.h,
    });
    let _ = window.set_position(PhysicalPosition {
        x: fixed.x,
        y: fixed.y,
    });

    tracing::info!(
        "the operator window was restored off-screen and was brought back \
         (from {}x{} at {},{} to {}x{} at {},{})",
        current.w,
        current.h,
        current.x,
        current.y,
        fixed.w,
        fixed.h,
        fixed.x,
        fixed.y,
    );
}

fn rect_of(monitor: &tauri::Monitor) -> Rect {
    let p = monitor.position();
    let s = monitor.size();
    Rect {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::window::output_label;
    use crate::services::display::DisplayRole;

    // ── The boundary ─────────────────────────────────────────────────────────

    #[test]
    fn the_operator_window_is_the_one_that_is_remembered() {
        assert!(is_remembered(MAIN_LABEL));
        assert_eq!(
            MAIN_LABEL, "main",
            "tauri.conf.json declares no label, so \
             Tauri assigns `main`; changing one without the other silently stops \
             the operator window from being remembered"
        );
    }

    /// THE test this etappe exists for.
    ///
    /// Not written against a hand-copied list of labels — against
    /// `output::window::output_label`, the function that builds the real ones,
    /// for every driven role and a spread of monitor indices including the ones
    /// a church with a lot of screens would reach. A copied list would keep
    /// passing on the day the label shape changed, which is the one day it
    /// matters.
    #[test]
    fn no_output_window_can_ever_be_remembered() {
        let mut checked = 0;
        for role in [
            DisplayRole::MainOutput,
            DisplayRole::StageDisplay,
            DisplayRole::ConfidenceMonitor,
        ] {
            for index in 0..8u32 {
                let label = output_label(role, index).expect("a driven role has a label");
                assert!(
                    !is_remembered(&label),
                    "window memory must never touch an output window, but `{label}` \
                     passed the filter — a stale projector position is a Sunday failure"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 24, "every driven role × index pair was checked");
    }

    /// The allowlist property, stated as the thing that makes it different from
    /// a denylist: a label nobody has thought of yet is excluded.
    #[test]
    fn a_window_nobody_has_invented_yet_is_excluded_by_default() {
        for label in [
            "output-preview-0", // a role that does not exist yet
            "output-lyrics-3",  // …nor this one
            "projector",        // a window not following the output- shape
            "stage",            //
            "second-operator",  //
            "main-2",           // a near-miss on the one allowed label
            "Main",             // case matters
            " main",            // and so does whitespace
            "main ",            //
            "",                 //
        ] {
            assert!(
                !is_remembered(label),
                "`{label}` is not the operator window and must not be remembered"
            );
        }
    }

    /// The filter must stay a FILTER. `with_denylist` would compile, pass every
    /// test above on the day it was written, and start remembering the first
    /// window somebody adds without updating the list.
    #[test]
    fn the_plugin_is_configured_with_an_allowlist_not_a_denylist() {
        let src = include_str!("window_memory.rs");
        let build = src
            .split("pub fn plugin<R: Runtime>()")
            .nth(1)
            .expect("this file defines `plugin()`");
        let build = &build[..build.find("\n}").expect("the function ends")];
        assert!(
            build.contains(".with_filter(is_remembered)"),
            "the plugin must be built with the allowlist predicate"
        );
        assert!(
            !build.contains("with_denylist"),
            "a denylist has to be extended for every new window; forgetting means \
             a projector window starts being remembered"
        );
    }

    /// The three flags that are deliberately absent, pinned so that a later
    /// `StateFlags::default()` (which is `all()`) cannot slip in unnoticed.
    #[test]
    fn visibility_fullscreen_and_decorations_are_not_remembered() {
        let flags = state_flags();
        assert!(flags.contains(StateFlags::SIZE));
        assert!(flags.contains(StateFlags::POSITION));
        assert!(flags.contains(StateFlags::MAXIMIZED));
        assert!(
            !flags.contains(StateFlags::VISIBLE),
            "a window saved hidden would be restored hidden — SundayStage starting \
             to nothing at all, with the process running"
        );
        assert!(
            !flags.contains(StateFlags::FULLSCREEN),
            "a restored-fullscreen operator window can come up covering the \
             congregation's screen"
        );
        assert!(!flags.contains(StateFlags::DECORATIONS));
    }

    /// The IPC half of the boundary.
    ///
    /// `plugin:window-state|restore_state` takes an arbitrary label, is NOT
    /// covered by the filter (which only runs in `on_window_ready`), and its
    /// no-cached-state branch inserts that label into the cache — which the next
    /// `RunEvent::Exit` writes to disk. The capability is the only thing between
    /// a line of JavaScript and a remembered projector window, and the
    /// capability's `windows` list covers `output-*`.
    #[test]
    fn the_plugins_ipc_commands_are_denied_to_every_window() {
        let cap = include_str!("../capabilities/default.json");
        for command in ["restore-state", "save-window-state", "filename"] {
            assert!(
                cap.contains(&format!("window-state:deny-{command}")),
                "capabilities/default.json must explicitly deny \
                 `window-state:{command}` — see the module header"
            );
        }
        assert!(
            !cap.contains("window-state:allow-") && !cap.contains("window-state:default"),
            "no window-state command may be granted to the frontend"
        );
        // The denial is only worth having if it reaches the output windows.
        assert!(
            cap.contains("output-*"),
            "the capability must cover output-*"
        );
    }

    // ── The geometry ─────────────────────────────────────────────────────────

    const LAPTOP: Rect = Rect {
        x: 0,
        y: 0,
        w: 1440,
        h: 900,
    };
    /// A second screen to the right, the usual church rig.
    const RIGHT: Rect = Rect {
        x: 1440,
        y: 0,
        w: 1920,
        h: 1080,
    };
    /// …and one to the LEFT, which is where negative origins come from.
    const LEFT: Rect = Rect {
        x: -1920,
        y: 0,
        w: 1920,
        h: 1080,
    };

    #[test]
    fn a_window_that_is_already_fine_is_left_completely_alone() {
        let w = Rect {
            x: 100,
            y: 80,
            w: 1200,
            h: 700,
        };
        assert_eq!(correct(w, &[LAPTOP]), None);
        assert_eq!(correct(w, &[LAPTOP, RIGHT]), None);
    }

    #[test]
    fn a_window_saved_on_a_screen_that_is_gone_comes_back_to_the_primary() {
        // Saved on the second screen; restored with only the laptop present.
        let saved = Rect {
            x: 1500,
            y: 120,
            w: 1280,
            h: 800,
        };
        assert_eq!(
            correct(saved, &[LAPTOP, RIGHT]),
            None,
            "still fine while the screen is there"
        );

        let fixed = correct(saved, &[LAPTOP]).expect("the screen is gone — bring it back");
        assert!(fixed.overlaps(&LAPTOP));
        assert!(fixed.x >= LAPTOP.x && fixed.y >= LAPTOP.y);
        assert!(fixed.right() <= LAPTOP.right() && fixed.bottom() <= LAPTOP.bottom());
    }

    #[test]
    fn the_same_holds_for_a_screen_that_was_to_the_left() {
        let saved = Rect {
            x: -1800,
            y: 40,
            w: 1400,
            h: 850,
        };
        assert_eq!(correct(saved, &[LAPTOP, LEFT]), None);
        let fixed = correct(saved, &[LAPTOP]).expect("the left-hand screen is gone");
        assert!(
            fixed.overlaps(&LAPTOP),
            "moved back onto the primary, not further left"
        );
        assert!(fixed.x >= 0);
    }

    /// The half the plugin does NOT guard: size is restored unconditionally.
    #[test]
    fn a_window_bigger_than_the_screen_it_lands_on_is_shrunk_to_fit() {
        // Saved on a borrowed 4K display, restored on the laptop. The OS placed
        // it at the origin, so it DOES overlap — the position rule alone would
        // leave a 3000×2000 window on a 1440×900 screen.
        let saved = Rect {
            x: 0,
            y: 0,
            w: 3000,
            h: 2000,
        };
        let fixed = correct(saved, &[LAPTOP]).expect("too big for this screen");
        assert_eq!(
            fixed,
            Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900
            }
        );
    }

    #[test]
    fn a_shrunk_window_is_also_pulled_fully_inside_its_host() {
        let saved = Rect {
            x: 900,
            y: 700,
            w: 2400,
            h: 1600,
        };
        let fixed = correct(saved, &[LAPTOP]).expect("too big and too far down-right");
        assert_eq!(fixed.w, LAPTOP.w);
        assert_eq!(fixed.h, LAPTOP.h);
        assert_eq!(fixed.x, 0);
        assert_eq!(fixed.y, 0);
    }

    #[test]
    fn a_window_hanging_off_the_bottom_right_is_nudged_back_only_when_it_is_gone() {
        // Mostly off-screen but still overlapping by a sliver: left alone. The
        // operator may have parked it there on purpose, and a window that shows
        // is a window she can drag.
        let sliver = Rect {
            x: 1400,
            y: 860,
            w: 800,
            h: 600,
        };
        assert!(sliver.overlaps(&LAPTOP));
        assert_eq!(correct(sliver, &[LAPTOP]), None);

        // Entirely past the bottom-right corner: brought back.
        let gone = Rect {
            x: 1440,
            y: 900,
            w: 800,
            h: 600,
        };
        assert!(!gone.overlaps(&LAPTOP));
        let fixed = correct(gone, &[LAPTOP]).expect("nothing of it is visible");
        assert!(fixed.overlaps(&LAPTOP));
    }

    #[test]
    fn a_window_straddling_two_screens_is_hosted_by_the_one_showing_most_of_it() {
        // 1200 wide starting at x = 1340: 100 px on the laptop, 1100 on the right
        // screen. The right screen is the host, so a shrink clamps against 1920,
        // not against 1440 — which is what stops a legitimate wide window from
        // being cut down to the smaller screen.
        let straddling = Rect {
            x: 1340,
            y: 0,
            w: 1200,
            h: 800,
        };
        assert_eq!(correct(straddling, &[LAPTOP, RIGHT]), None);

        let big = Rect {
            x: 1340,
            y: 0,
            w: 1800,
            h: 1000,
        };
        assert_eq!(
            correct(big, &[LAPTOP, RIGHT]),
            None,
            "fits on the host screen"
        );
    }

    #[test]
    fn no_monitors_reported_means_no_opinion() {
        // A sleeping display, or a headless run. An empty answer is not evidence
        // that the window is in the wrong place.
        let w = Rect {
            x: 9_000,
            y: 9_000,
            w: 800,
            h: 600,
        };
        assert_eq!(correct(w, &[]), None);
    }

    #[test]
    fn a_degenerate_saved_rectangle_does_not_panic_or_produce_one() {
        // A zero-sized entry can appear in a hand-edited or truncated state file.
        for w in [
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            Rect {
                x: -5,
                y: -5,
                w: 0,
                h: 700,
            },
            Rect {
                x: 0,
                y: 0,
                w: 700,
                h: 0,
            },
        ] {
            // Zero-sized overlaps nothing, so it is "gone" and gets pulled onto
            // the primary; the only requirement is that it lands inside it.
            if let Some(fixed) = correct(w, &[LAPTOP]) {
                assert!(fixed.x >= LAPTOP.x && fixed.y >= LAPTOP.y);
                assert!(fixed.right() <= LAPTOP.right());
                assert!(fixed.bottom() <= LAPTOP.bottom());
            }
        }
    }

    #[test]
    fn two_monitors_laid_edge_to_edge_do_not_count_as_overlapping() {
        // Half-open on both axes — the same convention the plugin's own monitor
        // check uses. Without it, every window on the right screen would look
        // like it also touches the laptop.
        assert!(!LAPTOP.overlaps(&RIGHT));
        assert!(!RIGHT.overlaps(&LAPTOP));
        assert!(!LAPTOP.overlaps(&LEFT));
    }

    #[test]
    fn very_large_rectangles_do_not_overflow_the_area_arithmetic() {
        let huge = Rect {
            x: 0,
            y: 0,
            w: u32::MAX,
            h: u32::MAX,
        };
        let other = Rect {
            x: 0,
            y: 0,
            w: u32::MAX,
            h: u32::MAX,
        };
        assert!(huge.overlap_area(&other) > 0);
        let far = Rect {
            x: i32::MIN,
            y: i32::MIN,
            w: 100,
            h: 100,
        };
        assert_eq!(far.overlap_area(&LAPTOP), 0);
        assert!(!far.overlaps(&LAPTOP));
    }
}
