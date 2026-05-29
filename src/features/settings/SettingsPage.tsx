/**
 * Settings — tabbed.
 *
 * Generelt (theme), Output (congregation-output appearance with live preview),
 * AI (Anthropic key/model/consent), Avansert (local crash reporting).
 */
import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  KeyRound,
  Loader2,
  Monitor,
  Settings2,
  ShieldAlert,
  Sparkles,
  Trash2,
} from "lucide-react";
import { emit } from "@tauri-apps/api/event";

import { ipc } from "@/lib/ipc";
import type { AiTestResult, LiveFrame, OutputAppearance } from "@/lib/bindings";
import {
  hasAiConsent,
  grantAiConsent,
  revokeAiConsent,
  preferredModel,
  setPreferredModel,
} from "@/lib/aiConsent";
import {
  OUTPUT_APPEARANCE,
  DEFAULT_OUTPUT_APPEARANCE,
} from "@/lib/outputBridge";
import { SlideView } from "@/components/SlideView";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Input,
  Select,
} from "@/components/ui";
import { ThemeToggle } from "@/components/ThemeToggle";
import { cn } from "@/lib/cn";
import { useT, type TKey } from "@/lib/i18n";

type Tab = "general" | "output" | "ai" | "advanced";

const TABS: Array<{ id: Tab; labelKey: TKey; icon: typeof Settings2 }> = [
  { id: "general", labelKey: "setGeneral", icon: Settings2 },
  { id: "output", labelKey: "setTabOutput", icon: Monitor },
  { id: "ai", labelKey: "setTabAi", icon: Sparkles },
  { id: "advanced", labelKey: "setAdvanced", icon: ShieldAlert },
];

export function SettingsPage() {
  const t = useT();
  const [tab, setTab] = useState<Tab>("general");

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-[var(--color-border)] px-8 pt-8">
        <h1 className="text-[var(--text-ui-3xl)] font-bold">
          {t("navSettings")}
        </h1>
        <div className="mt-4 flex gap-1">
          {TABS.map((tabItem) => {
            const Icon = tabItem.icon;
            return (
              <button
                key={tabItem.id}
                type="button"
                onClick={() => setTab(tabItem.id)}
                className={cn(
                  "flex items-center gap-2 rounded-t-md border-b-2 px-4 py-2 text-sm font-medium transition-colors",
                  tab === tabItem.id
                    ? "border-[var(--color-accent)] text-[var(--color-fg)]"
                    : "border-transparent text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]",
                )}
              >
                <Icon size={15} aria-hidden />
                {t(tabItem.labelKey)}
              </button>
            );
          })}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-8 py-8">
          {tab === "general" && <GeneralSettings />}
          {tab === "output" && <OutputSettings />}
          {tab === "ai" && <AiSettings />}
          {tab === "advanced" && <AdvancedSettings />}
        </div>
      </div>
    </div>
  );
}

function GeneralSettings() {
  const t = useT();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("setAppearance")}</CardTitle>
        <CardDescription>{t("setAppearanceDesc")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center justify-between">
          <span className="text-sm text-[var(--color-fg-muted)]">
            {t("themeLabel")}
          </span>
          <ThemeToggle />
        </div>
      </CardContent>
    </Card>
  );
}

function OutputSettings() {
  const t = useT();
  const qc = useQueryClient();
  const sampleFrame: LiveFrame = {
    kind: "slide",
    slide_content: {
      section_label: t("setSampleSection"),
      text_lines: [t("setSampleLine1"), t("setSampleLine2")],
      translation_lines: null,
      reference: null,
    },
  };
  const [draft, setDraft] = useState<OutputAppearance>(
    DEFAULT_OUTPUT_APPEARANCE,
  );
  const loaded = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );

  const appearanceQuery = useQuery({
    queryKey: ["outputAppearance"],
    queryFn: () => ipc.output.appearance(),
  });

  // Seed the editor once the saved appearance arrives.
  useEffect(() => {
    if (appearanceQuery.data && !loaded.current) {
      loaded.current = true;
      setDraft(appearanceQuery.data);
    }
  }, [appearanceQuery.data]);

  // Persist + broadcast (debounced) so sliders feel instant but don't thrash
  // the disk; the open output windows restyle live via the emitted event.
  function update(patch: Partial<OutputAppearance>) {
    const next = { ...draft, ...patch };
    setDraft(next);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      void ipc.output
        .setAppearance(next)
        .then((saved) => {
          void emit(OUTPUT_APPEARANCE, saved);
          qc.setQueryData(["outputAppearance"], saved);
        })
        .catch(() => {});
    }, 150);
  }

  function reset() {
    update(DEFAULT_OUTPUT_APPEARANCE);
  }

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Monitor size={18} className="text-[var(--color-accent)]" />
            {t("setOutputAppearance")}
          </CardTitle>
          <CardDescription>{t("setOutputAppearanceDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          {/* Live preview */}
          <div className="overflow-hidden rounded-lg ring-1 ring-[var(--color-border)]">
            <div className="aspect-video w-full">
              <SlideView frame={sampleFrame} appearance={draft} />
            </div>
          </div>

          <RangeRow
            label={t("setTextSize")}
            min={0.5}
            max={2.5}
            step={0.05}
            value={draft.text_scale}
            format={(v) => `${Math.round(v * 100)}%`}
            onChange={(v) => update({ text_scale: v })}
          />
          <RangeRow
            label={t("setLineHeight")}
            min={0.9}
            max={2.5}
            step={0.05}
            value={draft.line_height}
            format={(v) => v.toFixed(2)}
            onChange={(v) => update({ line_height: v })}
          />

          <div className="flex flex-wrap gap-6">
            <ColorRow
              label={t("tcTextColor")}
              value={draft.text_color}
              onChange={(v) => update({ text_color: v })}
            />
            <ColorRow
              label={t("inspBackground")}
              value={draft.bg_color}
              onChange={(v) => update({ bg_color: v })}
            />
            <div>
              <label className="mb-1 block text-xs text-[var(--color-fg-muted)]">
                {t("inspAlign")}
              </label>
              <Select
                className="w-32"
                value={draft.h_align}
                onChange={(e) =>
                  update({
                    h_align: e.target.value as OutputAppearance["h_align"],
                  })
                }
              >
                <option value="left">{t("setAlignLeft")}</option>
                <option value="center">{t("setAlignCenter")}</option>
                <option value="right">{t("setAlignRight")}</option>
              </Select>
            </div>
          </div>

          <ToggleRow
            label={t("setShowSectionLabel")}
            description={t("setShowSectionLabelDesc")}
            checked={draft.show_section_label}
            onChange={(v) => update({ show_section_label: v })}
          />
          <ToggleRow
            label={t("setUppercase")}
            description={t("setUppercaseDesc")}
            checked={draft.uppercase}
            onChange={(v) => update({ uppercase: v })}
          />

          <div className="flex justify-end">
            <Button variant="ghost" size="sm" onClick={reset}>
              {t("setResetDefault")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function RangeRow({
  label,
  min,
  max,
  step,
  value,
  format,
  onChange,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-sm">
        <span className="text-[var(--color-fg-muted)]">{label}</span>
        <span className="font-mono text-xs text-[var(--color-fg)]">
          {format(value)}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-[var(--color-accent)]"
      />
    </div>
  );
}

function ColorRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <label className="mb-1 block text-xs text-[var(--color-fg-muted)]">
        {label}
      </label>
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="h-9 w-12 cursor-pointer rounded border border-[var(--color-border)] bg-transparent"
        />
        <span className="font-mono text-xs text-[var(--color-fg-muted)]">
          {value}
        </span>
      </div>
    </div>
  );
}

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-sm">{label}</div>
        <div className="text-xs text-[var(--color-fg-muted)]">
          {description}
        </div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-6 w-11 shrink-0 rounded-full transition-colors",
          checked
            ? "bg-[var(--color-accent)]"
            : "bg-[var(--color-bg-surface)] ring-1 ring-[var(--color-border)]",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition-transform",
            checked && "translate-x-5",
          )}
        />
      </button>
    </div>
  );
}

function AiSettings() {
  const t = useT();
  const qc = useQueryClient();
  const [keyInput, setKeyInput] = useState("");
  const [model, setModel] = useState(preferredModel() ?? "claude-sonnet-4-6");
  const [test, setTest] = useState<AiTestResult | null>(null);
  const [consent, setConsent] = useState(hasAiConsent());

  const statusQuery = useQuery({
    queryKey: ["aiKeyStatus"],
    queryFn: () => ipc.ai.keyStatus(),
  });
  const modelsQuery = useQuery({
    queryKey: ["aiModels"],
    queryFn: () => ipc.ai.models(),
  });
  const saveKey = useMutation({
    mutationFn: () => ipc.ai.keySet(keyInput.trim()),
    onSuccess: () => {
      setKeyInput("");
      void qc.invalidateQueries({ queryKey: ["aiKeyStatus"] });
    },
  });
  const clearKey = useMutation({
    mutationFn: () => ipc.ai.keyClear(),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["aiKeyStatus"] }),
  });
  const testConn = useMutation({
    mutationFn: () => ipc.ai.testConnection(model),
    onSuccess: setTest,
  });

  const status = statusQuery.data;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <KeyRound size={18} className="text-[var(--color-accent)]" />
            {t("setAiTitle")}
          </CardTitle>
          <CardDescription>{t("setKeyStoredDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-[var(--color-fg-muted)]">
              {t("setStatus")}
            </span>
            {status?.stored ? (
              <Badge variant="success">{t("setStoredInKeychain")}</Badge>
            ) : status?.env ? (
              <Badge variant="neutral">{t("setFromEnv")}</Badge>
            ) : (
              <Badge variant="warning">{t("setNoKey")}</Badge>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Input
              type="password"
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              placeholder="sk-ant-…"
            />
            <Button
              onClick={() => saveKey.mutate()}
              disabled={keyInput.trim().length === 0 || saveKey.isPending}
            >
              {t("actionSave")}
            </Button>
            {status?.stored && (
              <Button
                variant="outline"
                size="icon"
                title={t("setClearKey")}
                onClick={() => clearKey.mutate()}
                disabled={clearKey.isPending}
              >
                <Trash2 size={15} />
              </Button>
            )}
          </div>

          <div className="flex items-center gap-2">
            <label className="text-sm text-[var(--color-fg-muted)]">
              {t("setDefaultModel")}
            </label>
            <Select
              className="max-w-xs"
              value={model}
              onChange={(e) => {
                setModel(e.target.value);
                setPreferredModel(e.target.value);
              }}
            >
              {(modelsQuery.data ?? []).map((m) => (
                <option key={m.id} value={m.id}>
                  {m.display}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex items-center gap-3">
            <Button
              variant="secondary"
              onClick={() => testConn.mutate()}
              disabled={testConn.isPending}
            >
              {testConn.isPending && (
                <Loader2 size={14} className="animate-spin" />
              )}
              {t("setTestConnection")}
            </Button>
            {test && (
              <span
                className={
                  test.ok
                    ? "flex items-center gap-1 text-sm text-[var(--color-success)]"
                    : "text-sm text-[var(--color-danger)]"
                }
              >
                {test.ok && <Check size={14} />}
                {test.message}
              </span>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("setAiPrivacy")}</CardTitle>
          <CardDescription>{t("setAiPrivacyDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-[var(--color-fg-muted)]">
              {t("setConsentLabel")}
            </span>
            {consent ? (
              <Badge variant="success">{t("setGiven")}</Badge>
            ) : (
              <Badge variant="neutral">{t("setNotGiven")}</Badge>
            )}
          </div>
          {consent ? (
            <Button
              variant="outline"
              onClick={() => {
                revokeAiConsent();
                setConsent(false);
              }}
            >
              {t("setRevoke")}
            </Button>
          ) : (
            <Button
              onClick={() => {
                grantAiConsent();
                setConsent(true);
              }}
            >
              {t("setGiveConsent")}
            </Button>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function AdvancedSettings() {
  const t = useT();
  const qc = useQueryClient();
  const crashStatus = useQuery({
    queryKey: ["crashStatus"],
    queryFn: () => ipc.crash.status(),
  });
  const crashCount = useQuery({
    queryKey: ["crashCount"],
    queryFn: () => ipc.crash.count(),
  });
  const setCrash = useMutation({
    mutationFn: (enabled: boolean) => ipc.crash.set(enabled),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["crashStatus"] }),
  });
  const clearCrashes = useMutation({
    mutationFn: () => ipc.crash.clear(),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["crashCount"] }),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("setCrashReporting")}</CardTitle>
        <CardDescription>{t("setCrashDesc")}</CardDescription>
      </CardHeader>
      <CardContent className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm">
          <span className="text-[var(--color-fg-muted)]">{t("setStatus")}</span>
          {crashStatus.data ? (
            <Badge variant="success">{t("setOn")}</Badge>
          ) : (
            <Badge variant="neutral">{t("setOff")}</Badge>
          )}
          {(crashCount.data ?? 0) > 0 && (
            <span className="text-xs text-[var(--color-fg-muted)]">
              {t("setCrashCount", { n: crashCount.data ?? 0 })}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {(crashCount.data ?? 0) > 0 && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => clearCrashes.mutate()}
            >
              {t("actionClear")}
            </Button>
          )}
          <Button
            variant={crashStatus.data ? "outline" : "primary"}
            size="sm"
            onClick={() => setCrash.mutate(!crashStatus.data)}
          >
            {crashStatus.data ? t("setTurnOff") : t("setTurnOn")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
