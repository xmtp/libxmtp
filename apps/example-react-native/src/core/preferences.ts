// Per-profile preferences (ExampleApp/Platform/ProfilePreferences.swift).
// Each name is one JSON file in the profile's `prefs/` directory. iOS
// encrypts them with AES-GCM; this app keeps plain JSON inside the protected,
// backup-excluded profile directory (plan decision D10).

import { bigintReplacer, decimalToBigint } from "./bigint.ts";
import { bytesToHex } from "./hex.ts";
import { type SendJournalStorage, parseSendDrafts } from "./sendJournal.ts";
import { sha256, utf8 } from "./sha256.ts";

/** Synchronous file access for one profile's `prefs/` directory. */
export interface PreferenceFiles {
  /** The file's text, or undefined when it does not exist. */
  read(name: string): string | undefined;
  /** Replaces the file atomically. */
  write(name: string, text: string): void;
  remove(name: string): void;
  /** The names of the existing files. */
  list(): readonly string[];
}

/** A preference that cannot be read (malformed JSON or shape). */
export class PreferenceError extends Error {
  constructor(name: string) {
    super(`The ${name} preference is unreadable.`);
    this.name = "PreferenceError";
  }
}

/** Names must be non-empty and contain no `/`; `.` and `..` are not names. */
export function isPreferenceName(name: string): boolean {
  return name !== "" && !name.includes("/") && name !== "." && name !== "..";
}

/** The composer draft file of a conversation: `composer-<sha256 hex of its ID>`. */
export function composerDraftName(conversationId: string): string {
  return "composer-" + bytesToHex(sha256(utf8(conversationId)));
}

/** A saved composer (`ComposerDraft` in iOS). */
export interface ComposerDraft {
  readonly text: string;
  /** Increments on each edit; an accepted send clears only an unedited draft. */
  readonly revision: number;
  readonly replyId?: string;
  /** The send reservation (the journal entry ID). */
  readonly requestId?: string;
  readonly submittedRevision?: number;
  readonly acceptedId?: string;
}

/** A saved reading position: the topmost visible message and its history position. */
export interface ScrollAnchor {
  readonly messageId: string;
  readonly sentAtNs: bigint;
  readonly cursor: string;
}

interface FeaturePreferences {
  readonly serverLabel?: string;
  /** Local conversation names ("Local name"), by conversation ID. */
  readonly aliases: Readonly<Record<string, string>>;
  /** Requests deleted on this device ("Delete request"). */
  readonly dismissedRequests: readonly string[];
  readonly skippedHandle: boolean;
  /** Local claim times (ms since the epoch), by handle. */
  readonly claimTimes: Readonly<Record<string, number>>;
}

const EMPTY_FEATURES: FeaturePreferences = {
  aliases: {},
  dismissedRequests: [],
  skippedHandle: false,
  claimTimes: {},
};

const FEATURES = "feature-preferences";
const READ_MARKERS = "read-markers";
const SEND_DRAFTS = "send-drafts";
const SCROLL_ANCHORS = "scroll-anchors";

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function stringRecord(
  value: unknown,
  name: string,
): Readonly<Record<string, string>> {
  if (value === undefined) return {};
  if (!isRecord(value)) throw new PreferenceError(name);
  const result: Record<string, string> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry !== "string") throw new PreferenceError(name);
    result[key] = entry;
  }
  return result;
}

const isStringArray = (value: unknown): value is readonly string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === "string");

function numberRecord(
  value: unknown,
  name: string,
): Readonly<Record<string, number>> {
  if (value === undefined) return {};
  if (!isRecord(value)) throw new PreferenceError(name);
  const result: Record<string, number> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry !== "number") throw new PreferenceError(name);
    result[key] = entry;
  }
  return result;
}

function parseFeatures(value: unknown): FeaturePreferences {
  if (!isRecord(value)) throw new PreferenceError(FEATURES);
  const serverLabel = value["serverLabel"];
  const dismissed = value["dismissedRequests"] ?? [];
  const skipped = value["skippedHandle"] ?? false;
  if (
    (serverLabel !== undefined && typeof serverLabel !== "string") ||
    !isStringArray(dismissed) ||
    typeof skipped !== "boolean"
  ) {
    throw new PreferenceError(FEATURES);
  }
  return {
    ...(serverLabel === undefined ? {} : { serverLabel }),
    aliases: stringRecord(value["aliases"], FEATURES),
    dismissedRequests: dismissed,
    skippedHandle: skipped,
    claimTimes: numberRecord(value["claimTimes"], FEATURES),
  };
}

function parseComposer(value: unknown, name: string): ComposerDraft {
  if (
    !isRecord(value) ||
    typeof value["text"] !== "string" ||
    typeof value["revision"] !== "number"
  ) {
    throw new PreferenceError(name);
  }
  const optional = (key: string, type: "string" | "number"): unknown => {
    const entry = value[key];
    if (entry !== undefined && typeof entry !== type) {
      throw new PreferenceError(name);
    }
    return entry;
  };
  const replyId = optional("replyId", "string");
  const requestId = optional("requestId", "string");
  const submittedRevision = optional("submittedRevision", "number");
  const acceptedId = optional("acceptedId", "string");
  return {
    text: value["text"],
    revision: value["revision"],
    ...(typeof replyId === "string" ? { replyId } : {}),
    ...(typeof requestId === "string" ? { requestId } : {}),
    ...(typeof submittedRevision === "number" ? { submittedRevision } : {}),
    ...(typeof acceptedId === "string" ? { acceptedId } : {}),
  };
}

/** One profile's preferences. Reads are cached; writes go to disk first. */
export class ProfilePreferences {
  readonly #files: PreferenceFiles;
  readonly #cache = new Map<string, unknown>();

  constructor(files: PreferenceFiles) {
    this.#files = files;
  }

  /** The parsed JSON of `name`, or undefined when it does not exist. */
  readJson(name: string): unknown {
    if (!isPreferenceName(name)) throw new PreferenceError(name);
    if (this.#cache.has(name)) return this.#cache.get(name);
    const text = this.#files.read(name);
    let value: unknown;
    if (text !== undefined) {
      try {
        value = JSON.parse(text);
      } catch {
        throw new PreferenceError(name);
      }
    }
    this.#cache.set(name, value);
    return value;
  }

  /** Writes `value` as JSON (bigints as decimal strings), then caches it. */
  writeJson(name: string, value: unknown): void {
    if (!isPreferenceName(name)) throw new PreferenceError(name);
    const text = JSON.stringify(value, bigintReplacer);
    this.#files.write(name, text);
    this.#cache.set(name, JSON.parse(text));
  }

  remove(name: string): void {
    if (!isPreferenceName(name)) throw new PreferenceError(name);
    this.#files.remove(name);
    this.#cache.set(name, undefined);
  }

  /** Names of stored preferences that start with `prefix`. */
  names(prefix: string): readonly string[] {
    return this.#files.list().filter((name) => name.startsWith(prefix));
  }

  #features(): FeaturePreferences {
    const value = this.readJson(FEATURES);
    return value === undefined ? EMPTY_FEATURES : parseFeatures(value);
  }

  #saveFeatures(
    change: (current: FeaturePreferences) => FeaturePreferences,
  ): void {
    this.writeJson(FEATURES, change(this.#features()));
  }

  /** The server name typed at Join, if any. */
  get serverLabel(): string | undefined {
    return this.#features().serverLabel;
  }

  setServerLabel(label: string): void {
    this.#saveFeatures((current) => ({ ...current, serverLabel: label }));
  }

  /** The local name of a conversation. */
  alias(conversationId: string): string | undefined {
    return this.#features().aliases[conversationId];
  }

  /** Sets or (with undefined or blank text) clears a local name. */
  setAlias(conversationId: string, alias: string | undefined): void {
    this.#saveFeatures((current) => {
      const { [conversationId]: _old, ...aliases } = current.aliases;
      return {
        ...current,
        aliases:
          alias === undefined || alias.trim() === ""
            ? aliases
            : { ...aliases, [conversationId]: alias },
      };
    });
  }

  /** Requests deleted on this device. */
  get dismissedRequests(): ReadonlySet<string> {
    return new Set(this.#features().dismissedRequests);
  }

  dismissRequest(conversationId: string): void {
    this.#saveFeatures((current) => ({
      ...current,
      dismissedRequests: [
        ...new Set([...current.dismissedRequests, conversationId]),
      ],
    }));
  }

  /** True after "Skip for now" on Choose handle. */
  get skippedHandle(): boolean {
    return this.#features().skippedHandle;
  }

  skipHandle(): void {
    this.#saveFeatures((current) => ({ ...current, skippedHandle: true }));
  }

  /** Local handle claim times. */
  claimTimes(): ReadonlyMap<string, Date> {
    return new Map(
      Object.entries(this.#features().claimTimes).map(([handle, time]) => [
        handle,
        new Date(time),
      ]),
    );
  }

  recordClaim(handle: string, at: Date): void {
    this.#saveFeatures((current) => ({
      ...current,
      claimTimes: { ...current.claimTimes, [handle]: at.getTime() },
    }));
  }

  /** The read marker (local insertion time, ns) of a logical key; 0 when unset. */
  marker(logicalKey: string): bigint {
    const markers = stringRecord(this.readJson(READ_MARKERS), READ_MARKERS);
    if (!Object.hasOwn(markers, logicalKey)) return 0n;
    const parsed = decimalToBigint(markers[logicalKey]);
    if (parsed === undefined) throw new PreferenceError(READ_MARKERS);
    return parsed;
  }

  setMarker(logicalKey: string, insertedAtNs: bigint): void {
    const markers = stringRecord(this.readJson(READ_MARKERS), READ_MARKERS);
    this.writeJson(READ_MARKERS, {
      ...markers,
      [logicalKey]: insertedAtNs.toString(),
    });
  }

  /** The `send-drafts` journal storage. */
  sendJournalStorage(): SendJournalStorage {
    return {
      read: () => {
        const value = this.readJson(SEND_DRAFTS);
        return value === undefined ? [] : parseSendDrafts(value);
      },
      write: (drafts) => {
        this.writeJson(SEND_DRAFTS, drafts);
      },
    };
  }

  /** The saved composer of a conversation. */
  composerDraft(conversationId: string): ComposerDraft | undefined {
    const name = composerDraftName(conversationId);
    const value = this.readJson(name);
    return value === undefined ? undefined : parseComposer(value, name);
  }

  saveComposerDraft(conversationId: string, draft: ComposerDraft): void {
    this.writeJson(composerDraftName(conversationId), draft);
  }

  removeComposerDraft(conversationId: string): void {
    this.remove(composerDraftName(conversationId));
  }

  /** The saved reading position of a conversation. */
  scrollAnchor(conversationId: string): ScrollAnchor | undefined {
    const anchors = this.readJson(SCROLL_ANCHORS);
    if (anchors === undefined) return undefined;
    if (!isRecord(anchors)) throw new PreferenceError(SCROLL_ANCHORS);
    const entry = anchors[conversationId];
    if (entry === undefined) return undefined;
    if (
      !isRecord(entry) ||
      typeof entry["messageId"] !== "string" ||
      typeof entry["sentAtNs"] !== "string" ||
      typeof entry["cursor"] !== "string"
    ) {
      throw new PreferenceError(SCROLL_ANCHORS);
    }
    const sentAtNs = decimalToBigint(entry["sentAtNs"]);
    if (sentAtNs === undefined) throw new PreferenceError(SCROLL_ANCHORS);
    return { messageId: entry["messageId"], sentAtNs, cursor: entry["cursor"] };
  }

  /** Saves (or with undefined removes) a reading position. */
  setScrollAnchor(
    conversationId: string,
    anchor: ScrollAnchor | undefined,
  ): void {
    const value = this.readJson(SCROLL_ANCHORS) ?? {};
    if (!isRecord(value)) throw new PreferenceError(SCROLL_ANCHORS);
    const { [conversationId]: _old, ...rest } = value;
    this.writeJson(
      SCROLL_ANCHORS,
      anchor === undefined ? rest : { ...rest, [conversationId]: anchor },
    );
  }
}
