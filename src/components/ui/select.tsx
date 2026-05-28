import type { ComponentProps } from "react";
import { ChevronDown } from "lucide-react";

import { cn } from "@/lib/cn";

// Native <select> styled to the design tokens — accessible by default and
// enough for most cases. A rich Combobox arrives when a feature needs search.
export function Select({
  className,
  children,
  ...props
}: ComponentProps<"select">) {
  return (
    <div className="relative inline-flex w-full items-center">
      <select
        className={cn(
          "h-9 w-full appearance-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] pr-8 pl-3 text-[var(--text-ui-sm)] text-[var(--color-fg)] transition-colors",
          "focus-visible:border-[var(--color-accent)] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        size={14}
        aria-hidden
        className="pointer-events-none absolute right-2.5 text-[var(--color-fg-muted)]"
      />
    </div>
  );
}
