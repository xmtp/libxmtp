import { decryptAttachment, type RemoteAttachment } from "@xmtp/browser-sdk";

export const ATTACHMENT_UPLOADS_ENABLED: boolean = false;

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

export const uploadEncryptedAttachment = (
  _file: File,
): Promise<RemoteAttachment> =>
  Promise.reject(
    new Error(
      "Attachment uploads are disabled until the backend supports remote attachments",
    ),
  );

export const downloadRemoteAttachment = async (content: RemoteAttachment) => {
  const response = await fetch(content.url);
  if (!response.ok) {
    throw new Error(
      `Unable to load attachment: [${response.status}] ${response.statusText}`,
    );
  }
  const payload = new Uint8Array(await response.arrayBuffer());
  return decryptAttachment(payload, content);
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
