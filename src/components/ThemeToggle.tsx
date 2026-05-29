import { Monitor, Sun, Moon, type LucideIcon } from "lucide-react";

import { useTheme, type ThemeMode } from "@/lib/theme";
import { cn } from "@/lib/cn";
import { useT, type TKey } from "@/lib/i18n";

const OPTIONS: Array<{ mode: ThemeMode; icon: LucideIcon; labelKey: TKey }> = [
  { mode: "system", icon: Monitor, labelKey: "themeSystem" },
  { mode: "light", icon: Sun, labelKey: "themeLight" },
  { mode: "dark", icon: Moon, labelKey: "themeDark" },
];

export function ThemeToggle({ className }: { className?: string }) {
  const t = useT();
  const mode = useTheme((s) => s.mode);
  const setMode = useTheme((s) => s.setMode);

  return (
    <div
      role="radiogroup"
      aria-label={t("themeLabel")}
      className={cn(
        "flex items-center gap-0.5 rounded-lg border border-[var(--color-border)] p-0.5",
        className,
      )}
    >
      {OPTIONS.map(({ mode: m, icon: Icon, labelKey }) => {
        const label = t(labelKey);
        return (
          <button
            key={m}
            type="button"
            role="radio"
            aria-checked={mode === m}
            aria-label={label}
            title={label}
            onClick={() => setMode(m)}
            className={cn(
              "grid h-7 w-7 place-items-center rounded-md transition-colors",
              mode === m
                ? "bg-[var(--color-bg-surface)] text-[var(--color-fg)]"
                : "text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]",
            )}
          >
            <Icon size={14} aria-hidden />
          </button>
        );
      })}
    </div>
  );
}
