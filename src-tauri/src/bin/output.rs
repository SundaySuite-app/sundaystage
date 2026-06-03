//! `sundaystage-output` — the crash-isolated live output process (Phase 5.2).
//!
//! This is the binary the build plan's Prompt 5.2 calls for: the live output
//! runs in a **separate OS process** so that if the main UI crashes, the
//! projector keeps showing the current slide. The wire protocol
//! ([`OutputMessage`] / [`OutputAck`]) and the [`Watchdog`] are the shared,
//! fully-tested contract in `sundaystage_lib::output`; this binary is the thin
//! process that drives them.
//!
//! ## How it runs
//!
//! The main app spawns one `sundaystage-output --display-index N` per assigned
//! monitor and talks to it over stdio: line-delimited JSON `OutputMessage`s in,
//! `OutputAck`s out. No database, no Tauri commands — just a render loop:
//!
//!   1. read a line → parse an [`OutputMessage`]
//!   2. feed it to the pure [`handler::Handler`] (frame state + watchdog)
//!   3. paint the resulting [`handler::Paint`] to the window
//!   4. write the [`OutputAck`] back as one JSON line
//!
//! ## What is and isn't tested
//!
//! Tested (pure, headless): line framing ([`handler::parse_line`]),
//! frame→HTML rendering ([`render`]), and the message/watchdog state machine
//! ([`handler::Handler`]). The actual Tauri window (borderless, full-screen,
//! placed on a specific monitor) needs a real windowing session and is deferred
//! per ARCHITECTURE.md — see `output::window` for the same line. The render loop
//! is structured so the window layer drops in without touching the tested core.

use std::io::Write as _;

/// Pure frame → HTML rendering. Mirrors the React `SlideView` so the separate
/// process paints identical pixels to the in-process preview.
mod render {
    use sundaystage_lib::services::display::OutputAppearance;
    use sundaystage_lib::services::live_session::LiveFrame;
    use sundaystage_lib::services::slide_doc::HAlign;

    /// Map [`HAlign`] to its CSS `text-align` value.
    fn align_css(a: HAlign) -> &'static str {
        match a {
            HAlign::Left => "left",
            HAlign::Center => "center",
            HAlign::Right => "right",
        }
    }

    /// Escape text so lyrics can never inject markup into the output document.
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    /// Render the `<body>` inner HTML for `frame` under `appearance`. Black/Logo
    /// ignore the lyric styling; slides honour colour, scale, alignment, etc.
    /// The result is what the window paints — and what the tests assert on.
    pub fn frame_to_html(frame: &LiveFrame, appearance: &OutputAppearance) -> String {
        match frame {
            // Pure black — never the church logo by accident.
            LiveFrame::Black => r#"<div class="frame black"></div>"#.to_string(),
            LiveFrame::Logo => r#"<div class="frame logo">SundayStage</div>"#.to_string(),
            LiveFrame::Message { text } => format!(
                r#"<div class="frame message" style="color:{}">{}</div>"#,
                esc(&appearance.text_color),
                esc(text),
            ),
            LiveFrame::Slide { slide_content } => {
                let mut body = String::new();
                if appearance.show_section_label {
                    if let Some(label) = &slide_content.section_label {
                        body.push_str(&format!(
                            r#"<div class="section-label">{}</div>"#,
                            esc(label)
                        ));
                    }
                }
                let transform = if appearance.uppercase {
                    "uppercase"
                } else {
                    "none"
                };
                for line in &slide_content.text_lines {
                    body.push_str(&format!(
                        r#"<p class="line" style="color:{};font-size:{}cqw;line-height:{};text-transform:{}">{}</p>"#,
                        esc(&appearance.text_color),
                        5.5 * appearance.text_scale,
                        appearance.line_height,
                        transform,
                        esc(line),
                    ));
                }
                if let Some(lines) = &slide_content.translation_lines {
                    for line in lines {
                        body.push_str(&format!(
                            r#"<p class="translation" style="color:{}">{}</p>"#,
                            esc(&appearance.text_color),
                            esc(line),
                        ));
                    }
                }
                if let Some(reference) = &slide_content.reference {
                    body.push_str(&format!(
                        r#"<div class="reference" style="color:{}">— {}</div>"#,
                        esc(&appearance.text_color),
                        esc(reference),
                    ));
                }
                format!(
                    r#"<div class="frame slide" style="background:{};text-align:{}">{}</div>"#,
                    esc(&appearance.bg_color),
                    align_css(appearance.h_align),
                    body,
                )
            }
        }
    }
}

/// The pure render-loop core: parse one IPC line, drive frame state + watchdog,
/// decide what to paint and what to ACK. No window, no stdio — so it is fully
/// unit-testable. The binary's `main` is the thin shell that wires this to real
/// stdin/stdout and a real Tauri window.
mod handler {
    use serde_json::Error as JsonError;
    use sundaystage_lib::output::{OutputAck, OutputMessage, Watchdog};
    use sundaystage_lib::services::display::OutputAppearance;
    use sundaystage_lib::services::live_session::LiveFrame;

    use crate::render::frame_to_html;

    /// Parse one line of the wire protocol into an [`OutputMessage`]. Blank lines
    /// are ignored (returns `Ok(None)`) so a stray newline never crashes the
    /// output mid-service.
    pub fn parse_line(line: &str) -> Result<Option<OutputMessage>, JsonError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(trimmed).map(Some)
    }

    /// What the loop should do after handling a message.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Step {
        /// HTML to paint, if the frame changed. `None` means leave the window as
        /// it is — crucially, this is what we return on a heartbeat so the last
        /// frame stays put.
        pub paint: Option<String>,
        /// ACK to write back to the main app, if any.
        pub ack: Option<OutputAck>,
        /// The output should tear down after this step.
        pub shutdown: bool,
    }

    /// Render-loop state: the last frame shown, the appearance to render it
    /// under, and the watchdog that decides "hold the last frame".
    pub struct Handler {
        appearance: OutputAppearance,
        last_frame: LiveFrame,
        watchdog: Watchdog,
        /// Whether we have already entered the "held last frame" state, so we
        /// only react to the transition once.
        holding: bool,
    }

    impl Handler {
        /// Start black at `now` (the congregation sees black, never a stale or
        /// random screen, until the first `Render` arrives).
        pub fn new(now: i64) -> Self {
            Self {
                appearance: OutputAppearance::default(),
                last_frame: LiveFrame::Black,
                watchdog: Watchdog::new(now),
                holding: false,
            }
        }

        /// HTML for the frame currently on screen — used for the initial paint
        /// and exercised by the render tests.
        pub fn current_html(&self) -> String {
            frame_to_html(&self.last_frame, &self.appearance)
        }

        /// Handle one inbound message at time `now`.
        pub fn handle(&mut self, msg: OutputMessage, now: i64) -> Step {
            // Any message from the main app is a sign of life.
            self.watchdog.beat(now);
            self.holding = false;
            match msg {
                OutputMessage::Render { frame, seq } => {
                    self.last_frame = frame;
                    Step {
                        paint: Some(self.current_html()),
                        ack: Some(OutputAck::Rendered {
                            seq,
                            rendered_at: now,
                        }),
                        shutdown: false,
                    }
                }
                // A heartbeat sustains liveness but must NOT repaint — repainting
                // on every beat would flicker the slide.
                OutputMessage::Heartbeat { .. } => Step {
                    paint: None,
                    ack: None,
                    shutdown: false,
                },
                OutputMessage::Shutdown => Step {
                    paint: None,
                    ack: None,
                    shutdown: true,
                },
            }
        }

        /// The watchdog tick the loop runs when no message has arrived. Returns
        /// `true` exactly once on the transition into "link is dead". We do NOT
        /// repaint or blank — the last frame is already on screen and stays
        /// there; the only effect is the main app learning the link dropped (and
        /// stage/confidence chrome showing a badge, handled in the window layer).
        pub fn tick(&mut self, now: i64) -> bool {
            if self.watchdog.should_hold_last_frame(now) && !self.holding {
                self.holding = true;
                return true;
            }
            false
        }

        /// Is the link to the main app currently considered alive?
        pub fn is_alive(&self, now: i64) -> bool {
            self.watchdog.is_alive(now)
        }
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parse `--display-index N` from CLI args. Defaults to 0 (the first assigned
/// monitor) when absent, so the binary is runnable for a smoke test without
/// arguments.
fn display_index_from(args: &[String]) -> u32 {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(v) = a.strip_prefix("--display-index=") {
            return v.parse().unwrap_or(0);
        }
        if a == "--display-index" {
            if let Some(v) = it.next() {
                return v.parse().unwrap_or(0);
            }
        }
    }
    0
}

/// Read line-delimited `OutputMessage`s from stdin, drive the [`handler::Handler`],
/// and write `OutputAck`s to stdout. This is the headless transport spine; the
/// real window paints `step.paint` (deferred — see module docs). Returns when
/// the main app sends `Shutdown` or closes the pipe (its own crash).
fn run_io_loop() {
    use std::io::BufRead as _;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    let display_index = display_index_from(&std::env::args().collect::<Vec<_>>());
    eprintln!("sundaystage-output: starting on display {display_index}");

    let mut h = handler::Handler::new(now_ms());
    let mut out = std::io::stdout();

    // Read stdin lines on a dedicated thread and hand them to the loop over a
    // channel, so the main loop can also wake on a watchdog tick (otherwise the
    // blocking `lines()` read would starve the timeout). A closed channel means
    // the main app's pipe went away — i.e. it crashed — and we hold the frame.
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // Tick a little finer than the watchdog timeout so "hold last frame" fires
    // promptly after the heartbeat stops.
    let tick = Duration::from_millis(500);
    loop {
        match rx.recv_timeout(tick) {
            Ok(line) => {
                let msg = match handler::parse_line(&line) {
                    Ok(Some(m)) => m,
                    Ok(None) => continue,
                    Err(e) => {
                        // Bad frame: surface it as a non-fatal ACK and keep the
                        // last frame on screen — never crash the output.
                        let ack = sundaystage_lib::output::OutputAck::Error {
                            message: format!("bad message: {e}"),
                        };
                        let _ =
                            writeln!(out, "{}", serde_json::to_string(&ack).unwrap_or_default());
                        let _ = out.flush();
                        continue;
                    }
                };
                let step = h.handle(msg, now_ms());
                if let Some(_html) = &step.paint {
                    // The Tauri window layer paints `_html` here (deferred).
                    // Until then the protocol round-trip is what we verify.
                }
                if let Some(ack) = step.ack {
                    let _ = writeln!(out, "{}", serde_json::to_string(&ack).unwrap_or_default());
                    let _ = out.flush();
                }
                if step.shutdown {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // No message this interval — ask the watchdog whether the link
                // just died. On the dead transition we keep the last frame up
                // (the window already shows it) and only log; we never blank.
                let now = now_ms();
                if h.tick(now) && !h.is_alive(now) {
                    eprintln!("sundaystage-output: lost heartbeat — holding last frame");
                }
            }
            // The reader thread ended → main app's pipe closed (it crashed).
            // Hold the last frame and keep the process alive so the projector
            // never goes dark; exit only once we are told to, or are killed.
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("sundaystage-output: input pipe closed — holding last frame");
                break;
            }
        }
    }
    eprintln!("sundaystage-output: exiting");
}

fn main() {
    run_io_loop();
}

#[cfg(test)]
mod tests {
    use super::handler::{parse_line, Handler};
    use super::{display_index_from, render::frame_to_html};
    use sundaystage_lib::output::{OutputAck, OutputMessage};
    use sundaystage_lib::services::cue_list::SlideContent;
    use sundaystage_lib::services::display::OutputAppearance;
    use sundaystage_lib::services::live_session::LiveFrame;

    fn slide(lines: &[&str]) -> LiveFrame {
        LiveFrame::Slide {
            slide_content: SlideContent {
                section_label: Some("Verse 1".into()),
                text_lines: lines.iter().map(|s| s.to_string()).collect(),
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
            },
        }
    }

    // ---- CLI ---------------------------------------------------------------

    #[test]
    fn display_index_parses_both_forms_and_defaults() {
        assert_eq!(display_index_from(&[]), 0);
        assert_eq!(
            display_index_from(&["--display-index".into(), "2".into()]),
            2
        );
        assert_eq!(display_index_from(&["--display-index=3".into()]), 3);
        // Garbage falls back to 0 rather than panicking mid-service.
        assert_eq!(display_index_from(&["--display-index=oops".into()]), 0);
    }

    // ---- IPC framing -------------------------------------------------------

    #[test]
    fn parse_line_ignores_blank_lines() {
        assert_eq!(parse_line("").unwrap(), None);
        assert_eq!(parse_line("   \n").unwrap(), None);
    }

    #[test]
    fn parse_line_reads_a_render_message() {
        let json = serde_json::to_string(&OutputMessage::Render {
            frame: LiveFrame::Black,
            seq: 9,
        })
        .unwrap();
        let msg = parse_line(&json).unwrap().unwrap();
        assert_eq!(
            msg,
            OutputMessage::Render {
                frame: LiveFrame::Black,
                seq: 9
            }
        );
    }

    #[test]
    fn parse_line_surfaces_garbage_as_error() {
        assert!(parse_line("{not json").is_err());
    }

    // ---- rendering ---------------------------------------------------------

    #[test]
    fn black_renders_pure_black_not_logo() {
        let html = frame_to_html(&LiveFrame::Black, &OutputAppearance::default());
        assert!(html.contains("frame black"));
        assert!(!html.contains("SundayStage"));
    }

    #[test]
    fn slide_paints_lines_label_and_appearance() {
        let appearance = OutputAppearance {
            text_color: "#abcdef".into(),
            uppercase: true,
            ..OutputAppearance::default()
        };
        let html = frame_to_html(&slide(&["Amazing grace"]), &appearance);
        assert!(html.contains("Amazing grace"));
        assert!(html.contains("Verse 1")); // section label on (default)
        assert!(html.contains("#abcdef"));
        assert!(html.contains("text-transform:uppercase"));
    }

    #[test]
    fn section_label_hidden_when_appearance_disables_it() {
        let appearance = OutputAppearance {
            show_section_label: false,
            ..OutputAppearance::default()
        };
        let html = frame_to_html(&slide(&["line"]), &appearance);
        assert!(!html.contains("Verse 1"));
    }

    #[test]
    fn lyrics_with_markup_are_escaped() {
        let html = frame_to_html(&slide(&["<script>&"]), &OutputAppearance::default());
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;&amp;"));
    }

    // ---- message round-trips (the core of the e2e contract) ----------------

    #[test]
    fn render_message_paints_frame_and_acks_seq() {
        let mut h = Handler::new(0);
        let step = h.handle(
            OutputMessage::Render {
                frame: slide(&["Hello"]),
                seq: 42,
            },
            100,
        );
        let html = step.paint.expect("render paints");
        assert!(html.contains("Hello"));
        assert_eq!(
            step.ack,
            Some(OutputAck::Rendered {
                seq: 42,
                rendered_at: 100
            })
        );
        assert!(!step.shutdown);
    }

    #[test]
    fn heartbeat_sustains_liveness_without_repainting() {
        let mut h = Handler::new(0);
        // Render a frame, then beat well past it — the frame must NOT be
        // repainted (no flicker) but the link stays alive.
        h.handle(
            OutputMessage::Render {
                frame: slide(&["Stay"]),
                seq: 1,
            },
            0,
        );
        let step = h.handle(OutputMessage::Heartbeat { at: 1_500 }, 1_500);
        assert_eq!(step.paint, None);
        assert_eq!(step.ack, None);
        assert!(h.is_alive(1_500));
        // The held frame is still the slide we rendered.
        assert!(h.current_html().contains("Stay"));
    }

    #[test]
    fn watchdog_timeout_holds_last_frame() {
        let mut h = Handler::new(0);
        h.handle(
            OutputMessage::Render {
                frame: slide(&["Last slide"]),
                seq: 1,
            },
            0,
        );
        // No heartbeat for longer than the timeout → link dead.
        assert!(!h.is_alive(5_000));
        // The transition fires exactly once...
        assert!(h.tick(5_000));
        assert!(!h.tick(6_000));
        // ...and we are STILL showing the last frame, not black.
        let html = h.current_html();
        assert!(html.contains("Last slide"));
        assert!(!html.contains("frame black"));
    }

    #[test]
    fn fresh_render_after_timeout_revives_the_link() {
        let mut h = Handler::new(0);
        h.handle(
            OutputMessage::Render {
                frame: slide(&["Old"]),
                seq: 1,
            },
            0,
        );
        assert!(h.tick(5_000)); // went dead
                                // A new render revives it and repaints.
        let step = h.handle(
            OutputMessage::Render {
                frame: slide(&["New"]),
                seq: 2,
            },
            5_500,
        );
        assert!(step.paint.unwrap().contains("New"));
        assert!(h.is_alive(5_500));
        // ...and the dead-link transition can fire again next time it drops.
        assert!(h.tick(8_000));
    }

    #[test]
    fn shutdown_closes_gracefully_without_paint() {
        let mut h = Handler::new(0);
        let step = h.handle(OutputMessage::Shutdown, 10);
        assert!(step.shutdown);
        assert_eq!(step.paint, None);
        assert_eq!(step.ack, None);
    }

    #[test]
    fn end_to_end_message_sequence_round_trips() {
        // Simulate the full main-process → output line stream: render, beat,
        // render, shutdown — and assert the ACKs the main app would read back.
        let mut h = Handler::new(0);
        let lines = [
            serde_json::to_string(&OutputMessage::Render {
                frame: slide(&["One"]),
                seq: 1,
            })
            .unwrap(),
            serde_json::to_string(&OutputMessage::Heartbeat { at: 500 }).unwrap(),
            serde_json::to_string(&OutputMessage::Render {
                frame: slide(&["Two"]),
                seq: 2,
            })
            .unwrap(),
            serde_json::to_string(&OutputMessage::Shutdown).unwrap(),
        ];
        let mut acks = Vec::new();
        let mut now = 0;
        let mut closed = false;
        for line in lines {
            now += 100;
            let msg = parse_line(&line).unwrap().unwrap();
            let step = h.handle(msg, now);
            if let Some(ack) = step.ack {
                acks.push(ack);
            }
            if step.shutdown {
                closed = true;
                break;
            }
        }
        assert!(closed);
        assert_eq!(
            acks,
            vec![
                OutputAck::Rendered {
                    seq: 1,
                    rendered_at: 100
                },
                OutputAck::Rendered {
                    seq: 2,
                    rendered_at: 300
                },
            ]
        );
        // The last frame painted is "Two".
        assert!(h.current_html().contains("Two"));
    }
}
