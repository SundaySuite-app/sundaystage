import { useState, type ReactNode } from "react";

import { cn } from "@/lib/cn";

interface TooltipProps {
  label: string;
  children: ReactNode;
  side?: "top" | "bottom";
  className?: string;
}

// CSS/state hover tooltip. Shows on hover + keyboard focus.
export function Tooltip({
  label,
  children,
  side = "top",
  className,
}: TooltipProps) {
  const [open, setOpen] = useState(false);
  return (
    <span
      className="relative inline-flex"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      {children}
      {open && (
        <span
          role="tooltip"
          className={cn(
            "pointer-events-none absolute left-1/2 z-50 -translate-x-1/2 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1 text-[var(--text-ui-xs)] whitespace-nowrap text-[var(--color-fg)] shadow-[var(--shadow-popover)]",
            side === "top" ? "bottom-full mb-1.5" : "top-full mt-1.5",
            className,
          )}
        >
          {label}
        </span>
      )}
    </span>
  );
}
