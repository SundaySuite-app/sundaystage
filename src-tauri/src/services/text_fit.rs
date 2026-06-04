//! Phase: deep-stage-1 — text auto-fit / reflow.
//!
//! Fixed-size slides overflow the moment a translation runs long or a volunteer
//! pastes a wall of lyrics: today the output font size is *static* (see
//! `bin/output.rs`, which hard-codes `5.5 * text_scale` cqw, and the editor's
//! `textBlockStyle`, which uses `style.size` verbatim). At service time that
//! means text spilling off the bottom of the screen.
//!
//! This module is the cure: a **pure, deterministic** fit algorithm. Given the
//! text, a bounding box (width + height on the virtual 1080 stage), a base/min
//! font size, a step, and an *injected* text-measurement function, it finds the
//! largest size — and the line breaks — that fit inside the box (and an optional
//! max-lines cap), clamping to the minimum when nothing fits.
//!
//! ## Why an injected measurer
//!
//! Real glyph metrics are a *runtime* concern: the browser knows the true pixel
//! width of "Amazing grace" in Inter 700 at 48px; a headless test does not. So
//! the algorithm never measures text itself — it calls a [`Measure`] closure.
//! Production passes a closure backed by the real layout engine (canvas
//! `measureText` in the editor, the window's metrics in the output process); the
//! tests pass a deterministic mock (e.g. "each glyph is `size * 0.5` px wide"),
//! and assert the *search* behaviour without any browser. The TypeScript port
//! (`src/lib/slideEditor/textFit.ts`) implements the identical search so the
//! editor preview and the live output agree on size and breaks.
//!
//! Everything here is total and panic-free: degenerate inputs (empty text, a
//! zero-width box, min > base) resolve to a sane clamped result rather than
//! looping or dividing by zero — a Sunday morning never hangs on a long verse.

/// The measured footprint of a laid-out string at a given font size: how the
/// text wrapped (one entry per visual line) and the total block height in the
/// same stage-px units as the box. The width that drove the wrap is implied by
/// the [`Measure`] closure that produced this.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOut {
    /// The text split into the visual lines it occupies at this size.
    pub lines: Vec<String>,
    /// Total height of the wrapped block in stage-px (lines × line-height).
    pub height: f32,
}

/// A text-measurement function: lay `text` out into the available `max_width`
/// (stage-px) at font `size` (stage-px) and report the wrapped lines + height.
///
/// Implementations MUST be pure for a given input so the fit search is
/// deterministic. The production implementation wraps real glyph metrics; tests
/// wrap a fixed per-glyph width. Hard `\n` in the source text are always honored
/// as line breaks by every implementation.
pub trait Measure {
    fn layout(&self, text: &str, size: f32, max_width: f32) -> LaidOut;
}

/// Blanket impl so a plain closure can be passed as a [`Measure`].
impl<F> Measure for F
where
    F: Fn(&str, f32, f32) -> LaidOut,
{
    fn layout(&self, text: &str, size: f32, max_width: f32) -> LaidOut {
        self(text, size, max_width)
    }
}

/// The box (and optional line cap) the text must fit within. Geometry is in the
/// same stage-px units the [`Measure`] closure reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitBox {
    /// Available width in stage-px (the slot rect width × stage width).
    pub width: f32,
    /// Available height in stage-px (the slot rect height × stage height).
    pub height: f32,
    /// Optional hard cap on visual lines. `None` = bounded only by height.
    pub max_lines: Option<usize>,
}

/// The size-search parameters. `base` is the desired (largest) size; the search
/// shrinks toward `min` in `step` decrements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitParams {
    pub base: f32,
    pub min: f32,
    pub step: f32,
}

impl Default for FitParams {
    fn default() -> Self {
        // Mirrors the historical 64px @1080 body size as the base, shrinking in
        // 2px steps down to a still-legible 18px floor.
        Self {
            base: 64.0,
            min: 18.0,
            step: 2.0,
        }
    }
}

/// The result of fitting: the chosen size and the line breaks at that size.
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    /// The largest size (≥ `min`, ≤ `base`) whose layout fits the box, or `min`
    /// when nothing fits (clamped).
    pub size: f32,
    /// The wrapped lines at the chosen size — what the renderer paints.
    pub lines: Vec<String>,
    /// True when the search bottomed out at `min` and the text still overflows
    /// the box (or the line cap). The renderer can use this to e.g. ellipsize,
    /// but it is never a failure — the slide still shows, clamped.
    pub clamped: bool,
}

/// Does `laid` fit `bx` — both the height and, if set, the line cap?
fn fits(laid: &LaidOut, bx: &FitBox) -> bool {
    if laid.height > bx.height + EPS {
        return false;
    }
    if let Some(max) = bx.max_lines {
        if laid.lines.len() > max {
            return false;
        }
    }
    true
}

/// A hair of slack so floating-point equality (e.g. exactly-fits) counts as fit.
const EPS: f32 = 1e-3;

/// Compute the largest font size and line breaks that fit `text` in `bx`,
/// measured by `measure`, searching from `params.base` down to `params.min`.
///
/// Behaviour (locked by the tests, and matched 1:1 by the TS port):
///   * Text that fits at `base` keeps `base` — no needless shrinking.
///   * Text just over the box shrinks by exactly one `step` (the first size
///     that fits), not all the way down.
///   * Text that never fits clamps to `min` and returns `clamped: true`, still
///     with the wrapped lines so the renderer paints *something*.
///   * `max_lines` is honored at every step (a too-tall layout fails on either
///     height or line count, whichever bites first).
///   * Empty/whitespace text trivially fits at `base` with no lines.
///   * Degenerate params (min ≥ base, step ≤ 0, non-finite) collapse to a single
///     evaluation at the clamped base so the search always terminates.
pub fn fit_text<M: Measure>(text: &str, bx: &FitBox, params: &FitParams, measure: &M) -> FitResult {
    // Empty text occupies nothing — keep the base size, no lines to paint.
    if text.trim().is_empty() {
        return FitResult {
            size: params.base.max(0.0),
            lines: Vec::new(),
            clamped: false,
        };
    }

    // Sanitize the search bounds so the loop is always finite and ordered.
    let base = sane(params.base, 1.0);
    let min = sane(params.min, 1.0).min(base);
    let step = {
        let s = sane(params.step, 1.0);
        if s <= 0.0 {
            // No usable step → evaluate only the base, then the min.
            (base - min).max(1.0)
        } else {
            s
        }
    };

    let mut size = base;
    let mut last = measure.layout(text, size, bx.width);
    // Walk down in `step` decrements; stop at the first size that fits.
    while size > min + EPS {
        if fits(&last, bx) {
            return FitResult {
                size,
                lines: last.lines,
                clamped: false,
            };
        }
        size = (size - step).max(min);
        last = measure.layout(text, size, bx.width);
    }

    // We are at (or below) min. Take it whether or not it fits.
    let clamped = !fits(&last, bx);
    FitResult {
        size: min,
        lines: last.lines,
        clamped,
    }
}

/// Clamp a possibly non-finite / non-positive size to a usable positive value.
fn sane(v: f32, fallback: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic mock measurer: every glyph is `size * char_w` px wide and
    /// each visual line is `size * line_h` px tall. Words wrap greedily at
    /// `max_width`; hard `\n` always break. No browser, no fonts — pure math, so
    /// the *search* is what we are testing, not glyph metrics.
    struct Mock {
        char_w: f32,
        line_h: f32,
    }

    impl Mock {
        fn new() -> Self {
            Self {
                char_w: 0.5,
                line_h: 1.2,
            }
        }
    }

    impl Measure for Mock {
        fn layout(&self, text: &str, size: f32, max_width: f32) -> LaidOut {
            let glyph = size * self.char_w;
            let max_chars = if glyph <= 0.0 {
                usize::MAX
            } else {
                ((max_width / glyph).floor() as usize).max(1)
            };
            let mut lines: Vec<String> = Vec::new();
            for hard in text.split('\n') {
                if hard.is_empty() {
                    lines.push(String::new());
                    continue;
                }
                let mut cur = String::new();
                for word in hard.split(' ') {
                    let candidate = if cur.is_empty() {
                        word.to_string()
                    } else {
                        format!("{cur} {word}")
                    };
                    if candidate.chars().count() <= max_chars || cur.is_empty() {
                        cur = candidate;
                    } else {
                        lines.push(std::mem::take(&mut cur));
                        cur = word.to_string();
                    }
                }
                lines.push(cur);
            }
            let height = lines.len() as f32 * size * self.line_h;
            LaidOut { lines, height }
        }
    }

    fn params() -> FitParams {
        FitParams {
            base: 64.0,
            min: 18.0,
            step: 2.0,
        }
    }

    #[test]
    fn short_text_keeps_base_size() {
        // "Hi" at 64px: glyph 32px, fits a 2000px-wide / 200px-tall box on one
        // line (one line = 64*1.2 = 76.8px ≤ 200). No shrink.
        let bx = FitBox {
            width: 2000.0,
            height: 200.0,
            max_lines: None,
        };
        let r = fit_text("Hi", &bx, &params(), &Mock::new());
        assert_eq!(r.size, 64.0);
        assert_eq!(r.lines, vec!["Hi".to_string()]);
        assert!(!r.clamped);
    }

    #[test]
    fn text_just_over_the_box_shrinks_by_one_step() {
        // Box height fits exactly two lines at 62 but only ~one at 64.
        // At 64: line-h = 76.8; a 2-line layout = 153.6.
        // At 62: line-h = 74.4; a 2-line layout = 148.8.
        // Pick a width that forces two lines at both sizes, and a height that
        // admits the 62 layout but not the 64 one.
        let bx = FitBox {
            width: 360.0,
            height: 150.0,
            max_lines: None,
        };
        // "alpha beta" : 10 chars incl. space.
        // 64: glyph 32, max_chars = 360/32 = 11 → fits on ONE line (76.8 ≤ 150).
        // That wouldn't shrink, so use longer text that wraps to 2 lines.
        let r = fit_text("alpha beta gamma", &bx, &params(), &Mock::new());
        // 64: max_chars 11 → "alpha beta" (10) ok, +gamma overflow → 2 lines →
        //     153.6 > 150 → does NOT fit. Shrink one step to 62.
        // 62: glyph 31, max_chars 11 → still 2 lines → 148.8 ≤ 150 → fits.
        assert_eq!(r.size, 62.0);
        assert_eq!(r.lines.len(), 2);
        assert!(!r.clamped);
    }

    #[test]
    fn very_long_text_clamps_to_min_and_breaks_lines() {
        // A short box that cannot hold the text even at min → clamp to 18, mark
        // clamped, but still return the wrapped lines.
        let bx = FitBox {
            width: 200.0,
            height: 40.0,
            max_lines: None,
        };
        let long = "the quick brown fox jumps over the lazy dog again and again";
        let r = fit_text(long, &bx, &params(), &Mock::new());
        assert_eq!(r.size, 18.0, "clamps to min");
        assert!(r.clamped, "still overflows at min");
        assert!(r.lines.len() > 1, "long text is broken into lines");
    }

    #[test]
    fn max_lines_is_respected() {
        // A tall box (height never binds) but a hard 1-line cap forces a shrink
        // until the text wraps to a single line.
        let bx = FitBox {
            width: 600.0,
            height: 100_000.0,
            max_lines: Some(1),
        };
        let r = fit_text("alpha beta gamma delta", &bx, &params(), &Mock::new());
        // At the chosen size the layout must be exactly one line.
        assert_eq!(r.lines.len(), 1);
        // And it must be the largest such size: one step bigger would wrap to 2.
        let bigger = Mock::new().layout("alpha beta gamma delta", r.size + 2.0, bx.width);
        assert!(bigger.lines.len() > 1 || r.size >= params().base);
    }

    #[test]
    fn max_lines_clamps_when_uncappable() {
        // Even at min the text cannot fit one line in a narrow box → clamp.
        let bx = FitBox {
            width: 30.0,
            height: 100_000.0,
            max_lines: Some(1),
        };
        let r = fit_text("several separate words here", &bx, &params(), &Mock::new());
        assert_eq!(r.size, 18.0);
        assert!(r.clamped);
    }

    #[test]
    fn empty_text_keeps_base_with_no_lines() {
        let bx = FitBox {
            width: 100.0,
            height: 100.0,
            max_lines: None,
        };
        let r = fit_text("   \n ", &bx, &params(), &Mock::new());
        assert_eq!(r.size, 64.0);
        assert!(r.lines.is_empty());
        assert!(!r.clamped);
    }

    #[test]
    fn hard_newlines_are_honored_as_breaks() {
        // A wide/tall box: no wrapping needed, but the two hard-broken lines
        // must both be present at base size.
        let bx = FitBox {
            width: 10_000.0,
            height: 10_000.0,
            max_lines: None,
        };
        let r = fit_text("line one\nline two", &bx, &params(), &Mock::new());
        assert_eq!(r.size, 64.0);
        assert_eq!(
            r.lines,
            vec!["line one".to_string(), "line two".to_string()]
        );
    }

    #[test]
    fn deterministic_same_inputs_same_output() {
        let bx = FitBox {
            width: 480.0,
            height: 220.0,
            max_lines: None,
        };
        let text = "Amazing grace how sweet the sound that saved a wretch like me";
        let a = fit_text(text, &bx, &params(), &Mock::new());
        let b = fit_text(text, &bx, &params(), &Mock::new());
        assert_eq!(a, b);
    }

    #[test]
    fn degenerate_params_do_not_loop_or_panic() {
        let bx = FitBox {
            width: 100.0,
            height: 100.0,
            max_lines: None,
        };
        // min > base, step 0, non-finite base — must terminate with a sane size.
        let weird = FitParams {
            base: f32::NAN,
            min: 100.0,
            step: 0.0,
        };
        let r = fit_text("text", &bx, &weird, &Mock::new());
        assert!(r.size.is_finite() && r.size > 0.0);
    }

    #[test]
    fn never_exceeds_base_even_in_a_huge_box() {
        let bx = FitBox {
            width: 1_000_000.0,
            height: 1_000_000.0,
            max_lines: None,
        };
        let r = fit_text("tiny", &bx, &params(), &Mock::new());
        assert_eq!(r.size, params().base);
    }
}
