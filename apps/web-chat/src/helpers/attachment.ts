import { Storage, type Client, type RemoteAttachment } from "@xmtp/browser-sdk";

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

export const pendingAttachmentCleanupPaths = (): string[] => {
  try {
    return pendingPaths(pendingAttachmentCleanupKey);
  } catch {
    return [];
  }
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

export const retryPendingDatabaseDeletions = async () => {
  for (const dbPath of pendingDatabaseDeletionPaths()) {
    const parts = dbPath.replace(/^\/+/, "").split("/");
    const current =
      parts.length === 5 &&
      parts[0] === "xmtp-sdk" &&
      parts.slice(1, 4).every(validSegment) &&
      parts[4] === "xmtp.db3";
    const legacy =
      parts.length === 1 && /^xmtp-.+-[0-9a-f]{64}\.db3$/i.test(parts[0]);
    if (!current && !legacy) throw new Error("Invalid local database path.");
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

export const removeAttachmentDirectory = async (dbPath: string | undefined) => {
  if (dbPath === undefined) return;
  const parts = dbPath.replace(/^\/+/, "").split("/");
  if (
    parts.some(
      (part) => !part || part === "." || part === ".." || part.includes("\\"),
    )
  ) {
    throw new Error("Invalid local database path.");
  }
  const folders =
    parts.length > 1 && parts.at(-1) === "xmtp.db3"
      ? [...parts.slice(0, -1), "attachments"]
      : parts.length === 1 && parts[0].endsWith(".db3")
        ? [`${parts[0]}.attachments`]
        : null;
  if (!folders) throw new Error("Invalid local database path.");

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
): Promise<RemoteAttachment> => {
  const pending = await client.attachments.create({
    kind: "bytes",
    bytes: new Uint8Array(await file.arrayBuffer()),
    mimeType: file.type,
    filename: file.name,
  });
  try {
    await pending.upload();
    return pending.remoteAttachment;
  } finally {
    await client.attachments.deleteLocal(pending.remoteAttachment);
  }
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
    await client.attachments.deleteLocal(content);
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
