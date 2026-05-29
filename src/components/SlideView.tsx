/**
 * The single source of truth for how a slide *looks* on the congregation
 * output. Rendered both in the real output windows (`src/output/OutputView`)
 * and in the Settings → Output live preview, so what you tweak is exactly what
 * the room sees. Styling is driven entirely by `OutputAppearance`.
 */
import type { CSSProperties } from "react";

import type { LiveFrame, OutputAppearance } from "@/lib/bindings";

interface Props {
  frame: LiveFrame | null;
  appearance: OutputAppearance;
  /** Force the section label on regardless of appearance (stage/confidence
   *  screens always want it; the main output respects the setting). */
  forceSectionLabel?: boolean;
  /** Base lyric size in vw; the preview passes a smaller base. */
  baseVw?: number;
}

export function SlideView({
  frame,
  appearance,
  forceSectionLabel = false,
  baseVw = 5.5,
}: Props) {
  const bg: CSSProperties = { backgroundColor: appearance.bg_color };

  if (!frame || frame.kind === "black") {
    return <div className="h-full w-full bg-black" />;
  }
  if (frame.kind === "logo") {
    return (
      <div
        className="grid h-full w-full place-items-center font-bold text-[var(--color-accent)] [font-size:8vw]"
        style={bg}
      >
        SundayStage
      </div>
    );
  }
  if (frame.kind === "message") {
    return (
      <div
        className="grid h-full w-full place-items-center px-[8vw] text-center [font-size:4vw]"
        style={{ ...bg, color: appearance.text_color }}
      >
        {frame.text}
      </div>
    );
  }

  const c = frame.slide_content;
  const showLabel = forceSectionLabel || appearance.show_section_label;
  const lineStyle: CSSProperties = {
    color: appearance.text_color,
    fontSize: `${baseVw * appearance.text_scale}vw`,
    lineHeight: appearance.line_height,
    textTransform: appearance.uppercase ? "uppercase" : "none",
  };

  return (
    <div className="grid h-full w-full place-items-center px-[6vw]" style={bg}>
      <div className="w-full" style={{ textAlign: appearance.h_align }}>
        {showLabel && c.section_label && (
          <div className="mb-[3vh] font-semibold tracking-[0.3em] text-[var(--color-accent)] uppercase [font-size:1.6vw]">
            {c.section_label}
          </div>
        )}
        {c.text_lines.map((line, i) => (
          <p key={i} className="font-semibold" style={lineStyle}>
            {line}
          </p>
        ))}
        {c.translation_lines?.map((line, i) => (
          <p
            key={`t-${i}`}
            className="mt-[1vh]"
            style={{
              color: appearance.text_color,
              opacity: 0.7,
              fontSize: `${baseVw * 0.58 * appearance.text_scale}vw`,
            }}
          >
            {line}
          </p>
        ))}
        {c.reference && (
          <div
            className="mt-[4vh] [font-size:2vw]"
            style={{ color: appearance.text_color, opacity: 0.6 }}
          >
            — {c.reference}
          </div>
        )}
      </div>
    </div>
  );
}
