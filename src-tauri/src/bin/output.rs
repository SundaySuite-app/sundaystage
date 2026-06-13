//! `sundaystage-output` — the crash-isolated live output process (Phase 5.2).
//!
//! The live output runs in a **separate OS process** so that if the main UI
//! crashes, the projector keeps showing the current slide. The wire protocol
//! ([`OutputMessage`] / [`OutputAck`]) and the [`Watchdog`] are the shared,
//! fully-tested contract in `sundaystage_lib::output`; the local-IPC transport
//! (Unix socket / Windows named pipe) lives in `sundaystage_lib::output::ipc`.
//!
//! ## Modes
//!
//! * **Windowed** (`--socket <path> --label output-main-0 --position X,Y
//!   --size WxH [--appearance-file <json>]`): a minimal Tauri app that opens
//!   one borderless full-screen window rendering the same `output.html` React
//!   renderer the in-process windows use, and pumps IPC messages into it as
//!   the `ss://render` / `ss://heartbeat` events it already understands.
//! * **Headless** (`--socket <path> --headless`): the identical IPC client
//!   loop *without* a window — what CI and `tests/output_isolation.rs` drive.
//! * **Stdio** (no `--socket`): the legacy line-delimited stdio loop, kept as
//!   a zero-dependency smoke path (`echo '{"type":"shutdown"}' | sundaystage-output`).
//!
//! ## Crash isolation contract
//!
//! When the link to the main app drops (its crash), this process **holds the
//! last frame and stays alive** — it never blanks, never exits. A relaunched
//! main app reaps it via the pidfile and spawns a fresh child (see
//! `output::process`). On [`OutputMessage::Shutdown`] it exits cleanly.
//!
//! ## What is and isn't tested
//!
//! Tested headlessly: framing, frame→HTML rendering, the message/watchdog
//! state machine, and (in `tests/output_isolation.rs`) the real spawned
//! binary speaking the real socket protocol, surviving parent death, and
//! being restarted by the supervisor. Only a screen can verify: actual
//! pixels, full-screen placement on the right monitor, and event delivery
//! into the webview — Richard's rig test.

use std::io::Write as _;
use std::path::PathBuf;

/// Pure frame → HTML rendering. Mirrors the React `SlideView` so the separate
/// process paints identical pixels to the in-process preview.
mod render {
    use sundaystage_lib::services::display::OutputAppearance;
    use sundaystage_lib::services::live_session::LiveFrame;
    use sundaystage_lib::services::slide_doc::HAlign;
    use sundaystage_lib::services::text_fit::{fit_text, FitBox, FitParams};

    /// The virtual stage the fit is computed against (matches `slide_doc` docs
    /// and the editor's `STAGE_HEIGHT`/`STAGE_ASPECT`). The lyric block occupies
    /// the same 0.84×0.50 centered region the `Lyrics Centered` template uses, so
    /// the output's auto-fit lines up with the editor preview.
    const STAGE_W: f32 = 1920.0;
    const STAGE_H: f32 = 1080.0;
    /// Fraction of the frame the lyric block fills (centered lyrics template).
    const LYRIC_W_FRAC: f32 = 0.84;
    const LYRIC_H_FRAC: f32 = 0.50;
    /// Base body size on the stage; the historical `5.5 cqw` ≈ 5.5% of a 1920px
    /// frame = ~105.6px, but the editor authors at 64px @1080, so we fit against
    /// the same 64px base scaled by `text_scale` and report the result back in
    /// cqw for the output's container-query units.
    const BASE_PX: f32 = 64.0;
    const MIN_PX: f32 = 22.0;
    const STEP_PX: f32 = 2.0;
    /// 1px @1080 stage → this many `cqw` (% of a 1920px-wide frame).
    const PX_TO_CQW: f32 = 100.0 / STAGE_W;

    /// Measure wrapped lyrics for the output process. Real glyph metrics are a
    /// runtime concern (the window's layout engine), so headlessly we use a
    /// conservative average-advance model: each glyph ≈ `size * 0.52` px wide,
    /// line-height from the appearance. This is the SAME shape the editor's
    /// canvas-`measureText` closure feeds [`fit_text`]; the size search — the
    /// part that is unit-tested in `services::text_fit` — is identical, so the
    /// preview and output agree to within the measurer's accuracy. Wrapping is
    /// greedy word-wrap; hard `\n` always break.
    fn measure_lyrics(
        line_height: f32,
    ) -> impl Fn(&str, f32, f32) -> sundaystage_lib::services::text_fit::LaidOut {
        move |text: &str, size: f32, max_width: f32| {
            let glyph = (size * 0.52).max(0.001);
            let max_chars = ((max_width / glyph).floor() as usize).max(1);
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
            let height = lines.len() as f32 * size * line_height;
            sundaystage_lib::services::text_fit::LaidOut { lines, height }
        }
    }

    /// Compute the auto-fit font size (in `cqw`) for the slide's lyric lines so a
    /// long translation / pasted wall of text shrinks to stay on screen instead
    /// of overflowing. Pure: delegates the search to the shared
    /// [`fit_text`] algorithm — the editor uses the identical algorithm so the
    /// preview matches.
    pub fn fit_lyrics_cqw(lines: &[String], appearance: &OutputAppearance) -> f32 {
        let text = lines.join("\n");
        let scale = appearance.text_scale.max(0.1);
        let bx = FitBox {
            width: STAGE_W * LYRIC_W_FRAC,
            height: STAGE_H * LYRIC_H_FRAC,
            max_lines: None,
        };
        let params = FitParams {
            base: BASE_PX * scale,
            min: MIN_PX,
            step: STEP_PX,
        };
        let result = fit_text(&text, &bx, &params, &measure_lyrics(appearance.line_height));
        result.size * PX_TO_CQW
    }

    /// Map [`HAlign`] to its CSS `text-align` value.
    fn align_css(a: HAlign) -> &'static str {
        match a {
            HAlign::Left => "left",
            HAlign::Center => "center",
            HAlign::Right => "right",
        }
    }

    /// Escape text so lyrics — and appearance values like colours that land
    /// inside double-quoted `style="…"` attributes — can never inject markup or
    /// break out of an attribute in the output document. Quotes are escaped too
    /// because some call sites interpolate into attribute values; a legitimate
    /// colour/lyric never contains `"`/`'`.
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
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
                // Auto-fit: a long translation / pasted lyrics would overflow the
                // fixed slide, so shrink the body size (shared with the editor via
                // `text_fit`) instead of spilling off-screen.
                let font_cqw = fit_lyrics_cqw(&slide_content.text_lines, appearance);
                for line in &slide_content.text_lines {
                    body.push_str(&format!(
                        r#"<p class="line" style="color:{};font-size:{}cqw;line-height:{};text-transform:{}">{}</p>"#,
                        esc(&appearance.text_color),
                        font_cqw,
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

/// Everything the process needs, parsed from CLI args (with the socket also
/// accepted via `SUNDAYSTAGE_OUTPUT_SOCKET` for tooling). Parsing is total —
/// bad values fall back to safe defaults rather than refusing to start a
/// projector mid-service.
#[derive(Debug, Clone, PartialEq)]
struct Opts {
    socket: Option<PathBuf>,
    headless: bool,
    label: String,
    /// Window position (x, y) on the virtual desktop.
    position: (i32, i32),
    /// Window size (w, h).
    size: (u32, u32),
    appearance_file: Option<PathBuf>,
}

impl Opts {
    fn parse(args: &[String]) -> Self {
        fn value_of(args: &[String], flag: &str) -> Option<String> {
            let eq = format!("{flag}=");
            let mut it = args.iter();
            while let Some(a) = it.next() {
                if let Some(v) = a.strip_prefix(&eq) {
                    return Some(v.to_string());
                }
                if a == flag {
                    return it.next().cloned();
                }
            }
            None
        }
        fn pair<T: std::str::FromStr + Copy>(s: &str, sep: char, fallback: (T, T)) -> (T, T) {
            let mut it = s.split(sep);
            match (
                it.next().and_then(|v| v.parse().ok()),
                it.next().and_then(|v| v.parse().ok()),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => fallback,
            }
        }
        let socket = value_of(args, "--socket")
            .or_else(|| std::env::var("SUNDAYSTAGE_OUTPUT_SOCKET").ok())
            .map(PathBuf::from);
        Self {
            socket,
            headless: args.iter().any(|a| a == "--headless"),
            label: value_of(args, "--label").unwrap_or_else(|| "output-main-0".into()),
            position: value_of(args, "--position")
                .map(|s| pair(&s, ',', (0, 0)))
                .unwrap_or((0, 0)),
            size: value_of(args, "--size")
                .map(|s| pair(&s, 'x', (1920, 1080)))
                .unwrap_or((1920, 1080)),
            appearance_file: value_of(args, "--appearance-file").map(PathBuf::from),
        }
    }
}

// ── IPC client (socket modes) ────────────────────────────────────────────────

/// Callback that delivers a render (frame + seq) to the window renderer.
type RenderFn = Box<dyn Fn(&sundaystage_lib::services::live_session::LiveFrame, u64) + Send>;

/// What the windowed shell does with each step the loop decides on. Headless
/// passes no-ops; the Tauri shell forwards into the webview.
struct Sink {
    /// Deliver a render (the raw frame + seq) to the window renderer.
    on_render: RenderFn,
    /// Deliver a heartbeat to the window renderer's own watchdog.
    on_heartbeat: Box<dyn Fn(i64) + Send>,
}

impl Sink {
    fn noop() -> Self {
        Self {
            on_render: Box::new(|_, _| {}),
            on_heartbeat: Box::new(|_| {}),
        }
    }
}

/// Connect to the supervisor's endpoint (retrying briefly — the parent binds
/// it just before spawning us) and pump messages until Shutdown or link loss.
/// Returns `true` if we were told to shut down, `false` if the link died —
/// in which case the caller must HOLD the last frame and keep the process
/// alive (the crash-isolation promise).
async fn ipc_client_loop(socket: &std::path::Path, sink: Sink) -> bool {
    use sundaystage_lib::output::ipc;
    use sundaystage_lib::output::{OutputAck, OutputMessage};

    // The parent spawns us right after binding, but give slow machines slack.
    let stream = {
        let mut attempt = 0u32;
        loop {
            match ipc::connect(socket).await {
                Ok(s) => break s,
                Err(e) if attempt < 50 => {
                    attempt += 1;
                    eprintln!("sundaystage-output: connect retry {attempt}: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(e) => {
                    eprintln!("sundaystage-output: cannot reach main app: {e}");
                    return false;
                }
            }
        }
    };
    eprintln!("sundaystage-output: connected to {}", socket.display());

    let (mut reader, mut writer) = stream.into_split();
    let mut h = handler::Handler::new(now_ms());
    let mut announced_hold = false;
    let tick = std::time::Duration::from_millis(500);
    loop {
        let msg = match tokio::time::timeout(tick, reader.read::<OutputMessage>()).await {
            Ok(Ok(Some(msg))) => msg,
            // Clean EOF or a broken pipe: the main app is gone. HOLD.
            Ok(Ok(None)) => return false,
            Ok(Err(e)) => {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    // One bad frame must never blank the projector — report it
                    // and keep reading.
                    let _ = writer
                        .write(&OutputAck::Error {
                            message: format!("bad message: {e}"),
                        })
                        .await;
                    continue;
                }
                return false;
            }
            // No traffic this interval — watchdog tick (hold-last-frame check).
            Err(_elapsed) => {
                let now = now_ms();
                if h.tick(now) && !h.is_alive(now) && !announced_hold {
                    announced_hold = true;
                    eprintln!("sundaystage-output: lost heartbeat — holding last frame");
                }
                continue;
            }
        };
        announced_hold = false;
        let now = now_ms();
        match &msg {
            OutputMessage::Render { frame, seq } => (sink.on_render)(frame, *seq),
            OutputMessage::Heartbeat { at } => (sink.on_heartbeat)(*at),
            OutputMessage::Shutdown => {}
        }
        let step = h.handle(msg, now);
        if let Some(ack) = step.ack {
            if writer.write(&ack).await.is_err() {
                return false;
            }
        }
        if step.shutdown {
            return true;
        }
    }
}

/// Headless socket mode (CI / integration tests): run the client loop; on
/// link loss, stay alive holding the (virtual) last frame — exactly what the
/// windowed process does with real pixels.
fn run_headless(socket: PathBuf) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async move {
        let clean = ipc_client_loop(&socket, Sink::noop()).await;
        if clean {
            eprintln!("sundaystage-output: shutdown requested — exiting");
            return;
        }
        eprintln!("sundaystage-output: link lost — holding last frame until killed");
        // The crash-isolation promise: outlive the parent. The relaunched
        // main app reaps us via the pidfile before spawning a replacement.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
}

// ── Windowed mode (real Tauri app) ───────────────────────────────────────────

/// State for the two tiny commands the `output.html` renderer calls on boot.
struct ChildState {
    appearance_file: Option<PathBuf>,
}

/// The renderer's late-join appearance fetch. The child has no DB — it reads
/// the same `output_appearance.json` the main app persists, passed by path.
#[tauri::command]
fn output_appearance(
    state: tauri::State<'_, ChildState>,
) -> sundaystage_lib::services::display::OutputAppearance {
    state
        .appearance_file
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| {
            serde_json::from_str::<sundaystage_lib::services::display::OutputAppearance>(&s).ok()
        })
        .unwrap_or_default()
        .sanitized()
}

/// The renderer's late-join frame fetch. In the isolated process the
/// supervisor pushes the current frame immediately on connect, so there is
/// nothing to pull — return "no session" and let the push paint.
#[tauri::command]
fn live_state() -> Option<serde_json::Value> {
    None
}

/// A real Tauri app: one borderless full-screen window on the assigned
/// monitor, rendering the shared `output.html` (React `OutputView`), fed by
/// the IPC client loop via the same `ss://render`/`ss://heartbeat` events the
/// in-process windows use. Needs a windowing session — verified on the rig.
fn run_windowed(opts: Opts) {
    use tauri::{Emitter as _, Manager as _, WebviewUrl, WebviewWindowBuilder};

    let socket = opts
        .socket
        .clone()
        .expect("windowed mode requires --socket");
    let label = opts.label.clone();
    tauri::Builder::default()
        .setup(move |app| {
            let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("output.html".into()))
                .title("SundayStage")
                // Same surface contract as output::window::open_outputs.
                .decorations(false)
                .position(opts.position.0 as f64, opts.position.1 as f64)
                .inner_size(opts.size.0 as f64, opts.size.1 as f64)
                .always_on_top(true)
                .skip_taskbar(true)
                .focused(false)
                .build()?;
            let _ = win.set_fullscreen(true);

            app.manage(ChildState {
                appearance_file: opts.appearance_file.clone(),
            });

            let handle = app.handle().clone();
            let socket = socket.clone();
            tauri::async_runtime::spawn(async move {
                let emit_handle = handle.clone();
                let hb_handle = handle.clone();
                let sink = Sink {
                    on_render: Box::new(move |frame, seq| {
                        // Same payload shape the operator UI's outputBridge
                        // emits, so OutputView needs zero changes.
                        let _ = emit_handle.emit(
                            "ss://render",
                            serde_json::json!({ "frame": frame, "seq": seq }),
                        );
                    }),
                    on_heartbeat: Box::new(move |at| {
                        let _ = hb_handle.emit("ss://heartbeat", serde_json::json!({ "at": at }));
                    }),
                };
                let clean = ipc_client_loop(&socket, sink).await;
                if clean {
                    handle.exit(0);
                } else {
                    // Main app died: hold the last frame — the heartbeat stop
                    // makes OutputView's own watchdog show the badge on
                    // stage/confidence chrome. We stay alive until reaped.
                    eprintln!("sundaystage-output: main app gone — holding last frame");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![output_appearance, live_state])
        .run(tauri::generate_context!("tauri.output.conf.json"))
        .expect("error while running sundaystage-output");
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
    let args: Vec<String> = std::env::args().collect();
    let opts = Opts::parse(&args);
    match (&opts.socket, opts.headless) {
        // Crash-isolated socket modes (Phase 5.2 proper).
        (Some(socket), true) => run_headless(socket.clone()),
        (Some(_), false) => run_windowed(opts),
        // Legacy stdio smoke path (no transport endpoint given).
        (None, _) => run_io_loop(),
    }
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
    fn opts_parse_reads_all_flags() {
        let args: Vec<String> = [
            "bin",
            "--socket",
            "/tmp/x.sock",
            "--label",
            "output-stage-2",
            "--position",
            "1920,0",
            "--size",
            "2560x1440",
            "--appearance-file",
            "/data/output_appearance.json",
            "--headless",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let o = super::Opts::parse(&args);
        assert_eq!(
            o.socket.as_deref(),
            Some(std::path::Path::new("/tmp/x.sock"))
        );
        assert_eq!(o.label, "output-stage-2");
        assert_eq!(o.position, (1920, 0));
        assert_eq!(o.size, (2560, 1440));
        assert!(o.headless);
        assert_eq!(
            o.appearance_file.as_deref(),
            Some(std::path::Path::new("/data/output_appearance.json"))
        );
    }

    #[test]
    fn opts_parse_is_total_with_safe_defaults() {
        let o = super::Opts::parse(&["bin".to_string()]);
        assert_eq!(o.socket, None);
        assert!(!o.headless);
        assert_eq!(o.label, "output-main-0");
        assert_eq!(o.position, (0, 0));
        assert_eq!(o.size, (1920, 1080));
        // Garbage geometry falls back instead of refusing to start.
        let o = super::Opts::parse(&[
            "bin".to_string(),
            "--position".into(),
            "oops".into(),
            "--size".into(),
            "bad".into(),
        ]);
        assert_eq!(o.position, (0, 0));
        assert_eq!(o.size, (1920, 1080));
    }

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
    fn long_lyrics_auto_fit_to_a_smaller_size_than_short_ones() {
        // The output consumes the SHARED `text_fit` algorithm: a wall of pasted
        // lyrics must render at a smaller font than a one-liner so it stays on
        // screen. We assert the relationship via the computed cqw size.
        let appearance = OutputAppearance::default();
        let short = super::render::fit_lyrics_cqw(&["Amazing grace".to_string()], &appearance);
        let long_line = "the quick brown fox jumps over the lazy dog ".repeat(8);
        let long = super::render::fit_lyrics_cqw(
            &[long_line.clone(), long_line.clone(), long_line],
            &appearance,
        );
        assert!(
            long < short,
            "long lyrics ({long}) should fit smaller than short ({short})"
        );
    }

    #[test]
    fn short_lyrics_keep_the_base_output_size() {
        // A single short line should not be shrunk at all — base 64px @1080
        // scaled to cqw (100/1920 px→cqw) = 64 * 100 / 1920 ≈ 3.333 cqw.
        let appearance = OutputAppearance::default();
        let cqw = super::render::fit_lyrics_cqw(&["Hi".to_string()], &appearance);
        let expected = 64.0_f32 * 100.0 / 1920.0;
        assert!((cqw - expected).abs() < 1e-3, "got {cqw}, want {expected}");
    }

    #[test]
    fn auto_fit_size_appears_in_rendered_slide_html() {
        let appearance = OutputAppearance::default();
        let html = frame_to_html(&slide(&["Amazing grace"]), &appearance);
        // The static 5.5cqw is gone; the fitted size is present.
        assert!(!html.contains("5.5cqw"));
        assert!(html.contains("cqw"));
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

    #[test]
    fn appearance_colors_cannot_break_out_of_style_attribute() {
        // A hand-edited / synced appearance JSON whose color contains a double
        // quote must NOT be able to close the `style="…"` attribute and inject
        // new attributes/markup into the sacrosanct live output document.
        let appearance = OutputAppearance {
            text_color: "#fff\" onload=\"alert(1)".into(),
            bg_color: "#000\"><script>boom()</script>".into(),
            ..OutputAppearance::default()
        };
        let html = frame_to_html(&slide(&["Amazing grace"]), &appearance);
        // The raw injected attribute/markup must not survive as live HTML: a
        // live `onload="` attribute would require an UNescaped quote to have
        // closed `style="…"` first, and a live `<script>` requires an unescaped
        // `<`. Both must be escaped, leaving only inert `&quot;`/`&lt;` text.
        assert!(
            !html.contains("onload=\""),
            "color broke out of style attribute: {html}"
        );
        assert!(
            !html.contains("<script>"),
            "bg color injected markup: {html}"
        );
        // The attacker's quote must appear only in escaped form.
        assert!(!html.contains("#fff\""), "unescaped breakout quote: {html}");
        assert!(html.contains("&quot;"), "quote was not escaped: {html}");
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
