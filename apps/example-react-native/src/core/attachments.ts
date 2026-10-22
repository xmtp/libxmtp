// Attachment models and rules: file phases, saved upload drafts, the
// verified-file cache, SDK failure copy, and the byte-count and file-type text
// the cards show.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import { bytesToHex, hexToBytes } from "./hex.ts";

/** Drafts use waiting → uploading → complete; cards use available → downloading → verified. */
export type FilePhase =
  | "available"
  | "waiting"
  | "uploading"
  | "complete"
  | "downloading"
  | "verified"
  | "failed"
  | "unknown"
  | "expired";

/** What a file card shows (no keys or URL). */
export interface FileCard {
  readonly id: string;
  readonly name: string;
  readonly length?: number;
  readonly phase: FilePhase;
  readonly error?: string;
  readonly retryable: boolean;
}

/** An upload draft, saved before it is sent. */
export interface FileDraft {
  /** A UUID; also the send journal entry ID. */
  readonly id: string;
  /** Undefined for an SDK pending upload that no chat claims yet. */
  readonly conversationId?: string;
  readonly file: RemoteAttachment;
  readonly phase: FilePhase;
  /** The SDK message ID once the send was accepted. */
  readonly acceptedId?: string;
  readonly retryable: boolean;
}

/** A downloaded, digest-checked and decrypted file. */
export interface VerifiedFile {
  /** Native path. */
  readonly path: string;
  readonly mimeType?: string;
  readonly filename: string;
}

export const AttachmentCopy = {
  file: "File",
  // Card status and actions.
  closed: "Closed until you accept",
  available: "Download to verify",
  downloading: "Downloading",
  verified: "Verified",
  failed: "Failed",
  download: "Download",
  retry: "Retry",
  open: "Open",
  save: "Save",
  openFailed: "Cannot open this file. Try again.",
  saveFailed: "Cannot save this file. Try again.",
  removeFailed: "Cannot remove this temporary file. Try again.",
  // Composer.
  addAttachment: "Add attachment",
  photos: "Photos and videos",
  files: "Files",
  testFile: "Test file",
  send: "Send",
  discard: "Discard",
  uploading: "Uploading",
  reviewSend: "Review send",
  tooLarge: "File exceeds the server upload limit",
  unreadable: "Cannot read this file. Try again.",
  selectFileFailed: "Cannot select this file.",
  selectPhotoFailed: "Cannot select this photo.",
  testFileFailed: "Cannot stage test file.",
  // SDK failures.
  notOffered: "This server does not support file uploads.",
  sizeLimit: "This file exceeds the server size limit.",
  unavailable: "This file is unavailable. Select the file again.",
  notFound: "This file is not available for download. Try again later.",
  unverified: "This file could not be verified.",
  notAllowed: "This file location is not allowed.",
  refused: "File access was refused. Check your server access.",
  operationFailed: "The file operation failed. Try again.",
} as const;

/** A failure with the copy to show and whether Retry is offered. */
export class AttachmentError extends Error {
  readonly retryable: boolean;

  constructor(message: string, retryable: boolean) {
    super(message);
    this.name = "AttachmentError";
    this.retryable = retryable;
  }
}

/** The SDK `AttachmentFailure` fields the mapping reads. */
export interface AttachmentFailureLike {
  readonly cause: string;
  readonly retryable: boolean;
  readonly httpStatus?: number | undefined;
}

function failureMessage(cause: string): string {
  switch (cause) {
    case "notOffered":
      return AttachmentCopy.notOffered;
    case "tooLarge":
      return AttachmentCopy.sizeLimit;
    case "stagedUnusable":
    case "deleted":
      return AttachmentCopy.unavailable;
    case "notFound":
      return AttachmentCopy.notFound;
    case "digestMismatch":
    case "decryptionFailed":
    case "notAnAttachment":
    case "malformed":
      return AttachmentCopy.unverified;
    case "blockedAddress":
    case "insecureUrl":
      return AttachmentCopy.notAllowed;
    case "credential":
      return AttachmentCopy.refused;
    default:
      return AttachmentCopy.operationFailed;
  }
}

function causeRetryable(failure: AttachmentFailureLike): boolean {
  switch (failure.cause) {
    case "localStorage":
    case "backendUnavailable":
    case "network":
    case "targetRejected":
    case "notFound":
    case "deleted":
      return true;
    case "credential":
      return failure.retryable;
    case "httpStatus": {
      const status = failure.httpStatus;
      return (
        status !== undefined &&
        (status === 408 || status === 429 || (status >= 500 && status <= 599))
      );
    }
    default:
      return false;
  }
}

/**
 * The copy and retry rule of an SDK attachment failure.
 * A thrown error's own `retryable` detail wins over the cause table.
 */
export function attachmentFailure(
  failure: AttachmentFailureLike,
  retryable?: boolean,
): AttachmentError {
  return new AttachmentError(
    failureMessage(failure.cause),
    retryable ?? causeRetryable(failure),
  );
}

/** The default failure: "The file operation failed. Try again.", retryable. */
export function operationFailed(): AttachmentError {
  return new AttachmentError(AttachmentCopy.operationFailed, true);
}

/** The card status line. */
export function cardStatus(phase: FilePhase, allowed: boolean): string {
  if (!allowed) return AttachmentCopy.closed;
  switch (phase) {
    case "available":
      return AttachmentCopy.available;
    case "downloading":
      return AttachmentCopy.downloading;
    case "verified":
      return AttachmentCopy.verified;
    case "failed":
      return AttachmentCopy.failed;
    default:
      return phase.charAt(0).toUpperCase() + phase.slice(1);
  }
}

/** The card of a file message that has no download state yet. */
export function defaultCard(id: string, file: RemoteAttachment): FileCard {
  return {
    id,
    name: file.filename ?? AttachmentCopy.file,
    ...(file.contentLength === undefined ? {} : { length: file.contentLength }),
    phase: "available",
    retryable: false,
  };
}

/** `value` with at most `digits` fraction digits: "1.20" → "1.2", "10" stays. */
function trimFraction(value: number, digits: number): string {
  const text = value.toFixed(digits);
  // Without a fraction there is nothing to trim: "10" keeps its zero.
  return digits === 0 ? text : text.replace(/\.?0+$/u, "");
}

/**
 * `ByteCountFormatter.string(fromByteCount:countStyle: .file)` in en-US:
 * decimal units, bytes and KB without fractions, MB with one digit and GB or
 * more with two ("Zero KB", "12 bytes", "212 KB", "1.2 MB").
 */
export function byteCount(bytes: number): string {
  if (bytes === 0) return "Zero KB";
  if (bytes === 1) return "1 byte";
  if (bytes < 1000) return `${String(bytes)} bytes`;
  const units = [
    { name: "KB", digits: 0 },
    { name: "MB", digits: 1 },
    { name: "GB", digits: 2 },
    { name: "TB", digits: 2 },
  ];
  let value = bytes / 1000;
  for (const [index, unit] of units.entries()) {
    const rounded = Number(value.toFixed(unit.digits));
    if (rounded < 1000 || index === units.length - 1) {
      return `${trimFraction(value, unit.digits)} ${unit.name}`;
    }
    value /= 1000;
  }
  return `${String(bytes)} bytes`;
}

/** The lowercase extension of a file name, without the dot ("" when none). */
function fileExtension(name: string): string {
  const base = name.slice(name.lastIndexOf("/") + 1);
  const dot = base.lastIndexOf(".");
  return dot <= 0 ? "" : base.slice(dot + 1).toLowerCase();
}

/**
 * Type descriptions and MIME types of common extensions: UTType
 * `localizedDescription` and `preferredMIMEType`, read on the iOS 27 simulator.
 */
const fileTypes: Partial<
  Record<string, { readonly description: string; readonly mime: string }>
> = {
  jpg: { description: "JPEG image", mime: "image/jpeg" },
  jpeg: { description: "JPEG image", mime: "image/jpeg" },
  png: { description: "PNG image", mime: "image/png" },
  gif: { description: "GIF image", mime: "image/gif" },
  heic: { description: "HEIF Image", mime: "image/heic" },
  heif: { description: "HEIF Image", mime: "image/heif" },
  webp: { description: "WebP Image", mime: "image/webp" },
  tif: { description: "TIFF image", mime: "image/tiff" },
  tiff: { description: "TIFF image", mime: "image/tiff" },
  bmp: { description: "Windows BMP image", mime: "image/bmp" },
  svg: { description: "SVG image", mime: "image/svg+xml" },
  mov: { description: "QuickTime movie", mime: "video/quicktime" },
  mp4: { description: "MPEG-4 movie", mime: "video/mp4" },
  m4v: { description: "Apple MPEG-4 movie", mime: "video/x-m4v" },
  mp3: { description: "MP3 audio", mime: "audio/mpeg" },
  m4a: { description: "Apple MPEG-4 audio", mime: "audio/x-m4a" },
  wav: { description: "Waveform audio", mime: "audio/vnd.wave" },
  aac: { description: "AAC audio", mime: "audio/aac" },
  pdf: { description: "PDF document", mime: "application/pdf" },
  txt: { description: "text", mime: "text/plain" },
  md: { description: "Markdown", mime: "text/markdown" },
  rtf: { description: "rich text (RTF)", mime: "text/rtf" },
  html: { description: "HTML text", mime: "text/html" },
  json: { description: "JSON", mime: "application/json" },
  xml: { description: "XML text", mime: "application/xml" },
  yaml: { description: "YAML", mime: "application/x-yaml" },
  csv: { description: "comma-separated values", mime: "text/csv" },
  ics: { description: "ICS File", mime: "text/calendar" },
  vcf: { description: "electronic business card", mime: "text/vcard" },
  zip: { description: "Zip archive", mime: "application/zip" },
  gz: { description: "GZip archive", mime: "application/x-gzip" },
  tar: { description: "tar archive", mime: "application/x-tar" },
  "7z": { description: "7-Zip archive", mime: "application/x-7z-compressed" },
  doc: {
    description: "Microsoft Word 97-2004 document",
    mime: "application/msword",
  },
  docx: {
    description: "Office Open XML word processing document",
    mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  },
  xls: {
    description: "Microsoft Excel 97-2004 worksheet",
    mime: "application/vnd.ms-excel",
  },
  xlsx: {
    description: "Office Open XML spreadsheet",
    mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  },
  ppt: {
    description: "Microsoft PowerPoint 97-2004 presentation",
    mime: "application/vnd.ms-powerpoint",
  },
  pptx: {
    description: "Office Open XML presentation",
    mime: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  },
  key: {
    description: "Keynote document",
    mime: "application/x-iwork-keynote-sffkey",
  },
  pages: {
    description: "Pages document",
    mime: "application/x-iwork-pages-sffpages",
  },
  numbers: {
    description: "Numbers document",
    mime: "application/x-iwork-numbers-sffnumbers",
  },
};

/** The staged preview's type line ("JPEG image"; "File" when unknown). */
function fileTypeDescription(name: string): string {
  return fileTypes[fileExtension(name)]?.description ?? AttachmentCopy.file;
}

/**
 * The MIME type the SDK records for a file: from the extension, else the
 * picker's type, else `application/octet-stream`.
 */
export function mimeTypeFor(name: string, pickerType?: string): string {
  return (
    fileTypes[fileExtension(name)]?.mime ??
    (pickerType === undefined || pickerType === ""
      ? "application/octet-stream"
      : pickerType)
  );
}

/** The staged preview's metadata line: "JPEG image · 212 KB". */
export function draftMetadata(draft: FileDraft): string {
  const type = fileTypeDescription(draft.file.filename ?? "");
  const length = draft.file.contentLength;
  return length === undefined ? type : `${type} · ${byteCount(length)}`;
}

/** Shared media puts these extensions under "Photos and videos". */
const visualExtensions = new Set([
  "jpg",
  "jpeg",
  "png",
  "gif",
  "webp",
  "heic",
  "mov",
  "mp4",
]);

export function isVisualFile(name: string | undefined): boolean {
  return visualExtensions.has(fileExtension(name ?? ""));
}

/** A picked photo's name: its own, or "Photo." + extension (default jpg). */
export function photoName(
  fileName: string | undefined | null,
  mimeType?: string | null,
): string {
  if (
    fileName !== undefined &&
    fileName !== null &&
    fileExtension(fileName) !== ""
  ) {
    return fileName;
  }
  const subtype = mimeType?.split("/")[1]?.toLowerCase();
  const extension =
    subtype === undefined || subtype === ""
      ? "jpg"
      : subtype === "quicktime"
        ? "mov"
        : subtype;
  return `Photo.${extension}`;
}

/** The last path component of a name, never empty (exports keep only it). */
export function safeFileName(name: string): string {
  const base = name.slice(name.lastIndexOf("/") + 1);
  return base === "" || base === "." || base === ".." ? "File" : base;
}

/** True when a staged copy of `size` bytes exceeds the server limit. */
export function exceedsLimit(size: number, limit: bigint): boolean {
  return BigInt(size) > limit;
}

/** Two descriptors name the same uploaded file. */
export function sameFile(a: RemoteAttachment, b: RemoteAttachment): boolean {
  return (
    a.url === b.url &&
    a.contentDigest === b.contentDigest &&
    a.filename === b.filename
  );
}

/** Two drafts are equal (all fields, including the descriptor's bytes). */
export function sameDraft(a: FileDraft | undefined, b: FileDraft): boolean {
  return (
    a !== undefined &&
    JSON.stringify(encodeDraft(a)) === JSON.stringify(encodeDraft(b))
  );
}

// Saved drafts: `attachment-<UUID>` preference files listed in
// `attachment-refs`. Bytes are stored as hex.

interface SavedDraft {
  readonly id: string;
  readonly conversationId?: string;
  readonly phase: FilePhase;
  readonly acceptedId?: string;
  readonly retryable: boolean;
  readonly file: {
    readonly url: string;
    readonly contentDigest: string;
    readonly secret: string;
    readonly salt: string;
    readonly nonce: string;
    readonly scheme: string;
    readonly contentLength?: number;
    readonly filename?: string;
  };
}

const phases: ReadonlySet<string> = new Set<FilePhase>([
  "available",
  "waiting",
  "uploading",
  "complete",
  "downloading",
  "verified",
  "failed",
  "unknown",
  "expired",
]);

function encodeDraft(draft: FileDraft): SavedDraft {
  const { file } = draft;
  return {
    id: draft.id,
    ...(draft.conversationId === undefined
      ? {}
      : { conversationId: draft.conversationId }),
    phase: draft.phase,
    ...(draft.acceptedId === undefined ? {} : { acceptedId: draft.acceptedId }),
    retryable: draft.retryable,
    file: {
      url: file.url,
      contentDigest: file.contentDigest,
      secret: bytesToHex(file.secret),
      salt: bytesToHex(file.salt),
      nonce: bytesToHex(file.nonce),
      scheme: file.scheme,
      ...(file.contentLength === undefined
        ? {}
        : { contentLength: file.contentLength }),
      ...(file.filename === undefined ? {} : { filename: file.filename }),
    },
  };
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function optionalString(record: Record<string, unknown>, key: string) {
  const value = record[key];
  if (value !== undefined && typeof value !== "string") {
    throw new Error("A saved attachment is malformed.");
  }
  return value;
}

function bytes(record: Record<string, unknown>, key: string): Uint8Array {
  const value = record[key];
  const parsed = typeof value === "string" ? hexToBytes(value) : undefined;
  if (parsed === undefined) throw new Error("A saved attachment is malformed.");
  return parsed;
}

/** Reads a saved draft. Throws on malformed data. */
function parseDraft(value: unknown): FileDraft {
  if (!isRecord(value) || !isRecord(value["file"])) {
    throw new Error("A saved attachment is malformed.");
  }
  const file = value["file"];
  const phase = value["phase"];
  if (
    typeof value["id"] !== "string" ||
    typeof phase !== "string" ||
    !phases.has(phase) ||
    typeof value["retryable"] !== "boolean" ||
    typeof file["url"] !== "string" ||
    typeof file["contentDigest"] !== "string" ||
    typeof file["scheme"] !== "string" ||
    (file["contentLength"] !== undefined &&
      typeof file["contentLength"] !== "number")
  ) {
    throw new Error("A saved attachment is malformed.");
  }
  const conversationId = optionalString(value, "conversationId");
  const acceptedId = optionalString(value, "acceptedId");
  const filename = optionalString(file, "filename");
  const contentLength = file["contentLength"];
  return {
    id: value["id"],
    ...(conversationId === undefined ? {} : { conversationId }),
    phase: phase as FilePhase,
    ...(acceptedId === undefined ? {} : { acceptedId }),
    retryable: value["retryable"],
    file: {
      url: file["url"],
      contentDigest: file["contentDigest"],
      secret: bytes(file, "secret"),
      salt: bytes(file, "salt"),
      nonce: bytes(file, "nonce"),
      scheme: file["scheme"],
      ...(contentLength === undefined ? {} : { contentLength }),
      ...(filename === undefined ? {} : { filename }),
    },
  };
}

/** The preference calls the draft store uses (`ProfilePreferences`). */
export interface DraftPreferences {
  readJson(name: string): unknown;
  writeJson(name: string, value: unknown): void;
  remove(name: string): void;
  names(prefix: string): readonly string[];
}

const DRAFT_PREFIX = "attachment-";
const REFERENCES = "attachment-refs";

/** Saved upload drafts, shared by every attachment store of a session. */
export class AttachmentDraftStore {
  readonly #preferences: DraftPreferences;
  readonly #listeners = new Set<() => void>();

  constructor(preferences: DraftPreferences) {
    this.#preferences = preferences;
  }

  /**
   * Calls `listener` after each saved or removed draft, until the returned
   * function runs.
   */
  onChange(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  #changed(): void {
    for (const listener of Array.from(this.#listeners)) listener();
  }

  #references(): string[] {
    const value = this.#preferences.readJson(REFERENCES);
    if (value === undefined) return [];
    if (
      !Array.isArray(value) ||
      !value.every((entry) => typeof entry === "string")
    ) {
      throw new Error("The attachment list is malformed.");
    }
    return value;
  }

  #read(name: string): FileDraft | undefined {
    const value = this.#preferences.readJson(name);
    return value === undefined ? undefined : parseDraft(value);
  }

  /** Every listed draft whose file exists, in the order they were saved. */
  all(): FileDraft[] {
    return this.#references().flatMap((name) => {
      const draft = this.#read(name);
      return draft === undefined ? [] : [draft];
    });
  }

  save(draft: FileDraft): void {
    const name = DRAFT_PREFIX + draft.id;
    this.#preferences.writeJson(name, encodeDraft(draft));
    const references = this.#references();
    if (!references.includes(name)) {
      this.#preferences.writeJson(REFERENCES, [...references, name]);
    }
    this.#changed();
  }

  remove(id: string): void {
    const name = DRAFT_PREFIX + id;
    this.#preferences.remove(name);
    this.#preferences.writeJson(
      REFERENCES,
      this.#references().filter((entry) => entry !== name),
    );
    this.#changed();
  }

  /** Removes draft files nobody lists and listed names whose file is gone. */
  cleanOrphans(): void {
    const references = new Set(this.#references());
    for (const name of this.#preferences.names(DRAFT_PREFIX)) {
      if (name !== REFERENCES && !references.has(name)) {
        this.#preferences.remove(name);
      }
    }
    this.#preferences.writeJson(
      REFERENCES,
      [...references].filter(
        (name) => this.#preferences.readJson(name) !== undefined,
      ),
    );
  }
}

/** Local plaintext is already verified; inspect only a short header for display type. */
export function localMimeType(name: string, header: Uint8Array): string {
  const starts = (...bytes: number[]) =>
    bytes.every((byte, index) => header[index] === byte);
  const text = (offset: number, value: string) =>
    Array.from({ length: value.length }, (_, index) =>
      value.charCodeAt(index),
    ).every((byte, index) => header[offset + index] === byte);
  if (starts(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a))
    return "image/png";
  if (starts(0xff, 0xd8, 0xff)) return "image/jpeg";
  if (text(0, "GIF87a") || text(0, "GIF89a")) return "image/gif";
  if (text(0, "RIFF") && text(8, "WEBP")) return "image/webp";
  if (starts(0x49, 0x49, 0x2a, 0) || starts(0x4d, 0x4d, 0, 0x2a))
    return "image/tiff";
  if (text(0, "BM")) return "image/bmp";
  if (text(4, "ftyp")) {
    if (text(8, "avif") || text(8, "avis")) return "image/avif";
    if (["heic", "heix", "hevc", "hevx"].some((brand) => text(8, brand)))
      return "image/heic";
    if (text(8, "mif1") || text(8, "msf1")) return "image/heif";
  }
  return mimeTypeFor(name);
}
