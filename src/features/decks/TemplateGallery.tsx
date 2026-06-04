/**
 * TemplateGallery — deep-stage-2.
 *
 * A modal that lets the operator pick a layout template and apply it to the
 * current slide. Each card shows a live preview (the real render bridge, fed
 * sample content) so "what you pick is what you get". Applying maps the slide's
 * derived content payload onto the chosen template's slots via the same pure
 * mapping the backend uses, then replaces the slide document through the
 * editor's history (one undo step) and pins the slide's template override.
 *
 * The selection/apply rules live in pure, unit-tested helpers
 * (`lib/slideEditor/templateGallery.ts` + `slotMapping.ts`); this file is the
 * thin React shell around them.
 */

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search, X } from "lucide-react";

import type { SlideDoc, Template, Theme } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";
import { parseLayout, parseTokens } from "@/lib/slideEditor/theme";
import {
  derivePayloadFromDoc,
  mapContentToSlots,
} from "@/lib/slideEditor/slotMapping";
import {
  buildApplyPlan,
  filterTemplates,
  gallerySections,
} from "@/lib/slideEditor/templateGallery";
import { SlideCanvas } from "./SlideCanvas";

interface TemplateGalleryProps {
  libraryId: string;
  themeId: string;
  doc: SlideDoc;
  templates: Template[];
  themes: Theme[];
  onApply: (rendered: SlideDoc, templateId: string) => void;
  onClose: () => void;
}

/** Sample text used only to paint a template's preview thumbnail. */
const SAMPLE = {
  title: "Amazing Grace",
  body: "How sweet the sound\n\nThat saved a wretch like me",
  lyrics: "Amazing grace how sweet the sound\n\nThat saved a wretch like me",
  reference: "John 3:16",
  footer: "— Sunday Service",
  image: "",
};

export function TemplateGallery({
  libraryId,
  themeId,
  doc,
  templates,
  themes,
  onApply,
  onClose,
}: TemplateGalleryProps) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);

  const filtered = useMemo(
    () => filterTemplates(templates, query),
    [templates, query],
  );
  const sections = useMemo(() => gallerySections(filtered), [filtered]);

  const previewTokens = useMemo(() => {
    const th = themes.find((x) => x.id === themeId);
    return th ? parseTokens(th) : null;
  }, [themes, themeId]);

  const apply = async (template: Template) => {
    const payload = derivePayloadFromDoc(doc);
    const plan = buildApplyPlan(template, payload);
    setBusyId(template.id);
    try {
      const rendered = await ipc.theme.render(
        libraryId,
        plan.templateId,
        themeId,
        plan.slotText,
      );
      onApply(rendered, template.id);
      onClose();
    } finally {
      setBusyId(null);
    }
  };

  const sectionLabel = (kind: "builtin" | "custom") =>
    kind === "builtin" ? t("galBuiltins") : t("galCustom");

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-6"
      role="dialog"
      aria-modal="true"
      aria-label={t("galTitle")}
    >
      <div className="flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-2xl">
        <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-5 py-3.5">
          <h2 className="text-[var(--text-ui-lg)] font-semibold">
            {t("galTitle")}
          </h2>
          <div className="flex-1" />
          <div className="relative">
            <Search
              size={14}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--color-fg-muted)]"
            />
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("galSearch")}
              className="w-56 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] py-1.5 pl-8 pr-2 text-xs focus:border-[var(--color-accent)] focus:outline-none"
            />
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("actionCancel")}
            className="grid h-8 w-8 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={16} />
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-5">
          {sections.length === 0 && (
            <p className="py-12 text-center text-sm text-[var(--color-fg-muted)]">
              {t("galEmpty")}
            </p>
          )}
          {sections.map((section) => (
            <section key={section.kind} className="mb-6 last:mb-0">
              <h3 className="mb-2.5 text-[10px] font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
                {sectionLabel(section.kind)}
              </h3>
              <ul className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
                {section.templates.map((template) => (
                  <li key={template.id}>
                    <TemplateCard
                      template={template}
                      tokens={previewTokens}
                      libraryId={libraryId}
                      themeId={themeId}
                      busy={busyId === template.id}
                      disabled={busyId !== null}
                      onApply={() => void apply(template)}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}

function TemplateCard({
  template,
  tokens,
  libraryId,
  themeId,
  busy,
  disabled,
  onApply,
}: {
  template: Template;
  tokens: ReturnType<typeof parseTokens> | null;
  libraryId: string;
  themeId: string;
  busy: boolean;
  disabled: boolean;
  onApply: () => void;
}) {
  const t = useT();
  // The preview uses the same render bridge + the same pure slot mapping the
  // apply will, so the thumbnail is faithful.
  const slotText = useMemo(
    () => mapContentToSlots(parseLayout(template), SAMPLE),
    [template],
  );
  const preview = useQuery({
    queryKey: ["template-preview", template.id, themeId, libraryId],
    queryFn: () => ipc.theme.render(libraryId, template.id, themeId, slotText),
  });

  return (
    <button
      type="button"
      onClick={onApply}
      disabled={disabled}
      title={t("galApplyTitle", { name: template.name })}
      className="group flex w-full flex-col gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-surface)] p-2 text-left transition-colors hover:border-[var(--color-accent)] disabled:cursor-not-allowed disabled:opacity-50"
    >
      <div className="relative aspect-video overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
        {preview.data && tokens ? (
          <SlideCanvas doc={preview.data} width={320} height={180} />
        ) : (
          <div className="h-full w-full animate-pulse bg-[var(--color-bg)]" />
        )}
        {busy && (
          <div className="absolute inset-0 grid place-items-center bg-black/40 text-xs text-white">
            {t("galApplying")}
          </div>
        )}
      </div>
      <span className="truncate px-0.5 text-xs font-medium">
        {template.name}
      </span>
    </button>
  );
}
