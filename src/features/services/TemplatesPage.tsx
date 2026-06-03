/**
 * Service templates page — browse built-in + user-defined templates, create
 * new ones, and apply them to existing services.
 *
 * A template is an ordered list of CueSpec slots (kind + label + optional
 * notes). Applying it to a service appends one service_item per slot.
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BookOpen,
  FileText,
  Megaphone,
  Music,
  Play,
  Plus,
  Trash2,
  Video,
  X,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { CueSpec, ServiceTemplate } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { Button } from "@/components/ui";
import {
  DEFAULT_ROLE,
  TEMPLATE_ROLES,
  getTemplateRole,
  panelsForRole,
  roleLabelKey,
  setTemplateRole,
  type RolePanels,
  type TemplateRole,
} from "./templateRoles";

const KIND_ICON: Record<string, typeof Music> = {
  song: Music,
  bible: BookOpen,
  prayer: FileText,
  announcement: Megaphone,
  media: Video,
};

export function TemplatesPage({
  libraryId,
  onApplied,
}: {
  libraryId: string;
  onApplied?: (serviceId: string) => void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [creating, setCreating] = useState(false);
  const [applyTemplate, setApplyTemplate] = useState<ServiceTemplate | null>(
    null,
  );
  const [toast, setToast] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<ServiceTemplate | null>(
    null,
  );
  // The template whose stage-display role is being inspected/edited.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // Per-device role assignments, keyed by template id. Hydrated lazily from
  // localStorage; the inspector preview re-renders the instant this changes.
  const [roles, setRoles] = useState<Record<string, TemplateRole>>({});

  function roleFor(id: string): TemplateRole {
    return roles[id] ?? getTemplateRole(id);
  }

  function assignRole(id: string, role: TemplateRole) {
    setTemplateRole(id, role);
    setRoles((prev) => ({ ...prev, [id]: role }));
  }

  const templatesQuery = useQuery({
    queryKey: ["serviceTemplates"],
    queryFn: () => ipc.serviceTemplate.list(),
  });
  const templates = templatesQuery.data ?? [];
  const builtins = templates.filter((t) => t.is_builtin === BigInt(1));
  const custom = templates.filter((t) => t.is_builtin !== BigInt(1));

  const deleteTemplate = useMutation({
    mutationFn: (id: string) => ipc.serviceTemplate.delete(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["serviceTemplates"] });
      setConfirmDelete(null);
    },
  });

  function showToast(msg: string) {
    setToast(msg);
    setTimeout(() => setToast(null), 3500);
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">
          {t("tmplPageTitle")}
        </h1>
        <div className="flex-1" />
        <Button onClick={() => setCreating(true)}>
          <Plus size={14} aria-hidden />
          {t("tmplCreate")}
        </Button>
      </header>

      {toast && (
        <div className="fixed bottom-4 left-1/2 z-50 flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-lg border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] px-4 py-2 text-sm shadow-[var(--shadow-elevated)]">
          <span>{toast}</span>
          <button
            onClick={() => setToast(null)}
            className="text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
          >
            <X size={14} />
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          {/* Built-in templates */}
          <section className="mb-8">
            <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--color-fg-muted)]">
              {t("tmplBuiltin")}
            </h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {builtins.map((tmpl) => (
                <TemplateCard
                  key={tmpl.id}
                  template={tmpl}
                  role={roleFor(tmpl.id)}
                  selected={tmpl.id === selectedId}
                  onSelect={() => setSelectedId(tmpl.id)}
                  onRoleChange={(role) => assignRole(tmpl.id, role)}
                  onApply={() => setApplyTemplate(tmpl)}
                  onDelete={null}
                />
              ))}
            </div>
          </section>

          {/* Custom templates */}
          <section>
            <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--color-fg-muted)]">
              {t("tmplCustom")}
            </h2>
            {custom.length === 0 ? (
              <div className="rounded-xl border border-dashed border-[var(--color-border)] p-8 text-center">
                <p className="font-semibold text-[var(--color-fg-muted)]">
                  {t("tmplEmptyTitle")}
                </p>
                <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
                  {t("tmplEmptyBody")}
                </p>
                <Button className="mt-4" onClick={() => setCreating(true)}>
                  <Plus size={14} aria-hidden />
                  {t("tmplCreate")}
                </Button>
              </div>
            ) : (
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {custom.map((tmpl) => (
                  <TemplateCard
                    key={tmpl.id}
                    template={tmpl}
                    role={roleFor(tmpl.id)}
                    selected={tmpl.id === selectedId}
                    onSelect={() => setSelectedId(tmpl.id)}
                    onRoleChange={(role) => assignRole(tmpl.id, role)}
                    onApply={() => setApplyTemplate(tmpl)}
                    onDelete={() => setConfirmDelete(tmpl)}
                  />
                ))}
              </div>
            )}
          </section>
        </div>

        {/* Stage-display role inspector — live preview of the selected role */}
        <RoleInspector
          template={
            selectedId
              ? (templates.find((t) => t.id === selectedId) ?? null)
              : null
          }
          role={selectedId ? roleFor(selectedId) : DEFAULT_ROLE}
        />
      </div>

      {/* Create modal */}
      {creating && (
        <CreateTemplateModal
          onCreated={() => {
            setCreating(false);
            void qc.invalidateQueries({ queryKey: ["serviceTemplates"] });
          }}
          onClose={() => setCreating(false)}
        />
      )}

      {/* Apply to service modal */}
      {applyTemplate && (
        <ApplyTemplateModal
          template={applyTemplate}
          libraryId={libraryId}
          onApplied={(serviceId, count, serviceName) => {
            setApplyTemplate(null);
            showToast(
              t("tmplApplyDone", {
                n: count,
                service: serviceName,
              }),
            );
            onApplied?.(serviceId);
          }}
          onClose={() => setApplyTemplate(null)}
        />
      )}

      {/* Confirm delete */}
      {confirmDelete && (
        <ConfirmModal
          title={t("tmplConfirmDeleteTitle")}
          body={t("tmplConfirmDeleteBody", { name: confirmDelete.name })}
          confirmLabel={t("actionDelete")}
          onConfirm={() => deleteTemplate.mutate(confirmDelete.id)}
          onCancel={() => setConfirmDelete(null)}
        />
      )}
    </div>
  );
}

function TemplateCard({
  template,
  role,
  selected,
  onSelect,
  onRoleChange,
  onApply,
  onDelete,
}: {
  template: ServiceTemplate;
  role: TemplateRole;
  selected: boolean;
  onSelect: () => void;
  onRoleChange: (role: TemplateRole) => void;
  onApply: () => void;
  onDelete: (() => void) | null;
}) {
  const t = useT();
  const specs = ipc.parseCueSpecs(template);

  return (
    <div
      onClick={onSelect}
      className={cn(
        "flex cursor-pointer flex-col rounded-xl border bg-[var(--color-bg-elevated)] p-4 transition-colors",
        selected
          ? "border-[var(--color-accent)] ring-1 ring-[var(--color-accent)]"
          : "border-[var(--color-border)] hover:border-[var(--color-accent)]/50",
      )}
    >
      <div className="mb-2 flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <h3 className="truncate font-semibold">{template.name}</h3>
          {template.description && (
            <p className="mt-0.5 text-xs text-[var(--color-fg-muted)] line-clamp-2">
              {template.description}
            </p>
          )}
        </div>
        {onDelete && (
          <button
            type="button"
            onClick={onDelete}
            title={t("tmplDelete")}
            className="shrink-0 rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)]"
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>

      {/* Slot preview — show first 6 */}
      <div className="mb-3 min-h-0 flex-1">
        <div className="space-y-0.5">
          {specs.slice(0, 6).map((spec, i) => (
            <SlotRow key={i} spec={spec} />
          ))}
          {specs.length > 6 && (
            <p className="pl-5 text-[11px] text-[var(--color-fg-muted)]">
              +{specs.length - 6} more…
            </p>
          )}
        </div>
      </div>

      {/* Stage-display role assignment */}
      <label className="mb-3 block" onClick={(e) => e.stopPropagation()}>
        <span className="mb-1 block text-[11px] text-[var(--color-fg-muted)]">
          {t("tmplRole")}
        </span>
        <select
          aria-label={t("tmplRole")}
          value={role}
          onChange={(e) => onRoleChange(e.target.value as TemplateRole)}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
        >
          {TEMPLATE_ROLES.map((r) => (
            <option key={r} value={r}>
              {t(roleLabelKey(r))}
            </option>
          ))}
        </select>
      </label>

      <div className="flex items-center justify-between border-t border-[var(--color-border)] pt-3">
        <span className="text-xs text-[var(--color-fg-muted)]">
          {t("tmplCueSpecs", { n: specs.length })}
        </span>
        <Button
          size="sm"
          onClick={(e) => {
            e.stopPropagation();
            onApply();
          }}
        >
          <Play size={12} aria-hidden fill="currentColor" />
          {t("tmplApply")}
        </Button>
      </div>
    </div>
  );
}

/**
 * Live preview of the stage display for the selected template's role. Reflects
 * the same panel toggles as the Rust `StageDisplayConfig` presets, updating the
 * instant the role changes in the card dropdown.
 */
function RoleInspector({
  template,
  role,
}: {
  template: ServiceTemplate | null;
  role: TemplateRole;
}) {
  const t = useT();

  if (!template) {
    return (
      <aside className="hidden w-72 shrink-0 overflow-y-auto border-l border-[var(--color-border)] p-5 lg:block">
        <p className="text-sm text-[var(--color-fg-muted)]">
          {t("tmplRoleSelectHint")}
        </p>
      </aside>
    );
  }

  const panels = panelsForRole(role);
  const rows: { key: keyof RolePanels; label: string }[] = [
    { key: "showCurrentSlide", label: t("tmplRolePanelCurrentSlide") },
    { key: "showNextSlide", label: t("tmplRolePanelNextSlide") },
    { key: "lyricsLarge", label: t("tmplRolePanelLyricsLarge") },
    { key: "showSectionLabel", label: t("tmplRolePanelSectionLabel") },
    { key: "showClock", label: t("tmplRolePanelClock") },
    { key: "showServiceTimer", label: t("tmplRolePanelServiceTimer") },
    { key: "showNotes", label: t("tmplRolePanelNotes") },
  ];

  return (
    <aside className="hidden w-72 shrink-0 overflow-y-auto border-l border-[var(--color-border)] p-5 lg:block">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-[var(--color-fg-muted)]">
        {t("tmplRolePreviewTitle")}
      </h2>
      <p className="mt-1 truncate text-sm font-semibold">{template.name}</p>
      <p className="mt-0.5 text-xs text-[var(--color-accent)]">
        {t(roleLabelKey(role))}
      </p>
      <p className="mt-3 text-[11px] text-[var(--color-fg-muted)]">
        {t("tmplRolePreviewHint")}
      </p>
      <ul className="mt-2 space-y-1" data-testid="role-preview-panels">
        {rows.map((r) => (
          <li
            key={r.key}
            data-panel={r.key}
            data-on={panels[r.key] ? "1" : "0"}
            className={cn(
              "flex items-center gap-2 rounded-md px-2 py-1 text-sm",
              panels[r.key]
                ? "bg-[var(--color-accent)]/10 text-[var(--color-fg)]"
                : "text-[var(--color-fg-muted)] line-through opacity-50",
            )}
          >
            <span
              className={cn(
                "inline-block h-2 w-2 shrink-0 rounded-full",
                panels[r.key]
                  ? "bg-[var(--color-accent)]"
                  : "bg-[var(--color-border)]",
              )}
              aria-hidden
            />
            {r.label}
          </li>
        ))}
      </ul>
    </aside>
  );
}

function SlotRow({ spec }: { spec: CueSpec }) {
  const Icon = KIND_ICON[spec.kind] ?? FileText;
  return (
    <div className="flex items-center gap-1.5 text-[11px] text-[var(--color-fg-muted)]">
      <Icon size={11} aria-hidden className="shrink-0" />
      <span className="truncate">{spec.label}</span>
    </div>
  );
}

function CreateTemplateModal({
  onCreated,
  onClose,
}: {
  onCreated: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [specs, setSpecs] = useState<CueSpec[]>([
    { kind: "song", label: "", notes: null },
  ]);

  const createMutation = useMutation({
    mutationFn: () =>
      ipc.serviceTemplate.create({
        name: name.trim(),
        description: description.trim() || null,
        cue_specs: specs.filter((s) => s.label.trim()),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["serviceTemplates"] });
      onCreated();
    },
  });

  function addSlot() {
    setSpecs((prev) => [...prev, { kind: "song", label: "", notes: null }]);
  }

  function removeSlot(i: number) {
    setSpecs((prev) => prev.filter((_, idx) => idx !== i));
  }

  function updateSlot(i: number, patch: Partial<CueSpec>) {
    setSpecs((prev) =>
      prev.map((s, idx) => (idx === i ? { ...s, ...patch } : s)),
    );
  }

  const valid =
    name.trim().length > 0 && specs.some((s) => s.label.trim().length > 0);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative flex max-h-[90vh] w-[min(95vw,600px)] flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-5 py-4">
          <h2 className="flex-1 font-semibold">{t("tmplCreate")}</h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {/* Name */}
          <label className="block text-sm">
            <span className="mb-1 block text-xs text-[var(--color-fg-muted)]">
              {t("tmplNewName")}
            </span>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-3 py-2 text-sm focus:border-[var(--color-accent)] focus:outline-none"
            />
          </label>

          {/* Description */}
          <label className="block text-sm">
            <span className="mb-1 block text-xs text-[var(--color-fg-muted)]">
              {t("tmplNewDescription")}
            </span>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-3 py-2 text-sm focus:border-[var(--color-accent)] focus:outline-none"
            />
          </label>

          {/* Slots */}
          <div>
            <span className="mb-2 block text-xs font-semibold text-[var(--color-fg-muted)]">
              {t("tmplNewSlots")}
            </span>
            <div className="space-y-2">
              {specs.map((spec, i) => (
                <div key={i} className="flex items-start gap-2">
                  <select
                    value={spec.kind}
                    onChange={(e) => updateSlot(i, { kind: e.target.value })}
                    className="w-36 shrink-0 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
                  >
                    <option value="song">{t("tmplKindSong")}</option>
                    <option value="bible">{t("tmplKindBible")}</option>
                    <option value="prayer">{t("tmplKindPrayer")}</option>
                    <option value="announcement">
                      {t("tmplKindAnnouncement")}
                    </option>
                    <option value="media">{t("tmplKindMedia")}</option>
                  </select>
                  <input
                    placeholder={t("tmplSlotLabel")}
                    value={spec.label}
                    onChange={(e) => updateSlot(i, { label: e.target.value })}
                    className="min-w-0 flex-1 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
                  />
                  <input
                    placeholder={t("tmplSlotNotes")}
                    value={spec.notes ?? ""}
                    onChange={(e) =>
                      updateSlot(i, {
                        notes: e.target.value || null,
                      })
                    }
                    className="w-32 shrink-0 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => removeSlot(i)}
                    disabled={specs.length <= 1}
                    className="shrink-0 rounded p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)] disabled:opacity-30"
                  >
                    <X size={14} />
                  </button>
                </div>
              ))}
            </div>
            <button
              type="button"
              onClick={addSlot}
              className="mt-2 flex items-center gap-1 text-xs text-[var(--color-accent)] hover:underline"
            >
              <Plus size={12} />
              {t("tmplAddSlot")}
            </button>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--color-border)] px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            {t("actionCancel")}
          </Button>
          <Button
            disabled={!valid || createMutation.isPending}
            onClick={() => createMutation.mutate()}
          >
            {t("actionSave")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ApplyTemplateModal({
  template,
  libraryId,
  onApplied,
  onClose,
}: {
  template: ServiceTemplate;
  libraryId: string;
  onApplied: (serviceId: string, count: number, serviceName: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const servicesQuery = useQuery({
    queryKey: ["services", libraryId],
    queryFn: () => ipc.service.upcoming(libraryId, 0, 100),
  });
  const services = servicesQuery.data ?? [];

  const applyMutation = useMutation({
    mutationFn: () => {
      if (!selectedId) throw new Error("no service selected");
      return ipc.serviceTemplate.apply(template.id, selectedId);
    },
    onSuccess: (items) => {
      const svc = services.find((s) => s.id === selectedId);
      onApplied(selectedId!, items.length, svc?.name ?? "");
    },
  });

  return (
    <div className="fixed inset-0 z-50 grid place-items-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative w-[min(90vw,440px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]">
        <h2 className="mb-1 font-semibold">{template.name}</h2>
        <p className="mb-4 text-sm text-[var(--color-fg-muted)]">
          {t("tmplApplySelectService")}
        </p>

        {services.length === 0 ? (
          <p className="text-sm text-[var(--color-fg-muted)]">
            {t("svcListEmpty")}
          </p>
        ) : (
          <ul className="max-h-56 space-y-0.5 overflow-y-auto">
            {services.map((svc) => (
              <li key={svc.id}>
                <button
                  type="button"
                  onClick={() => setSelectedId(svc.id)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors",
                    svc.id === selectedId
                      ? "bg-[var(--color-accent)]/15 ring-1 ring-[var(--color-accent)]"
                      : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]",
                  )}
                >
                  {svc.name}
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t("actionCancel")}
          </Button>
          <Button
            disabled={!selectedId || applyMutation.isPending}
            onClick={() => applyMutation.mutate()}
          >
            {applyMutation.isPending ? t("tmplApplying") : t("tmplApply")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ConfirmModal({
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onCancel}
        aria-hidden
      />
      <div className="relative w-[min(90vw,420px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]">
        <h2 className="font-semibold">{title}</h2>
        <p className="mt-1 text-sm text-[var(--color-fg-muted)]">{body}</p>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onCancel}>
            Avbryt
          </Button>
          <button
            type="button"
            onClick={onConfirm}
            className="rounded-md bg-[var(--color-danger)] px-4 py-1.5 text-sm font-semibold text-white hover:brightness-110"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
