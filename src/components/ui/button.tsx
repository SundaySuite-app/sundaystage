import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "@/lib/cn";

const button = cva(
  "inline-flex items-center justify-center gap-2 rounded-md font-medium whitespace-nowrap transition-all disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent)]",
  {
    variants: {
      variant: {
        primary:
          "bg-[var(--color-accent)] text-[var(--color-accent-fg)] shadow-sm hover:brightness-110 active:translate-y-px",
        secondary:
          "bg-[var(--color-bg-surface)] text-[var(--color-fg)] hover:brightness-110",
        outline:
          "border border-[var(--color-border)] bg-transparent text-[var(--color-fg)] hover:bg-[var(--color-bg-surface)]/60",
        ghost:
          "bg-transparent text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]/60 hover:text-[var(--color-fg)]",
        danger:
          "bg-[var(--color-danger)] text-white shadow-sm hover:brightness-110 active:translate-y-px",
      },
      size: {
        sm: "h-8 px-2.5 text-[var(--text-ui-sm)]",
        md: "h-9 px-3.5 text-[var(--text-ui-sm)]",
        lg: "h-11 px-5 text-[var(--text-ui-md)]",
        icon: "h-9 w-9 p-0",
      },
    },
    defaultVariants: { variant: "primary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof button>;

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return (
    <button className={cn(button({ variant, size }), className)} {...props} />
  );
}
