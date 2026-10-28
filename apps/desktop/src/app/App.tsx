import { AppMark } from "../components/AppMark";
import { CoreStatusCard } from "../components/CoreStatusCard";
import { DiagnosticsCard } from "../components/DiagnosticsCard";
import { EngineModulesCard } from "../components/EngineModulesCard";
import { LocalPreviewCard } from "../components/LocalPreviewCard";
import { RemoteFeaturesCard } from "../components/RemoteFeaturesCard";
import { useAppInfo } from "./useAppInfo";

export function App() {
  const { state, reload } = useAppInfo();
  const info = state.status === "ready" ? state.info : null;

  return (
    <div className="flex min-h-full flex-col">
      <header className="border-b border-neutral-800 bg-neutral-950/80">
        <div className="mx-auto flex w-full max-w-5xl items-center gap-3 px-6 py-4">
          <AppMark className="size-9 shrink-0" />
          <div className="min-w-0">
            <h1 className="text-lg leading-tight font-semibold text-neutral-50">TP Remote</h1>
            <p className="truncate text-xs text-neutral-400">Remote desktop · local capture preview build</p>
          </div>
          <div className="ml-auto flex items-center gap-2 text-xs">
            <span className="rounded-md border border-teal-800 px-2 py-1 font-mono text-teal-300">
              {info ? info.phase : "phase …"}
            </span>
            <span className="rounded-md border border-neutral-800 px-2 py-1 font-mono text-neutral-300">
              {info ? `v${info.appVersion}` : "v…"}
            </span>
          </div>
        </div>
      </header>

      <main className="mx-auto grid w-full max-w-5xl flex-1 content-start gap-5 px-6 py-6 md:grid-cols-2">
        <CoreStatusCard state={state} onRetry={reload} />
        <RemoteFeaturesCard state={state} />
        <LocalPreviewCard className="md:col-span-2" />
        <DiagnosticsCard state={state} className="md:col-span-2" />
        <EngineModulesCard state={state} className="md:col-span-2" />
      </main>

      <footer className="border-t border-neutral-800 px-6 py-3 text-center text-xs text-neutral-500">
        Internal development build · All rights reserved
      </footer>
    </div>
  );
}
