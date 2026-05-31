/**
 * FreeShow-style media drawer that slides up from the bottom of the console.
 * Collapsed by default; expands to the existing media browser so the operator
 * can manage backgrounds, loops and images without leaving the workspace.
 */
import { ChevronDown, Image as ImageIcon } from "lucide-react";

import type { Library } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { MediaPage } from "@/features/media/MediaPage";

interface Props {
  library: Library;
  open: boolean;
  onToggle: () => void;
}

export function MediaDrawer({ library, open, onToggle }: Props) {
  const t = useT();
  return (
    <section
      className={cn(
        "flex min-h-0 flex-col border-t border-[var(--color-border)] bg-[var(--color-bg-elevated)] transition-[height] duration-200",
        open ? "h-[42vh]" : "h-9",
      )}
    >
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full shrink-0 items-center gap-2 px-3 py-2 text-left text-xs font-semibold tracking-wide text-[var(--color-fg-muted)] uppercase hover:text-[var(--color-fg)]"
      >
        <ImageIcon size={14} aria-hidden />
        {t("wsMediaDrawer")}
        <ChevronDown
          size={14}
          aria-hidden
          className={cn(
            "ml-auto transition-transform",
            open ? "" : "rotate-180",
          )}
        />
      </button>
      {open && (
        <div className="min-h-0 flex-1 overflow-hidden">
          <MediaPage library={library} />
        </div>
      )}
    </section>
  );
}
