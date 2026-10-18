// Native paths and file:// URIs. The SDK takes native paths; Expo file APIs
// and pickers use file:// URIs.

/** The native path of a `file://` URI (percent-decoded). Other text is returned as is. */
export function uriToPath(uri: string): string {
  if (!uri.startsWith("file://")) return uri;
  const rest = uri.slice("file://".length);
  // file://localhost/path is the same file as file:///path.
  const path = rest.startsWith("localhost/")
    ? rest.slice("localhost".length)
    : rest;
  return decodeURIComponent(path);
}

/** The `file://` URI of an absolute native path, with each segment percent-encoded. */
export function pathToUri(path: string): string {
  if (path.startsWith("file://")) return path;
  return "file://" + path.split("/").map(encodeURIComponent).join("/");
}

/** Joins path segments with single `/` separators. */
export function joinPath(...parts: readonly string[]): string {
  return parts
    .filter((part) => part.length > 0)
    .map((part, index) =>
      index === 0 ? part.replace(/\/+$/u, "") : part.replace(/^\/+|\/+$/gu, ""),
    )
    .join("/");
}

/** True when `path` is strictly inside `root` (not `root` itself). */
export function isInside(path: string, root: string): boolean {
  const base = root.replace(/\/+$/u, "") + "/";
  return (
    path.startsWith(base) &&
    path.length > base.length &&
    !path.split("/").includes("..")
  );
}

/** The directory layout of one profile (`ProfilePaths` in the iOS example). */
export interface ProfilePaths {
  /** `<Application Support>/messenger/profiles/<uuid>` */
  readonly root: string;
  /** `sdk/messages.db3`, the SDK's SQLCipher database. */
  readonly database: string;
  /** `sdk/attachments`, the SDK's attachment directory. */
  readonly attachments: string;
  /** `temp`, attachment staging. */
  readonly temp: string;
  /** `prefs`, per-profile preference files. */
  readonly preferences: string;
  /** `exports`, verified files exported for Open or Save. */
  readonly exports: string;
}

/** The paths of profile `id` under the app's messenger root. */
export function profilePaths(messengerRoot: string, id: string): ProfilePaths {
  const root = joinPath(messengerRoot, "profiles", id);
  return {
    root,
    database: joinPath(root, "sdk", "messages.db3"),
    attachments: joinPath(root, "sdk", "attachments"),
    temp: joinPath(root, "temp"),
    preferences: joinPath(root, "prefs"),
    exports: joinPath(root, "exports"),
  };
}

/** The SQLite files of a database: the file, its `-wal` and its `-shm`. */
export function databaseFiles(database: string): readonly string[] {
  return [database, `${database}-wal`, `${database}-shm`];
}
