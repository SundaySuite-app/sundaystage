/**
 * A label + description + switch row, as the settings cards use it.
 *
 * Lifted out of `SettingsPage` when E6's privacy card needed the same row: two
 * copies of a switch is two chances for the "on" state to look different in the
 * one place an operator checks whether telemetry is off.
 */
import { cn } from "@/lib/cn";

export interface ToggleRowProps {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}

export function ToggleRow({
  label,
  description,
  checked,
  onChange,
  disabled = false,
}: ToggleRowProps) {
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
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-50",
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
