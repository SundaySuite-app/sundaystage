// Unit smoke — the class-name helper. Confirms clsx conditionals and
// tailwind-merge conflict resolution both flow through `cn`.
import { describe, it, expect } from "vitest";

import { cn } from "@/lib/cn";

describe("cn", () => {
  it("joins truthy classes and drops falsy ones", () => {
    const hidden = false;
    expect(cn("p-2", hidden && "hidden", "text-sm")).toBe("p-2 text-sm");
  });

  it("lets later Tailwind classes win conflicts", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
  });
});
