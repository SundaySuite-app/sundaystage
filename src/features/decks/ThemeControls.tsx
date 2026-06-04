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
import { CopyPlus, Pencil, Plus, Star, Trash2 } from "lucide-react";

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
  defaultTokens,
  parseLayout,
  parseTokens,
} from "@/lib/slideEditor/theme";
import {
  cleanThemeName,
  uniqueThemeName,
} from "@/lib/slideEditor/themeActions";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { ConfirmModal } from "@/components/ui";
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
  const t = useT();
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
  // Theme pending deletion — drives the confirmation modal.
  const [confirmDelete, setConfirmDelete] = useState<Theme | null>(null);
  useEffect(() => {
    setPickedTheme(null);
    setPickedTemplate(null);
  }, [activeSlide?.id]);

  const themeId = pickedTheme ?? activeSlide?.theme_id ?? themes[0]?.id ?? null;
  const templateId =
    pickedTemplate ?? activeSlide?.template_id ?? templates[0]?.id ?? null;
  const selectedTheme = themes.find((th) => th.id === themeId) ?? null;
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

  const setDefaultTemplateMut = useMutation({
    mutationFn: (id: string) =>
      ipc.theme.setLibraryDefaultTemplate(libraryId, id),
  });

  const createMut = useMutation({
    mutationFn: (name: string) =>
      ipc.theme.create(libraryId, name, defaultTokens()),
    onSuccess: (created) => {
      void qc.invalidateQueries({ queryKey: ["themes", libraryId] });
      setPickedTheme(created.id);
    },
  });

  const renameMut = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      ipc.theme.rename(id, name),
    onSuccess: (saved) => {
      qc.setQueryData<Theme[]>(["themes", libraryId], (old) =>
        (old ?? []).map((th) => (th.id === saved.id ? saved : th)),
      );
    },
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => ipc.theme.delete(id),
    onSuccess: (_void, id) => {
      void qc.invalidateQueries({ queryKey: ["themes", libraryId] });
      // Drop our pin so selection falls back to the slide/first theme.
      setPickedTheme((cur) => (cur === id ? null : cur));
    },
  });

  // ── CRUD handlers (window.prompt/confirm mirrors the sibling SongEditor) ──────
  const createTheme = () => {
    const suggested = uniqueThemeName(
      themes.map((th) => th.name),
      t("tcNewThemeName"),
    );
    const name = cleanThemeName(
      window.prompt(t("tcNewThemePrompt"), suggested),
    );
    if (name) createMut.mutate(name);
  };

  const renameTheme = () => {
    if (!selectedTheme || !editable) return;
    const name = cleanThemeName(
      window.prompt(t("tcRenamePrompt"), selectedTheme.name),
    );
    if (name) renameMut.mutate({ id: selectedTheme.id, name });
  };

  const deleteTheme = () => {
    if (!selectedTheme || !editable) return;
    setConfirmDelete(selectedTheme);
  };

  const updateTokensMut = useMutation({
    mutationFn: ({ id, tokens }: { id: string; tokens: ThemeTokens }) =>
      ipc.theme.updateTokens(id, tokens),
    onSuccess: (saved) => {
      qc.setQueryData<Theme[]>(["themes", libraryId], (old) =>
        (old ?? []).map((th) => (th.id === saved.id ? saved : th)),
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
      blocks: [newTextBlock(t("previewLabel"), { y: 0.35, h: 0.3 })],
    };
    return applyThemeToDoc(sample, editTokens);
  }, [editTokens, t]);

  return (
    <div className="space-y-4 border-b border-[var(--color-border)] p-4 text-sm">
      <section className="space-y-2">
        <Header>{t("tcTemplate")}</Header>
        <select
          value={templateId ?? ""}
          onChange={(e) => {
            const tpl = templates.find((x) => x.id === e.target.value);
            if (tpl) applyTemplate(tpl);
          }}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {templates.map((tpl) => (
            <option key={tpl.id} value={tpl.id}>
              {tpl.name}
              {Number(tpl.is_builtin) === 0 ? " ★" : ""}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={() => templateId && setDefaultTemplateMut.mutate(templateId)}
          disabled={!templateId}
          title={t("tcSetDefaultTemplateTitle")}
          className="flex w-full items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
        >
          <Star size={13} /> {t("arrSetDefault")}
        </button>
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <Header>{t("themeLabel")}</Header>
          <button
            type="button"
            onClick={createTheme}
            className="flex items-center gap-1 rounded-md border border-[var(--color-border)] px-1.5 py-1 text-[11px] text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <Plus size={12} /> {t("tcNewTheme")}
          </button>
        </div>
        <select
          value={themeId ?? ""}
          onChange={(e) => {
            const th = themes.find((x) => x.id === e.target.value);
            if (th) applyTheme(th);
          }}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {themes.map((th) => (
            <option key={th.id} value={th.id}>
              {th.name}
              {Number(th.is_builtin) === 0 ? " ★" : ""}
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
            <CopyPlus size={13} /> {t("actionDuplicate")}
          </button>
          <button
            type="button"
            onClick={() => themeId && setDefaultThemeMut.mutate(themeId)}
            disabled={!themeId}
            title={t("tcSetDefaultTitle")}
            className="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
          >
            <Star size={13} /> {t("arrSetDefault")}
          </button>
        </div>

        {editable && (
          <div className="flex gap-2">
            <button
              type="button"
              onClick={renameTheme}
              title={t("tcRenameTitle")}
              className="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
            >
              <Pencil size={13} /> {t("arrRename")}
            </button>
            <button
              type="button"
              onClick={deleteTheme}
              title={t("tcDeleteTitle")}
              className="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-[var(--color-border)] px-2 py-1.5 text-xs text-[var(--color-danger)] hover:bg-[var(--color-danger)]/10"
            >
              <Trash2 size={13} /> {t("actionDelete")}
            </button>
          </div>
        )}
      </section>

      {editable && editTokens ? (
        <section className="space-y-2.5">
          <Header>{t("tcEditTheme")}</Header>
          {previewDoc && (
            <div className="overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
              <SlideCanvas doc={previewDoc} width={240} height={135} />
            </div>
          )}
          <TokenRow label={t("inspBackground")}>
            <ColorInput
              value={editTokens.background.value}
              onChange={(value) =>
                editToken({ background: { ...editTokens.background, value } })
              }
            />
          </TokenRow>
          <TokenRow label={t("tcTextColor")}>
            <ColorInput
              value={editTokens.text_color}
              onChange={(text_color) => editToken({ text_color })}
            />
          </TokenRow>
          <TokenRow label={t("tcAccent")}>
            <ColorInput
              value={editTokens.accent_color}
              onChange={(accent_color) => editToken({ accent_color })}
            />
          </TokenRow>
          <TokenRow
            label={`${t("inspSize")} (${Math.round(editTokens.body_size)})`}
          >
            <input
              type="range"
              min={24}
              max={140}
              value={editTokens.body_size}
              onChange={(e) => editToken({ body_size: Number(e.target.value) })}
              className="w-28 accent-[var(--color-accent)]"
            />
          </TokenRow>
          <TokenRow label={t("inspWeight")}>
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
          <TokenRow label={t("inspShadow")}>
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
          {t("tcDuplicateHint")}
        </p>
      )}

      {confirmDelete && (
        <ConfirmModal
          title={t("tcDeleteTitle")}
          body={t("tcDeleteConfirm", { name: confirmDelete.name })}
          confirmLabel={t("actionDelete")}
          cancelLabel={t("actionCancel")}
          onConfirm={() => {
            deleteMut.mutate(confirmDelete.id);
            setConfirmDelete(null);
          }}
          onCancel={() => setConfirmDelete(null)}
        />
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
