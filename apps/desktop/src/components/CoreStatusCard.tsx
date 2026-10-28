import type { AppInfoState } from "../app/useAppInfo";
import { Card } from "./Card";

const LABEL = { loading: "Loading", ready: "Ready", failed: "Failed" } as const;

const DOT = {
  loading: "bg-neutral-400 animate-pulse",
  ready: "bg-emerald-400",
  failed: "bg-red-400",
} as const;

export function CoreStatusCard({
  state,
  onRetry,
}: {
  state: AppInfoState;
  onRetry: () => void;
}) {
  return (
    <Card title="Native core" labelledBy="core-status-title">
      <div role="status" aria-live="polite" data-status={state.status}>
        <p className="flex items-center gap-2 text-lg font-semibold text-neutral-100">
          <span className={`inline-block size-2.5 rounded-full ${DOT[state.status]}`} />
          <span data-testid="core-status-label">{LABEL[state.status]}</span>
        </p>
        {state.status === "loading" && (
          <p className="mt-2 text-sm text-neutral-400">Waiting for the Rust core to answer get_app_info…</p>
        )}
        {state.status === "ready" && (
          <p className="mt-2 text-sm text-neutral-400">
            The Rust core answered get_app_info over Tauri IPC.
          </p>
        )}
        {state.status === "failed" && (
          <div className="mt-2 space-y-3">
            <p className="text-sm text-red-300" data-testid="core-error-message">
              {state.error.message}
            </p>
            <p className="font-mono text-xs text-neutral-500">code: {state.error.code}</p>
            <button
              type="button"
              onClick={onRetry}
              className="rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 focus-visible:ring-2 focus-visible:ring-teal-400 focus-visible:outline-none"
            >
              Retry
            </button>
          </div>
        )}
      </div>
    </Card>
  );
}
