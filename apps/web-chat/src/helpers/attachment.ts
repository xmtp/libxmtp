import {
  Storage,
  type Client,
  type PendingAttachment,
  type RemoteAttachment,
} from "@xmtp/browser-sdk";

const pendingAttachmentCleanupKey = "XMTP_PENDING_ATTACHMENT_CLEANUP";
const pendingDatabaseDeletionKey = "XMTP_PENDING_DATABASE_DELETION";

const pendingPaths = (key: string): string[] => {
  const stored = localStorage.getItem(key);
  if (!stored) return [];
  try {
    const parsed: unknown = JSON.parse(stored);
    if (Array.isArray(parsed)) {
      const paths: unknown[] = parsed;
      if (paths.every((path): path is string => typeof path === "string")) {
        return paths;
      }
    }
  } catch {
    // Older versions stored one path as plain text.
  }
  return [stored];
};

const writePendingPaths = (key: string, paths: string[]) => {
  if (paths.length) {
    localStorage.setItem(key, JSON.stringify(paths));
  } else {
    localStorage.removeItem(key);
  }
};

const markPending = (key: string, dbPath: string) => {
  writePendingPaths(key, [...new Set([...pendingPaths(key), dbPath])]);
};

const clearPending = (key: string, dbPath: string) => {
  writePendingPaths(
    key,
    pendingPaths(key).filter((path) => path !== dbPath),
  );
};

export const pendingAttachmentCleanupPaths = async (): Promise<string[]> => {
  let paths: string[];
  try {
    paths = pendingPaths(pendingAttachmentCleanupKey).filter(
      (path) => isCurrentDatabasePath(path) || isLegacyDatabasePath(path),
    );
  } catch {
    return [];
  }
  const admitted = await Promise.all(
    paths.map(
      async (path) =>
        isLegacyDatabasePath(path) || (await hasRecordedDeployment(path)),
    ),
  );
  return paths.filter((_, index) => admitted[index]);
};

export const markAttachmentCleanupPending = (dbPath: string) => {
  markPending(pendingAttachmentCleanupKey, dbPath);
};

export const pendingDatabaseDeletionPaths = (): string[] =>
  pendingPaths(pendingDatabaseDeletionKey);

export const markDatabaseDeletionPending = (dbPath: string) => {
  markPending(pendingDatabaseDeletionKey, dbPath);
};

export const clearDatabaseDeletionPending = (dbPath: string) => {
  clearPending(pendingDatabaseDeletionKey, dbPath);
};

const validSegment = (part: string) =>
  part !== "" && part !== "." && part !== ".." && !part.includes("\\");

export const deploymentHash = async (identifier: string): Promise<string> => {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(identifier),
  );
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
};

const truncateUtf8 = (value: string, limit: number): string => {
  let result = "";
  let bytes = 0;
  for (const character of value) {
    const width = new TextEncoder().encode(character).length;
    if (bytes + width > limit) break;
    result += character;
    bytes += width;
  }
  return result;
};

export const deploymentComponent = async (
  identifier: string,
): Promise<string> => {
  const part = identifier.split(/[\\/]/).at(-1) ?? "";
  let name = Array.from(part)
    .filter((character) => {
      const code = character.codePointAt(0) ?? 0;
      return !(
        code <= 0x1f ||
        (code >= 0x7f && code <= 0x9f) ||
        (code >= 0x202a && code <= 0x202e) ||
        (code >= 0x2066 && code <= 0x2069) ||
        '<>:"|?*'.includes(character)
      );
    })
    .join("")
    .replace(/^[. ]+|[. ]+$/g, "");
  const stem = (name.split(".")[0] ?? "").replace(/[a-z]/g, (letter) =>
    letter.toUpperCase(),
  );
  if (/^(?:CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|(?:COM|LPT)[1-9¹²³])$/.test(stem)) {
    name = `_${name}`;
  }
  const encoder = new TextEncoder();
  if (encoder.encode(name).length > 190) {
    const dot = name.lastIndexOf(".");
    if (dot > 0) {
      const suffix = name.slice(dot);
      const suffixBytes = encoder.encode(suffix).length;
      name =
        suffixBytes >= 190
          ? truncateUtf8(suffix, 190)
          : truncateUtf8(name.slice(0, dot), 190 - suffixBytes) + suffix;
    } else {
      name = truncateUtf8(name, 190);
    }
  }
  name = name.replace(/^[. ]+|[. ]+$/g, "") || "attachment";
  name = name.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  return `${name}-${await deploymentHash(identifier)}`;
};

export const isCurrentDatabasePath = (
  dbPath: string,
  label?: string,
  deploymentName?: string,
) => {
  const parts = dbPath.replace(/^\/+/, "").split("/");
  return (
    parts.length === 5 &&
    parts[0] === "xmtp-sdk" &&
    validSegment(parts[1]) &&
    (label === undefined || parts[1] === label) &&
    /^[^/\\]{1,190}-[0-9a-f]{64}$/.test(parts[2]) &&
    (deploymentName === undefined || parts[2] === deploymentName) &&
    /^[0-9a-f]{64}$/.test(parts[3]) &&
    parts[4] === "xmtp.db3"
  );
};

export const isLegacyDatabasePath = (dbPath: string, label?: string) => {
  const path = dbPath.replace(/^\/+/, "");
  const match = /^xmtp-(.+)-[0-9a-f]{64}\.db3$/i.exec(path);
  return (
    !path.includes("/") &&
    match !== null &&
    validSegment(match[1]) &&
    (label === undefined || match[1] === label)
  );
};

const recordedDeploymentComponents = async (
  backend: FileSystemDirectoryHandle,
): Promise<Set<string>> => {
  let file: FileSystemFileHandle;
  try {
    file = await backend.getFileHandle("deployments.json");
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "NotFoundError") {
      return new Set();
    }
    throw cause;
  }
  let record: unknown;
  try {
    record = JSON.parse(await (await file.getFile()).text());
  } catch {
    return new Set();
  }
  if (typeof record !== "object" || record === null || Array.isArray(record)) {
    return new Set();
  }
  const { version, deployments } = record as {
    version?: unknown;
    deployments?: unknown;
  };
  if (
    version !== 1 ||
    typeof deployments !== "object" ||
    deployments === null ||
    Array.isArray(deployments)
  ) {
    return new Set();
  }
  const identifiers = Object.values(deployments);
  if (
    !identifiers.every((value): value is string => typeof value === "string")
  ) {
    return new Set();
  }
  return new Set(await Promise.all(identifiers.map(deploymentComponent)));
};

const hasRecordedDeployment = async (dbPath: string): Promise<boolean> => {
  if (!isCurrentDatabasePath(dbPath)) return false;
  const [, label, deployment] = dbPath.replace(/^\/+/, "").split("/");
  const root = await navigator.storage.getDirectory();
  try {
    const sdk = await root.getDirectoryHandle("xmtp-sdk");
    const backend = await sdk.getDirectoryHandle(label);
    return (await recordedDeploymentComponents(backend)).has(deployment);
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "NotFoundError") {
      return false;
    }
    throw cause;
  }
};

export const retryPendingDatabaseDeletions = async () => {
  for (const dbPath of pendingDatabaseDeletionPaths()) {
    const current = isCurrentDatabasePath(dbPath);
    const legacy = isLegacyDatabasePath(dbPath);
    if (!current && !legacy) throw new Error("Invalid local database path.");
    // A legacy file name does not identify its backend deployment.
    if (legacy) continue;
    if (!(await hasRecordedDeployment(dbPath))) continue;
    const admin = await Storage.admin();
    try {
      await admin.deleteFile(dbPath);
    } finally {
      await admin.end();
    }
    await cleanAttachmentDirectory(dbPath);
    clearDatabaseDeletionPending(dbPath);
  }
};

export const cleanAttachmentDirectory = async (dbPath: string | undefined) => {
  if (dbPath === undefined) return;
  attachmentFolders(dbPath);
  try {
    markAttachmentCleanupPending(dbPath);
  } catch {
    // Remove the files now even when the journal cannot be written.
  }
  await removeAttachmentDirectory(dbPath);
  try {
    clearPending(pendingAttachmentCleanupKey, dbPath);
  } catch {
    // A stale path causes one more safe cleanup attempt.
  }
};

// End a session without removing ciphertext needed by pending uploads.
export const cleanSessionAttachments = async (dbPath: string | undefined) => {
  if (dbPath === undefined) return;
  attachmentFolders(dbPath);
  try {
    markAttachmentCleanupPending(dbPath);
  } catch {
    // Try the OPFS cleanup even if the journal is unavailable.
  }
  await removePlaintextAttachmentDirectories(dbPath);
  try {
    clearPending(pendingAttachmentCleanupKey, dbPath);
  } catch {
    // A stale path causes one more safe cleanup attempt.
  }
};

// Check every app-owned attachment directory before a new session starts.
export const cleanStoredSessionAttachments = async () => {
  const root = await navigator.storage.getDirectory();
  for await (const [name, handle] of root.entries()) {
    if (handle.kind !== "directory") continue;
    if (name !== "xmtp-sdk") {
      if (name.endsWith(".attachments")) {
        const dbPath = name.slice(0, -".attachments".length);
        if (isLegacyDatabasePath(dbPath)) {
          await removePlaintextAttachmentDirectories(dbPath);
        }
      }
      continue;
    }
    for await (const [label, backend] of (
      handle as FileSystemDirectoryHandle
    ).entries()) {
      if (backend.kind !== "directory") continue;
      const recorded = await recordedDeploymentComponents(
        backend as FileSystemDirectoryHandle,
      );
      for await (const [database, deployment] of (
        backend as FileSystemDirectoryHandle
      ).entries()) {
        if (deployment.kind !== "directory") continue;
        if (!recorded.has(database)) continue;
        for await (const [inbox, directory] of (
          deployment as FileSystemDirectoryHandle
        ).entries()) {
          if (directory.kind !== "directory") continue;
          const dbPath = `xmtp-sdk/${label}/${database}/${inbox}/xmtp.db3`;
          if (isCurrentDatabasePath(dbPath)) {
            await removePlaintextAttachmentDirectories(dbPath);
          }
        }
      }
    }
  }
};

const MAX_FILE_SIZE = 1024 * 1024; // 1MB

const ALLOWED_FILE_TYPES = [
  "image/jpeg",
  "image/jpg",
  "image/png",
  "image/gif",
  "image/webp",
  "video/mp4",
  "video/webm",
  "video/quicktime",
  "audio/mpeg",
  "audio/mp3",
  "audio/wav",
  "audio/ogg",
];

export type FileValidation =
  | {
      valid: true;
    }
  | {
      valid: false;
      error: string;
    };

const attachmentFolders = (dbPath: string): string[] => {
  const path = dbPath.replace(/^\/+/, "");
  if (isCurrentDatabasePath(dbPath))
    return [...path.split("/").slice(0, -1), "attachments"];
  if (isLegacyDatabasePath(dbPath)) return [`${path}.attachments`];
  throw new Error("Invalid local database path.");
};

export const removePlaintextAttachmentDirectories = async (
  dbPath: string | undefined,
) => {
  if (dbPath === undefined) return;
  const folders = attachmentFolders(dbPath);
  let parent = await navigator.storage.getDirectory();
  try {
    for (const folder of folders) {
      parent = await parent.getDirectoryHandle(folder);
    }
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "NotFoundError") return;
    throw cause;
  }
  const names: string[] = [];
  for await (const [name, entry] of parent.entries()) {
    if (
      entry.kind === "directory" &&
      (name === ".tmp" || /^[0-9a-f]{64}$/.test(name))
    ) {
      names.push(name);
    }
  }
  for (const name of names) {
    await parent.removeEntry(name, { recursive: true });
  }
};

export const removeAttachmentDirectory = async (dbPath: string | undefined) => {
  if (dbPath === undefined) return;
  const folders = attachmentFolders(dbPath);

  const name = folders.pop();
  if (!name) throw new Error("Invalid local database path.");
  let parent = await navigator.storage.getDirectory();
  try {
    for (const folder of folders) {
      parent = await parent.getDirectoryHandle(folder);
    }
    await parent.removeEntry(name, { recursive: true });
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "NotFoundError") return;
    throw cause;
  }
};

export const validateFile = (file: File): FileValidation => {
  if (file.size > MAX_FILE_SIZE) {
    return {
      valid: false,
      error: "File size must not be greater than 1MB",
    };
  }

  if (!ALLOWED_FILE_TYPES.includes(file.type)) {
    return {
      valid: false,
      error:
        "File type not supported. Please select an image, video, or audio file.",
    };
  }

  return { valid: true };
};

export const uploadEncryptedAttachment = async (
  client: Client,
  file: File,
  pendingRef?: { current: PendingAttachment | null },
): Promise<RemoteAttachment> => {
  let pending = pendingRef?.current;
  if (!pending) {
    pending = await client.attachments.create({
      kind: "bytes",
      bytes: new Uint8Array(await file.arrayBuffer()),
      mimeType: file.type,
      filename: file.name,
    });
    if (pendingRef) pendingRef.current = pending;
  }
  await pending.upload();
  try {
    await client.attachments.deleteLocal(pending.remoteAttachment);
  } catch {
    // The upload is complete. Keep its remote record available to send.
  }
  return pending.remoteAttachment;
};

export const downloadRemoteAttachment = async (
  client: Client,
  content: RemoteAttachment,
) => {
  const attachment = await client.attachments.download(content);
  try {
    const parts = attachment.path.split("/").filter(Boolean);
    const filename = parts.pop();
    if (!filename || parts.includes(".."))
      throw new Error("Invalid attachment path");
    let directory = await navigator.storage.getDirectory();
    for (const part of parts)
      directory = await directory.getDirectoryHandle(part);
    const handle = await directory.getFileHandle(filename);
    const file = await handle.getFile();
    return new Blob([await file.arrayBuffer()], {
      type: attachment.mimeType ?? file.type,
    });
  } finally {
    try {
      await client.attachments.deleteLocal(content);
    } catch {
      // A local cleanup error must not discard a completed download.
    }
  }
};

export const getFileType = (filename: string) => {
  const extension = filename.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "jpg":
    case "jpeg":
    case "png":
    case "gif":
    case "webp":
      return "image";
    case "mp4":
    case "webm":
    case "mov":
      return "video";
    case "mp3":
    case "wav":
    case "ogg":
      return "audio";
    default:
      return "file";
  }
};

export const formatFileSize = (fileSize: number | undefined) => {
  if (!fileSize) return "";
  const kb = fileSize / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(1)} KB`;
  }
  const mb = kb / 1024;
  return `${mb.toFixed(1)} MB`;
};
