// The global session index (ExampleCore/Session/SessionStorage.swift). It is
// a JSON file with the iOS key names.

import { parseBackendUrl } from "./backendUrl.ts";
import { type ResetJournal, isResetPhase } from "./resetJournal.ts";

export type Appearance = "system" | "light" | "dark";

/** One server's identity on this device. */
export interface BackendProfile {
  readonly id: string;
  /** The URL as first saved (normalized when the profile was created). */
  readonly url: string;
  readonly inboxId?: string;
  /** The Ethereum address of the profile's signer. */
  readonly identity?: string;
}

export interface SessionIndex {
  readonly profiles: readonly BackendProfile[];
  readonly activeId?: string;
  readonly signedIn: boolean;
  readonly reset?: ResetJournal;
  readonly appearance: Appearance;
}

export const EMPTY_SESSION_INDEX: SessionIndex = {
  profiles: [],
  signedIn: false,
  appearance: "system",
};

/** A stored index that is not valid JSON of the expected shape. */
export class CorruptIndexError extends Error {
  constructor(detail: string) {
    super(`The session index is unreadable: ${detail}`);
    this.name = "CorruptIndexError";
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function optionalString(
  record: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = record[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string")
    throw new CorruptIndexError(`${key} is not text`);
  return value;
}

function parseProfile(value: unknown): BackendProfile {
  if (!isRecord(value))
    throw new CorruptIndexError("a profile is not an object");
  const id = optionalString(value, "id");
  const url = optionalString(value, "url");
  if (id === undefined || url === undefined) {
    throw new CorruptIndexError("a profile has no id or url");
  }
  const inboxId = optionalString(value, "inboxID");
  const identity = optionalString(value, "identity");
  return {
    id,
    url,
    ...(inboxId === undefined ? {} : { inboxId }),
    ...(identity === undefined ? {} : { identity }),
  };
}

function parseAppearance(value: unknown): Appearance {
  return value === "light" || value === "dark" ? value : "system";
}

/** Reads the index JSON. Throws `CorruptIndexError` for a malformed file. */
export function parseSessionIndex(text: string): SessionIndex {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch {
    throw new CorruptIndexError("not JSON");
  }
  if (!isRecord(json)) throw new CorruptIndexError("not an object");
  const profiles = json["profiles"] ?? [];
  if (!Array.isArray(profiles))
    throw new CorruptIndexError("profiles is not a list");
  const signedIn = json["signedIn"] ?? false;
  if (typeof signedIn !== "boolean")
    throw new CorruptIndexError("signedIn is not a boolean");
  const activeId = optionalString(json, "activeID");
  let reset: ResetJournal | undefined;
  const rawReset = json["reset"];
  if (rawReset !== undefined && rawReset !== null) {
    if (!isRecord(rawReset))
      throw new CorruptIndexError("reset is not an object");
    const profileId = optionalString(rawReset, "profileID");
    const phase = rawReset["phase"];
    if (profileId === undefined || !isResetPhase(phase)) {
      throw new CorruptIndexError("reset has no profile or phase");
    }
    reset = { profileId, phase };
  }
  return {
    profiles: profiles.map(parseProfile),
    signedIn,
    appearance: parseAppearance(json["appearance"]),
    ...(activeId === undefined ? {} : { activeId }),
    ...(reset === undefined ? {} : { reset }),
  };
}

/** The index JSON, with the iOS key names. */
export function serializeSessionIndex(index: SessionIndex): string {
  return JSON.stringify({
    profiles: index.profiles.map((profile) => ({
      id: profile.id,
      url: profile.url,
      inboxID: profile.inboxId,
      identity: profile.identity,
    })),
    activeID: index.activeId,
    signedIn: index.signedIn,
    reset:
      index.reset === undefined
        ? undefined
        : { profileID: index.reset.profileId, phase: index.reset.phase },
    appearance: index.appearance,
  });
}

/**
 * The profile for a backend URL. Stored URLs are normalized again, so a
 * legacy, unnormalized URL still matches.
 */
export function profileForUrl(
  index: SessionIndex,
  normalizedUrl: string,
): BackendProfile | undefined {
  return index.profiles.find(
    (profile) => parseBackendUrl(profile.url)?.value === normalizedUrl,
  );
}

/** The signed-in (or last used) profile. */
export function activeProfile(index: SessionIndex): BackendProfile | undefined {
  return index.profiles.find((profile) => profile.id === index.activeId);
}

/** A profile is established once it has an inbox ID or an identity. */
export function isEstablished(profile: BackendProfile): boolean {
  return profile.inboxId !== undefined || profile.identity !== undefined;
}

/** The index with `profile` replacing the entry that has its ID (or appended). */
export function withProfile(
  index: SessionIndex,
  profile: BackendProfile,
): SessionIndex {
  return {
    ...index,
    profiles: [
      ...index.profiles.filter((entry) => entry.id !== profile.id),
      profile,
    ],
  };
}
