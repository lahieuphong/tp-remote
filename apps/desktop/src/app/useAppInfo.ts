import { useCallback, useEffect, useState } from "react";

import type { AppInfo } from "../bindings";
import { getAppInfo } from "../core-api/appInfo";
import { type CoreApiError, toCoreApiError } from "../core-api/errors";

export type AppInfoState =
  | { status: "loading" }
  | { status: "ready"; info: AppInfo }
  | { status: "failed"; error: CoreApiError };

/** Loads AppInfo from the native core. "ready" is only reached when the call succeeds. */
export function useAppInfo(): { state: AppInfoState; reload: () => void } {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<AppInfoState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    getAppInfo().then(
      (info) => {
        if (!cancelled) setState({ status: "ready", info });
      },
      (error: unknown) => {
        if (!cancelled) setState({ status: "failed", error: toCoreApiError(error) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [attempt]);

  const reload = useCallback(() => {
    setState({ status: "loading" });
    setAttempt((n) => n + 1);
  }, []);

  return { state, reload };
}
