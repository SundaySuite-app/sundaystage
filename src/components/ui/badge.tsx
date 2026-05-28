import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "@/lib/cn";

const badge = cva(
  "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[var(--text-ui-xs)] font-medium",
  {
    variants: {
      variant: {
        neutral:
          "border-[var(--color-border)] bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]",
        accent:
          "border-transparent bg-[var(--color-accent)] text-[var(--color-accent-fg)]",
        success:
          "border-transparent bg-[var(--color-success)]/15 text-[var(--color-success)]",
        warning:
          "border-transparent bg-[var(--color-warning)]/15 text-[var(--color-warning)]",
        danger:
          "border-transparent bg-[var(--color-danger)]/15 text-[var(--color-danger)]",
      },
    },
    defaultVariants: { variant: "neutral" },
  },
);

export type BadgeProps = ComponentProps<"span"> & VariantProps<typeof badge>;

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badge({ variant }), className)} {...props} />;
}
