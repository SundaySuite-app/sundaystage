/**
 * The single source of truth for how a slide *looks* on the congregation
 * output. Rendered both in the real output windows (`src/output/OutputView`)
 * and in the Settings → Output live preview, so what you tweak is exactly what
 * the room sees. Styling is driven entirely by `OutputAppearance`.
 *
 * Sizes use container-query units (cqw/cqh) and the root establishes a size
 * container, so the same component fills a 4K projector and a small preview box
 * with identical proportions.
 */
import type { CSSProperties } from "react";

import type { LiveFrame, OutputAppearance } from "@/lib/bindings";

interface Props {
  frame: LiveFrame | null;
  appearance: OutputAppearance;
  /** Force the section label on regardless of appearance (stage/confidence
   *  screens always want it; the main output respects the setting). */
  forceSectionLabel?: boolean;
}

export function SlideView({
  frame,
  appearance,
  forceSectionLabel = false,
}: Props) {
  return (
    <div
      className="h-full w-full"
      style={{ containerType: "size", backgroundColor: appearance.bg_color }}
    >
      <Inner
        frame={frame}
        appearance={appearance}
        forceSectionLabel={forceSectionLabel}
      />
    </div>
  );
}

function Inner({ frame, appearance, forceSectionLabel }: Props) {
  if (!frame || frame.kind === "black") {
    return <div className="h-full w-full bg-black" />;
  }
  if (frame.kind === "logo") {
    return (
      <div className="grid h-full w-full place-items-center font-bold text-[var(--color-accent)] [font-size:8cqw]">
        SundayStage
      </div>
    );
  }
  if (frame.kind === "message") {
    return (
      <div
        className="grid h-full w-full place-items-center px-[8cqw] text-center [font-size:4cqw]"
        style={{ color: appearance.text_color }}
      >
        {frame.text}
      </div>
    );
  }

  const c = frame.slide_content;
  const showLabel = forceSectionLabel || appearance.show_section_label;
  const lineStyle: CSSProperties = {
    color: appearance.text_color,
    fontSize: `${5.5 * appearance.text_scale}cqw`,
    lineHeight: appearance.line_height,
    textTransform: appearance.uppercase ? "uppercase" : "none",
  };

  return (
    <div className="grid h-full w-full place-items-center px-[6cqw]">
      <div className="w-full" style={{ textAlign: appearance.h_align }}>
        {showLabel && c.section_label && (
          <div className="mb-[3cqh] font-semibold tracking-[0.3em] text-[var(--color-accent)] uppercase [font-size:1.6cqw]">
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
            className="mt-[1cqh]"
            style={{
              color: appearance.text_color,
              opacity: 0.7,
              fontSize: `${3.2 * appearance.text_scale}cqw`,
            }}
          >
            {line}
          </p>
        ))}
        {c.reference && (
          <div
            className="mt-[4cqh] [font-size:2cqw]"
            style={{ color: appearance.text_color, opacity: 0.6 }}
          >
            — {c.reference}
          </div>
        )}
      </div>
    </div>
  );
}
