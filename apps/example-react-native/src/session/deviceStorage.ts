// Device implementations of the session ports: secrets in the Keychain
// (Keystore on Android), the index and profile directories under
// Application Support (internal storage on Android).

import {
  type ProfilePaths,
  databaseFiles,
  isInside,
  joinPath,
  profilePaths,
} from "@/core/paths.ts";
import type { PreferenceFiles } from "@/core/preferences.ts";
import type {
  ProfileFiles,
  SecretRef,
  SecretStore,
} from "@/core/sessionController.ts";
import { SessionCopy, SessionError } from "@/core/sessionError.ts";
import {
  EMPTY_SESSION_INDEX,
  type SessionIndex,
  parseSessionIndex,
  serializeSessionIndex,
} from "@/core/sessionIndex.ts";
import {
  directoryNames,
  ensureDirectory,
  ensureProtectedDirectory,
  pathKind,
  readText,
  removePath,
  writeTextAtomic,
} from "@/platform/files";
import { applicationSupportDirectory } from "@/platform/messengerPlatform";
import { randomBytes } from "@/platform/random";
import { secureDelete, secureGet, secureSet } from "@/platform/secureStore";

const SECRET_REFS: readonly SecretRef[] = [
  "wallet",
  "database-key",
  "preferences-key",
  "credential",
];

/** A promise for synchronous work, rejecting with what it throws. */
function settle<T>(work: () => T): Promise<T> {
  try {
    return Promise.resolve(work());
  } catch (error) {
    return Promise.reject(
      error instanceof Error ? error : new Error(String(error)),
    );
  }
}

/** Secrets keyed `<profile UUID>.<ref>` (secure-store keys cannot contain `:`). */
export class DeviceSecretStore implements SecretStore {
  read(profileId: string, ref: SecretRef): Promise<string | undefined> {
    return secureGet(`${profileId}.${ref}`);
  }

  write(profileId: string, ref: SecretRef, value: string): Promise<void> {
    return secureSet(`${profileId}.${ref}`, value);
  }

  remove(profileId: string, ref: SecretRef): Promise<void> {
    return secureDelete(`${profileId}.${ref}`);
  }

  async removeAll(profileId: string): Promise<void> {
    for (const ref of SECRET_REFS) await this.remove(profileId, ref);
  }

  randomBytes(count: number): Uint8Array {
    return randomBytes(count);
  }
}

/** The messenger root: `<Application Support>/messenger`. */
export function messengerRoot(): string {
  return joinPath(applicationSupportDirectory(), "messenger");
}

/** The index file and the profile directories (`ProfileFiles.swift`). */
export class DeviceProfileFiles implements ProfileFiles {
  readonly #root: () => string;

  constructor(root: () => string = messengerRoot) {
    this.#root = root;
  }

  #indexPath(): string {
    return joinPath(this.#root(), "session-index.json");
  }

  #profile(profileId: string): ProfilePaths {
    const paths = profilePaths(this.#root(), profileId);
    if (!isInside(paths.root, joinPath(this.#root(), "profiles"))) {
      throw SessionError.message(SessionCopy.unsafePath);
    }
    return paths;
  }

  prepare(): Promise<void> {
    return settle(() => {
      ensureProtectedDirectory(this.#root());
    });
  }

  loadIndex(): Promise<SessionIndex> {
    return settle(() => {
      const text = readText(this.#indexPath());
      return text === undefined ? EMPTY_SESSION_INDEX : parseSessionIndex(text);
    });
  }

  saveIndex(index: SessionIndex): Promise<void> {
    return settle(() => {
      writeTextAtomic(this.#indexPath(), serializeSessionIndex(index));
    });
  }

  paths(profileId: string): Promise<ProfilePaths> {
    return settle(() => {
      const paths = this.#profile(profileId);
      for (const directory of [
        paths.root,
        paths.attachments,
        paths.temp,
        paths.preferences,
      ]) {
        ensureProtectedDirectory(directory);
      }
      return paths;
    });
  }

  databaseExists(profileId: string): Promise<boolean> {
    return settle(() => {
      const paths = this.#profile(profileId);
      return (
        pathKind(paths.root) === "directory" &&
        pathKind(joinPath(paths.root, "sdk")) === "directory" &&
        pathKind(paths.database) === "file"
      );
    });
  }

  removeDatabase(profileId: string): Promise<void> {
    return settle(() => {
      for (const file of databaseFiles(this.#profile(profileId).database))
        removePath(file);
    });
  }

  removeExports(profileId: string): Promise<void> {
    return settle(() => {
      removePath(this.#profile(profileId).exports);
    });
  }

  removeProfile(profileId: string): Promise<void> {
    return settle(() => {
      removePath(this.#profile(profileId).root);
    });
  }
}

/** One profile's `prefs/` directory: each preference is `<name>.json`. */
export class DevicePreferenceFiles implements PreferenceFiles {
  readonly #directory: string;

  constructor(directory: string) {
    this.#directory = directory;
  }

  read(name: string): string | undefined {
    return readText(joinPath(this.#directory, `${name}.json`));
  }

  write(name: string, text: string): void {
    ensureDirectory(this.#directory);
    writeTextAtomic(joinPath(this.#directory, `${name}.json`), text);
  }

  remove(name: string): void {
    removePath(joinPath(this.#directory, `${name}.json`));
  }

  list(): readonly string[] {
    return directoryNames(this.#directory)
      .filter((name) => name.endsWith(".json"))
      .map((name) => name.slice(0, -".json".length));
  }
}
