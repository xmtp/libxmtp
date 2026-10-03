import { type Client, type RemoteAttachment } from "@xmtp/browser-sdk";

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
  await pending.upload();
  return pending.remoteAttachment;
};

export const downloadRemoteAttachment = async (
  client: Client,
  content: RemoteAttachment,
) => {
  const attachment = await client.attachments.download(content);
  const parts = attachment.path.split("/").filter(Boolean);
  const filename = parts.pop();
  if (!filename || parts.includes(".."))
    throw new Error("Invalid attachment path");
  let directory = await navigator.storage.getDirectory();
  for (const part of parts)
    directory = await directory.getDirectoryHandle(part);
  const handle = await directory.getFileHandle(filename);
  const file = await handle.getFile();
  return file.slice(0, file.size, attachment.mimeType ?? file.type);
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
