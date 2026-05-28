/**
 * Inspector — Phase 3.1 properties panel.
 *
 * Edits the background of the slide and the single selected text block. Each
 * control change is one undo step (committed through the editor's history);
 * the text body previews live and commits on blur so typing is one step, not
 * one-per-keystroke.
 */

import { useRef } from "react";

import type { BackgroundKind, SlideDoc } from "@/lib/bindings";
import {
  type TextBlock,
  isTextBlock,
  patchStyle,
  patchTextBlock,
  replaceBlock,
} from "@/lib/slideEditor/doc";
import {
  type Command,
  setBackgroundCommand,
  updateBlockCommand,
} from "@/lib/slideEditor/history";
import { cn } from "@/lib/cn";

interface InspectorProps {
  doc: SlideDoc;
  selectedIds: ReadonlySet<string>;
  onCommit: (cmd: Command) => void;
  onPreview: (doc: SlideDoc) => void;
}

const SHADOW = "0 2px 8px rgba(0,0,0,0.6)";

export function Inspector({
  doc,
  selectedIds,
  onCommit,
  onPreview,
}: InspectorProps) {
  const single =
    selectedIds.size === 1
      ? doc.blocks.find((b) => selectedIds.has(b.id))
      : undefined;
  const textBlock = single && isTextBlock(single) ? single : undefined;

  return (
    <div className="flex flex-col gap-5 p-4 text-sm">
      <BackgroundSection doc={doc} onCommit={onCommit} />
      {textBlock ? (
        <TextBlockSection
          key={textBlock.id}
          doc={doc}
          block={textBlock}
          onCommit={onCommit}
          onPreview={onPreview}
        />
      ) : (
        <p className="text-xs text-[var(--color-fg-muted)]">
          {selectedIds.size > 1
            ? `${selectedIds.size} elementer valgt. Dra for å flytte, eller velg ett for å redigere.`
            : "Velg et tekstelement for å redigere det."}
        </p>
      )}
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-2.5">
      <h3 className="text-[10px] font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex items-center justify-between gap-3">
      <span className="text-xs text-[var(--color-fg-muted)]">{label}</span>
      {children}
    </label>
  );
}

function SegmentedButtons<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex overflow-hidden rounded-md border border-[var(--color-border)]">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          className={cn(
            "px-2 py-1 text-xs transition-colors",
            value === o.value
              ? "bg-[var(--color-accent)] text-[var(--color-sunday-blue-900)]"
              : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function BackgroundSection({
  doc,
  onCommit,
}: {
  doc: SlideDoc;
  onCommit: (c: Command) => void;
}) {
  const bg = doc.background;
  const setKind = (kind: BackgroundKind) => {
    const value =
      kind === "gradient" && bg.type !== "gradient"
        ? "linear-gradient(160deg, #1a2240, #0b1020)"
        : kind === "color" && bg.type !== "color"
          ? "#0b1020"
          : bg.value;
    onCommit(setBackgroundCommand(bg, { type: kind, value }));
  };
  const setValue = (value: string) =>
    onCommit(setBackgroundCommand(bg, { ...bg, value }));

  return (
    <Section title="Bakgrunn">
      <SegmentedButtons
        value={bg.type === "gradient" ? "gradient" : "color"}
        options={[
          { value: "color" as BackgroundKind, label: "Farge" },
          { value: "gradient" as BackgroundKind, label: "Gradient" },
        ]}
        onChange={setKind}
      />
      {bg.type === "color" ? (
        <Row label="Farge">
          <ColorField value={bg.value} onChange={setValue} />
        </Row>
      ) : (
        <textarea
          value={bg.value}
          onChange={(e) => setValue(e.target.value)}
          rows={2}
          className="w-full resize-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 font-mono text-[11px] focus:border-[var(--color-accent)] focus:outline-none"
        />
      )}
    </Section>
  );
}

function ColorField({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  // A hex value drives the native picker; arbitrary CSS colors still type in.
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value) ? value : "#000000";
  return (
    <span className="flex items-center gap-2">
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(e.target.value)}
        className="h-6 w-8 cursor-pointer rounded border border-[var(--color-border)] bg-transparent"
      />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-20 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1.5 py-1 font-mono text-[11px] focus:border-[var(--color-accent)] focus:outline-none"
      />
    </span>
  );
}

function TextBlockSection({
  doc,
  block,
  onCommit,
  onPreview,
}: {
  doc: SlideDoc;
  block: TextBlock;
  onCommit: (c: Command) => void;
  onPreview: (d: SlideDoc) => void;
}) {
  // Captured when the textarea gains focus so the whole edit is one undo step.
  const editStart = useRef<TextBlock | null>(null);

  const commitPatch = (after: TextBlock) =>
    onCommit(updateBlockCommand(block, after));

  return (
    <Section title="Tekst">
      <textarea
        value={block.text}
        rows={3}
        onFocus={() => {
          editStart.current = block;
        }}
        onChange={(e) =>
          onPreview(
            replaceBlock(doc, patchTextBlock(block, { text: e.target.value })),
          )
        }
        onBlur={(e) => {
          const before = editStart.current ?? block;
          editStart.current = null;
          if (before.text !== e.target.value) {
            onCommit(
              updateBlockCommand(
                before,
                patchTextBlock(before, { text: e.target.value }),
              ),
            );
          }
        }}
        className="w-full resize-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
      />

      <Row label="Justering">
        <SegmentedButtons
          value={block.align}
          options={[
            { value: "left", label: "V" },
            { value: "center", label: "M" },
            { value: "right", label: "H" },
          ]}
          onChange={(align) => commitPatch(patchTextBlock(block, { align }))}
        />
      </Row>
      <Row label="Vertikalt">
        <SegmentedButtons
          value={block.valign}
          options={[
            { value: "top", label: "Topp" },
            { value: "middle", label: "Midt" },
            { value: "bottom", label: "Bunn" },
          ]}
          onChange={(valign) => commitPatch(patchTextBlock(block, { valign }))}
        />
      </Row>

      <Row label={`Størrelse (${Math.round(block.style.size)})`}>
        <input
          type="range"
          min={16}
          max={200}
          step={1}
          value={block.style.size}
          onChange={(e) =>
            commitPatch(patchStyle(block, { size: Number(e.target.value) }))
          }
          className="w-32 accent-[var(--color-accent)]"
        />
      </Row>
      <Row label="Vekt">
        <select
          value={block.style.weight}
          onChange={(e) =>
            commitPatch(patchStyle(block, { weight: Number(e.target.value) }))
          }
          className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {[400, 500, 600, 700, 800, 900].map((w) => (
            <option key={w} value={w}>
              {w}
            </option>
          ))}
        </select>
      </Row>
      <Row label="Farge">
        <ColorField
          value={block.style.color}
          onChange={(color) => commitPatch(patchStyle(block, { color }))}
        />
      </Row>
      <Row label="Kursiv">
        <input
          type="checkbox"
          checked={block.style.italic}
          onChange={(e) =>
            commitPatch(patchStyle(block, { italic: e.target.checked }))
          }
          className="h-4 w-4 accent-[var(--color-accent)]"
        />
      </Row>
      <Row label="Skygge">
        <input
          type="checkbox"
          checked={block.style.shadow !== null}
          onChange={(e) =>
            commitPatch(
              patchStyle(block, { shadow: e.target.checked ? SHADOW : null }),
            )
          }
          className="h-4 w-4 accent-[var(--color-accent)]"
        />
      </Row>
    </Section>
  );
}
