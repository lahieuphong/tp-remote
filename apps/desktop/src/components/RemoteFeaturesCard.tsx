import type { AppInfoState } from "../app/useAppInfo";
import { Card } from "./Card";

export function RemoteFeaturesCard({ state }: { state: AppInfoState }) {
  return (
    <Card title="Remote features" labelledBy="remote-features-title">
      <p className="text-lg font-semibold text-amber-300">Remote features not implemented</p>
      <p className="mt-2 text-sm leading-relaxed text-neutral-400">
        This build cannot share a screen with another computer, accept connections or control
        another computer. It opens no network listener and requests no accessibility permission.
        The local screen preview below stays on this computer.
      </p>
      {state.status === "ready" && (
        <p className="mt-3 font-mono text-xs text-neutral-500">
          reported by Rust core: remoteFeatures = {state.info.remoteFeatures}
        </p>
      )}
    </Card>
  );
}
