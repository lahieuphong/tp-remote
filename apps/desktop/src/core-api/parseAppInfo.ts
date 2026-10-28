import type {
  AppInfo,
  BuildProfile,
  EngineModuleStatus,
  ModuleState,
  PlatformInfo,
  ProtocolSupport,
  RemoteFeatures,
  ShellInfo,
} from "../bindings";
import { CoreApiError } from "./errors";
import { ContractViolation, array, exact, nullableString, object, oneOf, string, u16 } from "./validate";

/**
 * Runtime validation of `get_app_info` responses against the generated contract.
 *
 * Each object is rebuilt field by field into a value typed with the generated binding,
 * so adding, removing or retyping a field in Rust breaks this file at compile time.
 * At runtime, wrong types, unknown enum values and unexpected fields are rejected.
 */

// Records over generated unions: exhaustive at compile time.
const BUILD_PROFILES: Record<BuildProfile, true> = { debug: true, release: true };
const REMOTE_FEATURES: Record<RemoteFeatures, true> = { notImplemented: true };
const MODULE_STATES: Record<ModuleState, true> = { foundation: true, partial: true };

function platformInfo(value: unknown, path: string): PlatformInfo {
  const o = object(value, path);
  return exact(
    o,
    { os: string(o, "os", path), family: string(o, "family", path), arch: string(o, "arch", path) },
    path,
  );
}

function protocolSupport(value: unknown, path: string): ProtocolSupport {
  const o = object(value, path);
  return exact(
    o,
    {
      current: u16(o, "current", path),
      minSupported: u16(o, "minSupported", path),
      maxSupported: u16(o, "maxSupported", path),
    },
    path,
  );
}

function shellInfo(value: unknown, path: string): ShellInfo {
  const o = object(value, path);
  return exact(
    o,
    {
      tauriVersion: string(o, "tauriVersion", path),
      webviewVersion: nullableString(o, "webviewVersion", path),
    },
    path,
  );
}

function engineModule(value: unknown, path: string): EngineModuleStatus {
  const o = object(value, path);
  return exact(
    o,
    {
      crateName: string(o, "crateName", path),
      role: string(o, "role", path),
      state: oneOf(o, "state", path, MODULE_STATES),
      ownerPhase: string(o, "ownerPhase", path),
    },
    path,
  );
}

export function parseAppInfo(raw: unknown): AppInfo {
  const path = "appInfo";
  try {
    const o = object(raw, path);
    const info: AppInfo = {
      appName: string(o, "appName", path),
      appVersion: string(o, "appVersion", path),
      phase: string(o, "phase", path),
      platform: platformInfo(o.platform, `${path}.platform`),
      buildProfile: oneOf(o, "buildProfile", path, BUILD_PROFILES),
      protocol: protocolSupport(o.protocol, `${path}.protocol`),
      shell: shellInfo(o.shell, `${path}.shell`),
      remoteFeatures: oneOf(o, "remoteFeatures", path, REMOTE_FEATURES),
      engineModules: array(o, "engineModules", path).map((m, i) =>
        engineModule(m, `${path}.engineModules[${String(i)}]`),
      ),
    };
    return exact(o, info, path);
  } catch (error) {
    if (error instanceof ContractViolation) {
      throw new CoreApiError(
        "invalidResponse",
        `The native core returned data that does not match the AppInfo contract: ${error.message}.`,
      );
    }
    throw error;
  }
}
