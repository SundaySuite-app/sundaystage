/**
 * Class-name helper — combines `clsx` for conditional logic with
 * `tailwind-merge` to dedupe conflicting Tailwind classes.
 *
 * Usage: `cn("p-2", isActive && "bg-accent", "p-4")` → `"bg-accent p-4"`
 */

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
