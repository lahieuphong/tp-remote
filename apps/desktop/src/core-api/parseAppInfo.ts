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

/**
 * Runtime validation of `get_app_info` responses against the generated contract.
 *
 * Each object is rebuilt field by field into a value typed with the generated binding,
 * so adding, removing or retyping a field in Rust breaks this file at compile time.
 * At runtime, wrong types, unknown enum values and unexpected fields are rejected.
 */

type JsonObject = Record<string, unknown>;

class ContractViolation extends Error {}

function object(value: unknown, path: string): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ContractViolation(`${path} must be an object`);
  }
  return value as JsonObject;
}

function string(source: JsonObject, key: string, path: string): string {
  const value = source[key];
  if (typeof value !== "string") {
    throw new ContractViolation(`${path}.${key} must be a string`);
  }
  return value;
}

function nullableString(source: JsonObject, key: string, path: string): string | null {
  const value = source[key];
  if (value === null) {
    return null;
  }
  if (typeof value !== "string") {
    throw new ContractViolation(`${path}.${key} must be a string or null`);
  }
  return value;
}

function u16(source: JsonObject, key: string, path: string): number {
  const value = source[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new ContractViolation(`${path}.${key} must be an integer between 0 and 65535`);
  }
  return value;
}

function oneOf<T extends string>(
  source: JsonObject,
  key: string,
  path: string,
  allowed: Record<T, true>,
): T {
  const value = source[key];
  if (typeof value !== "string" || !Object.hasOwn(allowed, value)) {
    throw new ContractViolation(`${path}.${key} has an unsupported value`);
  }
  return value as T;
}

function array(source: JsonObject, key: string, path: string): unknown[] {
  const value = source[key];
  if (!Array.isArray(value)) {
    throw new ContractViolation(`${path}.${key} must be an array`);
  }
  return value;
}

/** Mirrors Rust's `deny_unknown_fields`: the source may not carry extra keys. */
function exact<T extends object>(source: JsonObject, parsed: T, path: string): T {
  for (const key of Object.keys(source)) {
    if (!Object.hasOwn(parsed, key)) {
      throw new ContractViolation(`${path}.${key} is not part of the contract`);
    }
  }
  return parsed;
}

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
