import type { AppInfoState } from "../app/useAppInfo";
import type { ModuleState } from "../bindings";
import { Card } from "./Card";

const STATE_STYLE: Record<ModuleState, string> = {
  foundation: "border-neutral-700 text-neutral-400",
  partial: "border-amber-700/70 text-amber-300",
};

export function EngineModulesCard({ state, className }: { state: AppInfoState; className: string }) {
  return (
    <Card title="Engine modules" labelledBy="modules-title" className={className}>
      {state.status === "ready" ? (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm" data-testid="engine-modules">
            <thead className="text-xs text-neutral-500">
              <tr>
                <th className="py-1.5 pr-4 font-medium">Crate</th>
                <th className="py-1.5 pr-4 font-medium">Role</th>
                <th className="py-1.5 pr-4 font-medium">State</th>
                <th className="py-1.5 font-medium">Next phase</th>
              </tr>
            </thead>
            <tbody>
              {state.info.engineModules.map((m) => (
                <tr key={m.crateName} className="border-t border-neutral-800/70">
                  <td className="py-1.5 pr-4 font-mono text-neutral-100">{m.crateName}</td>
                  <td className="py-1.5 pr-4 text-neutral-400">{m.role}</td>
                  <td className="py-1.5 pr-4">
                    <span className={`rounded border px-1.5 py-0.5 text-xs ${STATE_STYLE[m.state]}`}>
                      {m.state}
                    </span>
                  </td>
                  <td className="py-1.5 font-mono text-neutral-400">{m.ownerPhase}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="text-sm text-neutral-500">Module status appears when the native core responds.</p>
      )}
    </Card>
  );
}
