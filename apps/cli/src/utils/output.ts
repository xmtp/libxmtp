import { stdout } from "node:process";

import type { PublicIdentityKind } from "@xmtp/node-sdk";

export function isTTY(): boolean {
  // isTTY can be undefined when stdout is not a TTY (e.g., piped output)
  // oxlint-disable-next-line typescript/no-unnecessary-condition
  return stdout.isTTY ?? false;
}

/**
 * Characters that user input must not put raw in terminal output: C0, DEL,
 * and C1 controls, which can start escape sequences or fake a line, and the
 * Unicode bidirectional and invisible formatting characters (U+061C,
 * U+200B to U+200F, U+202A to U+202E, U+2060, U+2066 to U+2069, U+FEFF),
 * which can reorder or hide text in a bidi-aware terminal.
 */
const UNSAFE_CHARACTERS =
  // oxlint-disable-next-line no-control-regex
  /[\u0000-\u001f\u007f-\u009f\u061c\u200b-\u200f\u202a-\u202e\u2060\u2066-\u2069\ufeff]/g;

/**
 * Text with each unsafe character (see `UNSAFE_CHARACTERS`) replaced by a
 * visible `\uXXXX` escape. Use it for user input in terminal output.
 */
export function escapeControlCharacters(text: string): string {
  return text.replace(UNSAFE_CHARACTERS, (character) => {
    const code = character.charCodeAt(0).toString(16).padStart(4, "0");
    return `\\u${code}`;
  });
}

export function jsonStringify(data: unknown, pretty = false): string {
  return JSON.stringify(
    data,
    (_, value: unknown) =>
      typeof value === "bigint"
        ? value.toString()
        : value instanceof Map
          ? Object.fromEntries(value as Map<string, unknown>)
          : value,
    pretty ? 2 : undefined,
  );
}

export function formatPublicIdentityKind(kind: PublicIdentityKind): string {
  switch (kind) {
    case "ethereum":
      return "Ethereum";
    case "passkey":
      return "Passkey";
    case "handle":
      return "Handle";
    default: {
      kind satisfies never;
      return "";
    }
  }
}

export function formatTimestampNs(ns: bigint | string | undefined): string {
  if (ns === undefined) {
    return "Unknown";
  }
  const ms = Number(BigInt(ns) / BigInt(1_000_000));
  const date = new Date(ms);
  return date
    .toISOString()
    .replace("T", " ")
    .replace(/\.\d{3}Z$/, "");
}

export function formatHuman(data: unknown, indent = 0): string {
  const prefix = " ".repeat(indent);

  if (data === null || data === undefined) {
    return "";
  }

  if (typeof data === "string") {
    return prefix + data;
  }

  if (
    typeof data === "number" ||
    typeof data === "boolean" ||
    typeof data === "bigint"
  ) {
    return prefix + String(data);
  }

  if (Array.isArray(data)) {
    if (data.length === 0) {
      return prefix + "(empty)";
    }
    // Format as table if array of objects
    if (typeof data[0] === "object" && data[0] !== null) {
      return formatTable(data as Record<string, unknown>[], prefix);
    }
    return data.map((item) => formatHuman(item, indent)).join("\n");
  }

  if (typeof data === "object") {
    return formatObject(data as Record<string, unknown>, prefix);
  }

  return prefix + jsonStringify(data);
}

function stringifyValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  if (
    typeof value === "number" ||
    typeof value === "boolean" ||
    typeof value === "bigint"
  ) {
    return String(value);
  }
  return jsonStringify(value);
}

function formatTable(rows: Record<string, unknown>[], prefix = ""): string {
  if (rows.length === 0) {
    return "";
  }

  const keys = Object.keys(rows[0]);
  const widths = keys.map((key) =>
    Math.max(key.length, ...rows.map((row) => stringifyValue(row[key]).length)),
  );

  const header =
    prefix + keys.map((key, i) => key.padEnd(widths[i])).join("  ");
  const separator = prefix + widths.map((w) => "-".repeat(w)).join("  ");
  const body = rows
    .map(
      (row) =>
        prefix +
        keys
          .map((key, i) => stringifyValue(row[key]).padEnd(widths[i]))
          .join("  "),
    )
    .join("\n");

  return `${header}\n${separator}\n${body}`;
}

function formatObject(
  obj: Record<string, unknown>,
  prefix = "",
  keyWidth?: number,
): string {
  const entries = Object.entries(obj).filter(
    ([, v]) => v !== undefined && v !== null && v !== "",
  );
  if (entries.length === 0) {
    return "";
  }
  const maxKeyLen = keyWidth ?? Math.max(...entries.map(([k]) => k.length));
  return entries
    .map(
      ([key, value]) =>
        `${prefix}${key.padEnd(maxKeyLen)}  ${formatHuman(value)}`,
    )
    .join("\n");
}

export type Section = {
  title: string;
  data: Record<string, unknown>;
};

export function formatSections(sections: Section[], indent = 0): string {
  const prefix = " ".repeat(indent);

  // Collect all defined keys across all sections to compute a shared key width
  const allKeys = sections.flatMap((s) =>
    Object.entries(s.data)
      .filter(([, v]) => v !== undefined && v !== null && v !== "")
      .map(([k]) => k),
  );
  const keyWidth =
    allKeys.length > 0 ? Math.max(...allKeys.map((k) => k.length)) : 0;

  return sections
    .map((s) => `${s.title}\n\n${formatObject(s.data, prefix, keyWidth)}`)
    .join("\n\n");
}
