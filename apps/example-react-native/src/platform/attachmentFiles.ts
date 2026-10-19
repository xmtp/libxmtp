// Attachment files on the device: pickers, staging copies into the profile's
// protected `temp/attachment-staging/<uuid>`, export copies into
// `exports/<uuid>/<name>`, Quick Look and Save to Files.

import {
  errorCodes,
  isErrorWithCode,
  saveDocuments,
} from "@react-native-documents/picker";
import { getDocumentAsync } from "expo-document-picker";
import { File, Paths } from "expo-file-system";
import { launchImageLibraryAsync } from "expo-image-picker";
import { Platform } from "react-native";

import { exceedsLimit, photoName, safeFileName } from "@/core/attachments.ts";
import { errorLogText } from "@/core/errorLog.ts";
import { isInside, joinPath, pathToUri, uriToPath } from "@/core/paths.ts";
import {
  directoryNames,
  ensureDirectory,
  ensureProtectedDirectory,
  removePath,
} from "@/platform/files";
import { randomUuid } from "@/platform/random";

/** The staged copy would exceed the server's upload limit. */
export class StagingTooLargeError extends Error {
  constructor() {
    super("File exceeds the server upload limit");
    this.name = "StagingTooLargeError";
  }
}

/** A file the user picked: a `file://` URI the app can read, its name and type. */
export interface PickedFile {
  readonly uri: string;
  readonly name: string;
  readonly mimeType?: string;
}

const UUID = /^[0-9A-Fa-f]{8}(-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}$/u;

/** `<profile>/temp/attachment-staging`. */
export function stagingDirectory(temp: string): string {
  return joinPath(temp, "attachment-staging");
}

function newStagingPath(directory: string): string {
  ensureProtectedDirectory(directory);
  return joinPath(directory, randomUuid());
}

/**
 * Copies a picked file into the staging directory, checking the size against
 * the server limit before and after the copy (a partial or oversized copy is
 * removed). Returns the staged native path.
 */
export async function stageCopy(
  sourceUri: string,
  directory: string,
  limit: bigint,
): Promise<string> {
  const source = new File(sourceUri);
  if (exceedsLimit(source.size, limit)) throw new StagingTooLargeError();
  const target = newStagingPath(directory);
  try {
    const file = new File(pathToUri(target));
    await source.copy(file);
    if (exceedsLimit(new File(pathToUri(target)).size, limit)) {
      throw new StagingTooLargeError();
    }
    return target;
  } catch (error) {
    removePath(target);
    throw error;
  }
}

/** Writes base64 bytes (the Debug "Test file") as a staged file of at most `maxBytes`. */
export function stageBase64(
  base64: string,
  directory: string,
  limit: bigint,
  maxBytes: number,
): string {
  const target = newStagingPath(directory);
  try {
    const file = new File(pathToUri(target));
    file.writeSync(base64, { encoding: "base64" });
    const size = new File(pathToUri(target)).size;
    if (size > maxBytes || exceedsLimit(size, limit)) {
      throw new StagingTooLargeError();
    }
    return target;
  } catch (error) {
    removePath(target);
    throw error;
  }
}

/** Removes a staged copy, only when it is a UUID-named file in `directory`. */
export function removeStaged(path: string, directory: string): void {
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (!isInside(path, directory) || !UUID.test(name)) return;
  removePath(path);
}

/** Removes staged copies a closed session left behind (UUID-named entries only). */
export function removeAbandonedStaging(directory: string): void {
  for (const name of directoryNames(directory)) {
    if (UUID.test(name)) removePath(joinPath(directory, name));
  }
}

/** The iOS document picker's own copy: `<container>/tmp/<bundle>-Inbox/…`. */
function isPickerInboxCopy(path: string): boolean {
  const container = uriToPath(Paths.document.uri).replace(
    /\/Documents\/?$/u,
    "",
  );
  const tmp = joinPath(container, "tmp");
  if (!isInside(path, tmp)) return false;
  return (path.slice(tmp.length + 1).split("/")[0] ?? "").endsWith("-Inbox");
}

/**
 * Removes a picker's own copy: one inside the app's cache directory, or the
 * iOS document picker's Inbox copy.
 */
export function removePickerCopy(uri: string): void {
  try {
    const path = uriToPath(uri);
    if (isInside(path, uriToPath(Paths.cache.uri)) || isPickerInboxCopy(path)) {
      removePath(path);
    }
  } catch (error) {
    console.warn(
      "Cannot remove a picked file's cache copy",
      errorLogText(error),
    );
  }
}

/** "Photos and videos": one image or video, or undefined when cancelled. */
export async function pickPhoto(): Promise<PickedFile | undefined> {
  const result = await launchImageLibraryAsync({
    mediaTypes: ["images", "videos"],
    allowsMultipleSelection: false,
  });
  const asset = result.canceled ? undefined : result.assets[0];
  if (asset === undefined) return undefined;
  return {
    uri: asset.uri,
    name: photoName(asset.fileName, asset.mimeType),
    ...(asset.mimeType === undefined ? {} : { mimeType: asset.mimeType }),
  };
}

/**
 * "Files": any one document, or undefined when cancelled. On iOS the system
 * already copies the file into the app's tmp Inbox, so the picker does not
 * copy it again into the unprotected cache before the size check; staging
 * reads that copy, and the copy is removed afterwards.
 */
export async function pickFile(): Promise<PickedFile | undefined> {
  const result = await getDocumentAsync({
    type: "*/*",
    copyToCacheDirectory: Platform.OS !== "ios",
    multiple: false,
  });
  const asset = result.canceled ? undefined : result.assets[0];
  if (asset === undefined) return undefined;
  return {
    uri: asset.uri,
    name: asset.name,
    ...(asset.mimeType === undefined ? {} : { mimeType: asset.mimeType }),
  };
}

/** Copies a verified file to `<exports>/<uuid>/<name>` for Open or Save. */
export async function exportCopy(
  path: string,
  filename: string,
  exportsDirectory: string,
): Promise<string> {
  const folder = joinPath(exportsDirectory, randomUuid());
  ensureDirectory(folder);
  const target = joinPath(folder, safeFileName(filename));
  try {
    await new File(pathToUri(path)).copy(new File(pathToUri(target)));
    return target;
  } catch (error) {
    removePath(folder);
    throw error;
  }
}

/** Removes an export copy's UUID folder. */
export function removeExport(path: string): void {
  const folder = path.slice(0, path.lastIndexOf("/"));
  const name = folder.slice(folder.lastIndexOf("/") + 1);
  if (UUID.test(name)) removePath(folder);
}

/** Quick Look (iOS) or the system viewer (Android). Resolves once shown. */
export async function previewFile(
  path: string,
  mimeType?: string,
): Promise<void> {
  await new File(pathToUri(path)).preview(
    mimeType === undefined ? undefined : { mimeType },
  );
}

/** Save to Files. Resolves false when the user cancelled. */
export async function saveFile(path: string, name: string): Promise<boolean> {
  try {
    await saveDocuments({
      sourceUris: [pathToUri(path)],
      copy: true,
      fileName: name,
    });
    return true;
  } catch (error) {
    if (
      isErrorWithCode(error) &&
      error.code === errorCodes.OPERATION_CANCELED
    ) {
      return false;
    }
    throw error;
  }
}
