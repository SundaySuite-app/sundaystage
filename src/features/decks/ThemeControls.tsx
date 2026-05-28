/**
 * ThemeControls — Phase 3.2.
 *
 * Lives at the top of the editor's right rail. Lets the user:
 *   - apply a template (re-lays the slide's text into the template's slots)
 *   - apply a theme (restyles background + text to the church's look)
 *   - duplicate a built-in theme into an editable library theme
 *   - edit that theme's tokens with a live preview
 *   - set the library default theme/template (cascade level 3)
 *
 * Applying template/theme persists the slide's override id and replaces the
 * slide document through the parent's history (one undo step).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CopyPlus, Star } from "lucide-react";

import type {
  SlideDoc,
  Slide,
  Template,
  Theme,
  ThemeTokens,
} from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { newTextBlock } from "@/lib/slideEditor/doc";
import {
  applyThemeToDoc,
  combinedText,
  parseLayout,
  parseTokens,
} from "@/lib/slideEditor/theme";
import { cn } from "@/lib/cn";
import { SlideCanvas } from "./SlideCanvas";

interface ThemeControlsProps {
  libraryId: string;
  doc: SlideDoc;
  activeSlide: Slide | null;
  onReplaceDoc: (after: SlideDoc) => void;
  onSetSlideTheme: (themeId: string | null) => void;
  onSetSlideTemplate: (templateId: string | null) => void;
}

export function ThemeControls({
  libraryId,
  doc,
  activeSlide,
  onReplaceDoc,
  onSetSlideTheme,
  onSetSlideTemplate,
}: ThemeControlsProps) {
  const qc = useQueryClient();

  const themesQuery = useQuery({
    queryKey: ["themes", libraryId],
    queryFn: () => ipc.theme.listThemes(libraryId),
  });
  const templatesQuery = useQuery({
    queryKey: ["templates", libraryId],
    queryFn: () => ipc.theme.listTemplates(libraryId),
  });
  const themes = useMemo(() => themesQuery.data ?? [], [themesQuery.data]);
  const templates = useMemo(
    () => templatesQuery.data ?? [],
    [templatesQuery.data],
  );

  // User picker overrides; reset to the slide's own ids when the slide changes.
  const [pickedTheme, setPickedTheme] = useState<string | null>(null);
  const [pickedTemplate, setPickedTemplate] = useState<string | null>(null);
  useEffect(() => {
    setPickedTheme(null);
    setPickedTemplate(null);
  }, [activeSlide?.id]);

  const themeId = pickedTheme ?? activeSlide?.theme_id ?? themes[0]?.id ?? null;
  const templateId =
    pickedTemplate ?? activeSlide?.template_id ?? templates[0]?.id ?? null;
  const selectedTheme = themes.find((t) => t.id === themeId) ?? null;
  const editable = !!selectedTheme && Number(selectedTheme.is_builtin) === 0;

  // ── Apply actions ──────────────────────────────────────────────────────────
  const renderMut = useMutation({
    mutationFn: ({
      template,
      slotText,
    }: {
      template: Template;
      slotText: Record<string, string>;
    }) => ipc.theme.render(libraryId, template.id, themeId ?? "", slotText),
  });

  const applyTemplate = (template: Template) => {
    const layout = parseLayout(template);
    const text = combinedText(doc);
    const slotText =
      layout.slots.length > 0 ? { [layout.slots[0].name]: text } : {};
    renderMut.mutate(
      { template, slotText },
      {
        onSuccess: (rendered) => {
          onReplaceDoc(rendered);
          onSetSlideTemplate(template.id);
          setPickedTemplate(template.id);
        },
      },
    );
  };

  const applyTheme = (theme: Theme) => {
    onReplaceDoc(applyThemeToDoc(doc, parseTokens(theme)));
    onSetSlideTheme(theme.id);
    setPickedTheme(theme.id);
  };

  // ── Theme management ────────────────────────────────────────────────────────
  const duplicateMut = useMutation({
    mutationFn: (sourceId: string) => ipc.theme.duplicate(sourceId, libraryId),
    onSuccess: (created) => {
      void qc.invalidateQueries({ queryKey: ["themes", libraryId] });
      setPickedTheme(created.id);
    },
  });

  const setDefaultThemeMut = useMutation({
    mutationFn: (id: string) => ipc.theme.setLibraryDefaultTheme(libraryId, id),
  });

  const updateTokensMut = useMutation({
    mutationFn: ({ id, tokens }: { id: string; tokens: ThemeTokens }) =>
      ipc.theme.updateTokens(id, tokens),
    onSuccess: (saved) => {
      qc.setQueryData<Theme[]>(["themes", libraryId], (old) =>
        (old ?? []).map((t) => (t.id === saved.id ? saved : t)),
      );
    },
  });

  // Local editable token state for the live-preview editor.
  const [editTokens, setEditTokens] = useState<ThemeTokens | null>(null);
  useEffect(() => {
    setEditTokens(selectedTheme ? parseTokens(selectedTheme) : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedTheme?.id]);

  const saveTimer = useRef<number | null>(null);
  const editToken = (patch: Partial<ThemeTokens>) => {
    if (!editable || !selectedTheme || !editTokens) return;
    const next = { ...editTokens, ...patch };
    setEditTokens(next);
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(
      () => updateTokensMut.mutate({ id: selectedTheme.id, tokens: next }),
      400,
    );
  };

  const previewDoc: SlideDoc | null = useMemo(() => {
    if (!editTokens) return null;
    const sample: SlideDoc = {
      background: editTokens.background,
      blocks: [newTextBlock("Forhåndsvisning", { y: 0.35, h: 0.3 })],
    };
    return applyThemeToDoc(sample, editTokens);
  }, [editTokens]);

  return (
    <div className="space-y-4 border-b border-[var(--color-border)] p-4 text-sm">
      <section className="space-y-2">
        <Header>Mal</Header>
        <select
          value={templateId ?? ""}
          onChange={(e) => {
            const tpl = templates.find((t) => t.id === e.target.value);
            if (tpl) applyTemplate(tpl);
          }}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {templates.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
              {Number(t.is_builtin) === 0 ? " ★" : ""}
            </option>
          ))}
        </select>
      </section>

      <section className="space-y-2">
        <Header>Tema</Header>
        <select
          value={themeId ?? ""}
          onChange={(e) => {
            const th = themes.find((t) => t.id === e.target.value);
            if (th) applyTheme(th);
          }}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {themes.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
              {Number(t.is_builtin) === 0 ? " ★" : ""}
            </option>
          ))}
        </select>

        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => themeId && duplicateMut.mutate(themeId)}
            disabled={!themeId}
            className="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
          >
            <CopyPlus size={13} /> Dupliser
          </button>
          <button
            type="button"
            onClick={() => themeId && setDefaultThemeMut.mutate(themeId)}
            disabled={!themeId}
            title="Sett som bibliotekets standardtema"
            className="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
          >
            <Star size={13} /> Standard
          </button>
        </div>
      </section>

      {editable && editTokens ? (
        <section className="space-y-2.5">
          <Header>Rediger tema</Header>
          {previewDoc && (
            <div className="overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
              <SlideCanvas doc={previewDoc} width={240} height={135} />
            </div>
          )}
          <TokenRow label="Bakgrunn">
            <ColorInput
              value={editTokens.background.value}
              onChange={(value) =>
                editToken({ background: { ...editTokens.background, value } })
              }
            />
          </TokenRow>
          <TokenRow label="Tekstfarge">
            <ColorInput
              value={editTokens.text_color}
              onChange={(text_color) => editToken({ text_color })}
            />
          </TokenRow>
          <TokenRow label="Aksent">
            <ColorInput
              value={editTokens.accent_color}
              onChange={(accent_color) => editToken({ accent_color })}
            />
          </TokenRow>
          <TokenRow label={`Størrelse (${Math.round(editTokens.body_size)})`}>
            <input
              type="range"
              min={24}
              max={140}
              value={editTokens.body_size}
              onChange={(e) => editToken({ body_size: Number(e.target.value) })}
              className="w-28 accent-[var(--color-accent)]"
            />
          </TokenRow>
          <TokenRow label="Vekt">
            <select
              value={editTokens.heading_weight}
              onChange={(e) =>
                editToken({ heading_weight: Number(e.target.value) })
              }
              className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1 text-xs focus:border-[var(--color-accent)] focus:outline-none"
            >
              {[400, 500, 600, 700, 800, 900].map((w) => (
                <option key={w} value={w}>
                  {w}
                </option>
              ))}
            </select>
          </TokenRow>
          <TokenRow label="Skygge">
            <input
              type="checkbox"
              checked={editTokens.shadow !== null}
              onChange={(e) =>
                editToken({
                  shadow: e.target.checked ? "0 2px 8px rgba(0,0,0,0.6)" : null,
                })
              }
              className="h-4 w-4 accent-[var(--color-accent)]"
            />
          </TokenRow>
        </section>
      ) : (
        <p className="text-[11px] text-[var(--color-fg-muted)]">
          Dupliser et innebygd tema for å redigere fargene og typografien.
        </p>
      )}
    </div>
  );
}

function Header({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="text-[10px] font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
      {children}
    </h3>
  );
}

function TokenRow({
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

function ColorInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value) ? value : "#000000";
  return (
    <span className="flex items-center gap-2">
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          "h-6 w-8 cursor-pointer rounded border border-[var(--color-border)] bg-transparent",
        )}
      />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-24 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1.5 py-1 font-mono text-[11px] focus:border-[var(--color-accent)] focus:outline-none"
      />
    </span>
  );
}
