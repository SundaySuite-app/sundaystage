/* Companion "follow along" viewer (Phase 12.2 + 12.3).
 *
 * Renders the broadcast frames a phone receives, plus the accessibility pack.
 * The realtime transport (Supabase Realtime channel) rides on the Phase 9 cloud
 * layer — not wired yet — so this runs in DEMO mode: it cycles a few sample
 * frames so the viewer and accessibility settings can be tried end to end. The
 * broadcast schema matches the desktop publisher (services/companion/publisher.rs).
 */
(function () {
  "use strict";

  var SETTINGS_KEY = "ss-companion";
  var DEFAULTS = {
    size: "l",
    scheme: "dark",
    font: "sans",
    spacing: "default",
    tts: false,
    vibrate: false,
    reduceMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  };

  var settings = load();

  function load() {
    try {
      return Object.assign(
        {},
        DEFAULTS,
        JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}"),
      );
    } catch (e) {
      return Object.assign({}, DEFAULTS);
    }
  }
  function save() {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch (e) {
      /* ignore */
    }
  }

  // ── Apply settings to the DOM ──────────────────────────────────────────────
  var body = document.body;
  function applySettings() {
    body.dataset.size = settings.size;
    body.dataset.scheme = settings.scheme;
    body.dataset.font = settings.font;
    body.dataset.spacing = settings.spacing;
    body.dataset.reduce = settings.reduceMotion ? "1" : "0";
    document.querySelectorAll(".seg").forEach(function (seg) {
      var control = seg.dataset.control;
      seg.querySelectorAll("button").forEach(function (b) {
        b.setAttribute(
          "aria-pressed",
          String(b.dataset.val === settings[control]),
        );
      });
    });
    var tts = document.getElementById("tts");
    var vib = document.getElementById("vibrate");
    var rm = document.getElementById("reduce-motion");
    if (tts) tts.checked = settings.tts;
    if (vib) vib.checked = settings.vibrate;
    if (rm) rm.checked = settings.reduceMotion;
  }

  // ── Settings drawer wiring ─────────────────────────────────────────────────
  var drawer = document.getElementById("drawer");
  document.getElementById("open-settings").onclick = function () {
    drawer.classList.remove("hidden");
  };
  document.getElementById("close-settings").onclick = function () {
    drawer.classList.add("hidden");
  };
  document.querySelectorAll(".seg").forEach(function (seg) {
    var control = seg.dataset.control;
    seg.querySelectorAll("button").forEach(function (b) {
      b.onclick = function () {
        settings[control] = b.dataset.val;
        save();
        applySettings();
      };
    });
  });
  document.getElementById("tts").onchange = function (e) {
    settings.tts = e.target.checked;
    save();
    if (!settings.tts && "speechSynthesis" in window) speechSynthesis.cancel();
  };
  document.getElementById("vibrate").onchange = function (e) {
    settings.vibrate = e.target.checked;
    save();
  };
  document.getElementById("reduce-motion").onchange = function (e) {
    settings.reduceMotion = e.target.checked;
    save();
    applySettings();
  };
  document.getElementById("reread").onclick = function () {
    speak(currentText, true);
  };
  document.getElementById("reset").onclick = function () {
    settings = Object.assign({}, DEFAULTS);
    save();
    applySettings();
  };

  // ── Text-to-speech ─────────────────────────────────────────────────────────
  function speak(text, force) {
    if ((!settings.tts && !force) || !("speechSynthesis" in window) || !text)
      return;
    speechSynthesis.cancel();
    var u = new SpeechSynthesisUtterance(text);
    u.lang = "nb-NO";
    u.rate = 1;
    speechSynthesis.speak(u);
  }

  // ── Rendering a broadcast frame ────────────────────────────────────────────
  var sectionEl = document.getElementById("section");
  var contentEl = document.getElementById("content");
  var referenceEl = document.getElementById("reference");
  var currentText = "";
  var lastSeq = -1;

  function render(frame) {
    if (frame.seq <= lastSeq) return; // ignore stale/duplicate
    lastSeq = frame.seq;
    if (frame.kind === "blackout") {
      sectionEl.textContent = "";
      contentEl.textContent = "";
      referenceEl.textContent = "";
      currentText = "";
      return;
    }
    sectionEl.textContent = frame.section_label || "";
    contentEl.textContent = frame.text || "";
    referenceEl.textContent = frame.reference || "";
    currentText = [frame.section_label, frame.text, frame.reference]
      .filter(Boolean)
      .join(". ");
    if (settings.vibrate && navigator.vibrate) navigator.vibrate(60);
    speak(currentText, false);
  }

  // ── Wake lock (keep the screen on during a service) ────────────────────────
  var wakeLock = null;
  async function requestWakeLock() {
    try {
      if ("wakeLock" in navigator)
        wakeLock = await navigator.wakeLock.request("screen");
    } catch (e) {
      /* user can re-tap; not critical */
    }
  }
  document.addEventListener("visibilitychange", function () {
    if (document.visibilityState === "visible" && !wakeLock) requestWakeLock();
  });

  // ── DEMO transport (until Supabase Realtime is wired in Phase 9) ───────────
  var V = 1;
  var DEMO_FRAMES = [
    {
      v: V,
      kind: "lyric",
      text: "Amazing grace, how sweet the sound\nThat saved a wretch like me",
      section_label: "vers 1",
      reference: null,
    },
    {
      v: V,
      kind: "lyric",
      text: "I once was lost, but now am found\nWas blind, but now I see",
      section_label: "vers 1",
      reference: null,
    },
    {
      v: V,
      kind: "scripture",
      text: "For God so loved the world,\nthat he gave his only begotten Son",
      section_label: null,
      reference: "John 3:16",
    },
    {
      v: V,
      kind: "announcement",
      text: "Velkommen til gudstjenesten",
      section_label: null,
      reference: null,
    },
    {
      v: V,
      kind: "lyric",
      text: "My chains are gone, I've been set free",
      section_label: "refreng",
      reference: null,
    },
  ];

  function startDemo() {
    document.getElementById("connecting").classList.add("hidden");
    document.getElementById("viewer").classList.remove("hidden");
    var i = 0;
    render(Object.assign({ seq: 0 }, DEMO_FRAMES[0]));
    setInterval(function () {
      i = (i + 1) % DEMO_FRAMES.length;
      // seq always increases so the stale-guard mirrors real ordering
      render(Object.assign({ seq: lastSeq + 1 }, DEMO_FRAMES[i]));
    }, 6000);
  }

  // ── Boot ───────────────────────────────────────────────────────────────────
  applySettings();
  requestWakeLock();
  setTimeout(startDemo, 1200);

  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("sw.js").catch(function () {});
  }
})();
