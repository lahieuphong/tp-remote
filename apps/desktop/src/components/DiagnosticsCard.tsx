import type { AppInfoState } from "../app/useAppInfo";
import type { AppInfo } from "../bindings";
import { Card } from "./Card";

function rows(info: AppInfo): [string, string][] {
  const { protocol, platform, shell } = info;
  return [
    ["App version", info.appVersion],
    ["Project phase", info.phase],
    ["Operating system", `${platform.os} (${platform.family})`],
    ["CPU architecture", platform.arch],
    ["Build profile", info.buildProfile],
    [
      "Protocol version",
      `v${String(protocol.current)} (supports v${String(protocol.minSupported)}–v${String(protocol.maxSupported)})`,
    ],
    ["Tauri", shell.tauriVersion],
    ["WebView runtime", shell.webviewVersion ?? "unavailable"],
  ];
}

export function DiagnosticsCard({ state, className }: { state: AppInfoState; className: string }) {
  return (
    <Card title="Diagnostics (from Rust)" labelledBy="diagnostics-title" className={className}>
      {state.status === "ready" ? (
        <dl className="grid grid-cols-1 gap-x-8 gap-y-2 text-sm sm:grid-cols-2" data-testid="diagnostics">
          {rows(state.info).map(([label, value]) => (
            <div key={label} className="flex justify-between gap-4 border-b border-neutral-800/70 py-1.5">
              <dt className="text-neutral-400">{label}</dt>
              <dd className="text-right font-mono text-neutral-100">{value}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="text-sm text-neutral-500">
          {state.status === "loading"
            ? "Diagnostics appear when the native core responds."
            : "No diagnostics: the native core did not return data."}
        </p>
      )}
    </Card>
  );
}
