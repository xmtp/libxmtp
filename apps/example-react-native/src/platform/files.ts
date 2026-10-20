import { Directory, File, Paths } from "expo-file-system";

import { pathToUri } from "@/core/paths.ts";
import {
  excludeFromBackup,
  protectUntilFirstUserAuthentication,
} from "@/platform/messengerPlatform";
import { randomUuid } from "@/platform/random";

// Synchronous file access by native path (the SDK takes native paths; Expo's
// file classes take file:// URIs).

/** What is at `path`: nothing, a file or a directory. */
export function pathKind(path: string): "none" | "file" | "directory" {
  const info = Paths.info(pathToUri(path));
  if (!info.exists) return "none";
  return info.isDirectory === true ? "directory" : "file";
}

/** Creates `path` and its parents; an existing directory is fine. */
export function ensureDirectory(path: string): void {
  new Directory(pathToUri(path)).create({
    intermediates: true,
    idempotent: true,
  });
}

/**
 * Creates a directory for private app data: iOS file protection until first
 * unlock and no device or iCloud backup. Android needs neither (internal
 * storage, `allowBackup="false"`).
 */
export function ensureProtectedDirectory(path: string): void {
  ensureDirectory(path);
  protectUntilFirstUserAuthentication(path);
  excludeFromBackup(path);
}

/** The file's text, or undefined when there is no file. */
export function readText(path: string): string | undefined {
  const file = new File(pathToUri(path));
  return file.exists ? file.textSync() : undefined;
}

/** Writes `text` to a temporary sibling, then moves it over `path`. */
export function writeTextAtomic(path: string, text: string): void {
  const temporary = new File(pathToUri(`${path}.${randomUuid()}.tmp`));
  temporary.writeSync(text);
  try {
    temporary.moveSync(new File(pathToUri(path)), { overwrite: true });
  } catch (error) {
    if (temporary.exists) temporary.delete();
    throw error;
  }
}

/**
 * Removes a file or a whole directory. A missing path is not an error, also
 * when it disappears during the call (SQLite removes `-wal` and `-shm` when
 * its last connection closes).
 */
export function removePath(path: string): void {
  try {
    switch (pathKind(path)) {
      case "none":
        return;
      case "directory":
        new Directory(pathToUri(path)).delete();
        return;
      case "file":
        new File(pathToUri(path)).delete();
    }
  } catch (error) {
    if (pathKind(path) !== "none") throw error;
  }
}

/** The names in a directory, or none when it does not exist. */
export function directoryNames(path: string): readonly string[] {
  if (pathKind(path) !== "directory") return [];
  return new Directory(pathToUri(path)).list().map((entry) => entry.name);
}
