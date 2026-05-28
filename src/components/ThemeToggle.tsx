import { Monitor, Sun, Moon, type LucideIcon } from "lucide-react";

import { useTheme, type ThemeMode } from "@/lib/theme";
import { cn } from "@/lib/cn";

const OPTIONS: Array<{ mode: ThemeMode; icon: LucideIcon; label: string }> = [
  { mode: "system", icon: Monitor, label: "System" },
  { mode: "light", icon: Sun, label: "Lyst" },
  { mode: "dark", icon: Moon, label: "Mørkt" },
];

export function ThemeToggle({ className }: { className?: string }) {
  const mode = useTheme((s) => s.mode);
  const setMode = useTheme((s) => s.setMode);

  return (
    <div
      role="radiogroup"
      aria-label="Tema"
      className={cn(
        "flex items-center gap-0.5 rounded-lg border border-[var(--color-border)] p-0.5",
        className,
      )}
    >
      {OPTIONS.map(({ mode: m, icon: Icon, label }) => (
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
      ))}
    </div>
  );
}
