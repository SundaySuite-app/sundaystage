import { useCallback, useEffect, useRef, useState } from "react";

/**
 * useErrorToast — owns a single error message string plus an auto-dismiss timer.
 *
 * Pairs with the `<ErrorToast>` primitive to surface failures that would
 * otherwise be swallowed (e.g. a failed disk write on a settings save). Live
 * output is sacrosanct, so this never throws or blocks — it just makes a silent
 * failure visible.
 *
 * `showError(msg)` displays the toast for `timeoutMs` (default 6s) then clears
 * it; `dismiss()` clears it immediately. The timer is cleaned up on unmount so a
 * late save failure can't fire `setState` on a torn-down panel.
 */
export function useErrorToast(timeoutMs = 6000) {
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const dismiss = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    setMessage(null);
  }, []);

  const showError = useCallback(
    (msg: string) => {
      if (timer.current) clearTimeout(timer.current);
      setMessage(msg);
      timer.current = setTimeout(() => setMessage(null), timeoutMs);
    },
    [timeoutMs],
  );

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  return { message, showError, dismiss };
}
