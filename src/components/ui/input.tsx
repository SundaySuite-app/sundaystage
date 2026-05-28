import type { ComponentProps } from "react";

import { cn } from "@/lib/cn";

export function Input({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "h-9 w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 text-[var(--text-ui-sm)] text-[var(--color-fg)] placeholder:text-[var(--color-fg-muted)] transition-colors",
        "focus-visible:border-[var(--color-accent)] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
