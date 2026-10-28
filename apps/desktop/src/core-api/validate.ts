/**
 * Runtime validation helpers for IPC responses. Parsers rebuild each object field by
 * field into a value typed with the generated binding, so a Rust change breaks them at
 * compile time; at runtime, wrong types, unknown enum values and extra fields throw.
 */

export type JsonObject = Record<string, unknown>;

export class ContractViolation extends Error {}

export function object(value: unknown, path: string): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ContractViolation(`${path} must be an object`);
  }
  return value as JsonObject;
}

export function string(source: JsonObject, key: string, path: string): string {
  const value = source[key];
  if (typeof value !== "string") {
    throw new ContractViolation(`${path}.${key} must be a string`);
  }
  return value;
}

export function nullableString(source: JsonObject, key: string, path: string): string | null {
  const value = source[key];
  if (value === null) {
    return null;
  }
  if (typeof value !== "string") {
    throw new ContractViolation(`${path}.${key} must be a string or null`);
  }
  return value;
}

function integer(source: JsonObject, key: string, path: string, min: number, max: number): number {
  const value = source[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw new ContractViolation(`${path}.${key} must be an integer between ${String(min)} and ${String(max)}`);
  }
  return value;
}

export function u16(source: JsonObject, key: string, path: string): number {
  return integer(source, key, path, 0, 0xffff);
}

export function u32(source: JsonObject, key: string, path: string): number {
  return integer(source, key, path, 0, 0xffff_ffff);
}

export function i32(source: JsonObject, key: string, path: string): number {
  return integer(source, key, path, -0x8000_0000, 0x7fff_ffff);
}

export function nullableU32(source: JsonObject, key: string, path: string): number | null {
  return source[key] === null ? null : u32(source, key, path);
}

/** A finite, non-negative number (rates). */
export function rate(source: JsonObject, key: string, path: string): number {
  const value = source[key];
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new ContractViolation(`${path}.${key} must be a finite non-negative number`);
  }
  return value;
}

export function boolean(source: JsonObject, key: string, path: string): boolean {
  const value = source[key];
  if (typeof value !== "boolean") {
    throw new ContractViolation(`${path}.${key} must be a boolean`);
  }
  return value;
}

export function oneOf<T extends string>(
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

export function nullableOneOf<T extends string>(
  source: JsonObject,
  key: string,
  path: string,
  allowed: Record<T, true>,
): T | null {
  return source[key] === null ? null : oneOf(source, key, path, allowed);
}

export function array(source: JsonObject, key: string, path: string): unknown[] {
  const value = source[key];
  if (!Array.isArray(value)) {
    throw new ContractViolation(`${path}.${key} must be an array`);
  }
  return value;
}

/** Mirrors Rust's `deny_unknown_fields`: the source may not carry extra keys. */
export function exact<T extends object>(source: JsonObject, parsed: T, path: string): T {
  for (const key of Object.keys(source)) {
    if (!Object.hasOwn(parsed, key)) {
      throw new ContractViolation(`${path}.${key} is not part of the contract`);
    }
  }
  return parsed;
}
