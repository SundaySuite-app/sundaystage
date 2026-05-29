/**
 * Phase 5.2 — output-display controls in the operator console.
 *
 * Lists the connected monitors, lets the operator assign each a role, and
 * opens/closes the borderless full-screen output windows. Anchored as a small
 * non-blocking panel (never a modal dialog during live).
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Monitor } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { DisplayRole, OutputConfig } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { Button, Select } from "@/components/ui";

const ROLE_LABEL: Record<DisplayRole, string> = {
  off: "Av",
  main_output: "Hovedutgang",
  stage_display: "Sceneskjerm",
  confidence_monitor: "Confidence",
};
const ROLES: DisplayRole[] = [
  "off",
  "main_output",
  "stage_display",
  "confidence_monitor",
];

export function OutputControls() {
  const [open, setOpen] = useState(false);

  const monitorsQuery = useQuery({
    queryKey: ["outputMonitors"],
    queryFn: () => ipc.output.monitors(),
  });
  const configQuery = useQuery({
    queryKey: ["outputConfig"],
    queryFn: () => ipc.output.config(),
  });
  const isOpenQuery = useQuery({
    queryKey: ["outputIsOpen"],
    queryFn: () => ipc.output.isOpen(),
    refetchInterval: 2000,
  });

  const monitors = monitorsQuery.data ?? [];
  const config = configQuery.data;
  const driving = isOpenQuery.data ?? false;

  // The screen currently carrying the congregation output, for an at-a-glance
  // label ("Utgang: Skjerm 2") instead of a bare monitor count.
  const mainOutput = monitors.find(
    (m) =>
      config?.assignments.find((a) => a.monitor_index === m.index)?.role ===
      "main_output",
  );

  function roleFor(index: number): DisplayRole {
    return (
      config?.assignments.find((a) => a.monitor_index === index)?.role ?? "off"
    );
  }

  async function setRole(monitorIndex: number, role: DisplayRole) {
    if (!config) return;
    const next: OutputConfig = {
      assignments: config.assignments.map((a) =>
        a.monitor_index === monitorIndex ? { ...a, role } : a,
      ),
    };
    await ipc.output.setConfig(next);
    await configQuery.refetch();
    if (driving) await ipc.output.open(); // re-apply to live windows
  }

  async function toggle() {
    if (driving) await ipc.output.close();
    else await ipc.output.open();
    await isOpenQuery.refetch();
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]"
        title="Skjermer og utganger"
      >
        <span
          className={cn(
            "h-2 w-2 rounded-full",
            driving ? "bg-[var(--color-success)]" : "bg-[var(--color-warning)]",
          )}
        />
        <Monitor size={13} />
        {driving && mainOutput
          ? `Utgang: ${mainOutput.name}`
          : driving
            ? "Utgang aktiv"
            : `${monitors.length} skjerm${monitors.length === 1 ? "" : "er"}`}
      </button>

      {open && (
        <div className="absolute right-0 bottom-full mb-2 w-80 rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3 text-[var(--color-fg)] shadow-[var(--shadow-elevated)]">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-xs font-semibold tracking-wide uppercase">
              Skjermer
            </h3>
            <Button
              size="sm"
              variant={driving ? "outline" : "primary"}
              onClick={toggle}
            >
              {driving ? "Lukk utgang" : "Åpne utgang"}
            </Button>
          </div>

          {monitors.length === 0 ? (
            <p className="py-3 text-center text-xs text-[var(--color-fg-muted)]">
              Ingen skjermer funnet.
            </p>
          ) : (
            <ul className="space-y-2">
              {monitors.map((m) => (
                <li key={m.index} className="flex items-center gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 truncate text-xs font-medium">
                      {m.name}
                      {m.is_primary && (
                        <span className="rounded bg-[var(--color-bg-surface)] px-1 text-[9px] text-[var(--color-fg-muted)]">
                          operatør
                        </span>
                      )}
                    </div>
                    <div className="font-mono text-[10px] text-[var(--color-fg-muted)]">
                      {m.width}×{m.height}
                    </div>
                  </div>
                  <Select
                    className="w-36"
                    value={roleFor(m.index)}
                    onChange={(e) =>
                      setRole(m.index, e.target.value as DisplayRole)
                    }
                  >
                    {ROLES.map((r) => (
                      <option key={r} value={r}>
                        {ROLE_LABEL[r]}
                      </option>
                    ))}
                  </Select>
                </li>
              ))}
            </ul>
          )}
          <p className="mt-2 text-[10px] text-[var(--color-fg-muted)]">
            Hovedutgang vises rent; sceneskjerm får klokke og varsel ved tap av
            forbindelse.
          </p>
        </div>
      )}
    </div>
  );
}
