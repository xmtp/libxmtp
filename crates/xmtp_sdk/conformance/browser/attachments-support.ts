// Helpers for the browser attachment scenarios: OPFS files, attachment events,
// and attachment failures. Browser attachment paths name OPFS entries.
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { equal, expect } from "./suite-support";

export const ATTACHMENT_KINDS: sdk.EventKind[] = [
  "attachment.upload_started",
  "attachment.upload_completed",
  "attachment.upload_failed",
  "attachment.download_started",
  "attachment.download_completed",
  "attachment.download_failed",
  "attachment.deleted",
];

export type AttachmentEvent = Extract<
  sdk.ClientEvent,
  { readonly attachment: unknown }
>;

/** Browser options for a client in an OPFS directory, allowed to reach loopback storage. */
export function fileOptions(
  backendURL: string,
  directory: string,
  attachments: sdk.AttachmentOptions = { allowPrivateNetwork: true },
): sdk.ClientOptions {
  return {
    backend: { url: backendURL },
    storage: { location: { directory }, singleConnection: false },
    deviceSync: false,
    allowOffline: false,
    registration: { auto: true },
    attachments,
  };
}

export function bytesSource(text: string): sdk.AttachmentSource {
  return {
    kind: "bytes",
    bytes: new TextEncoder().encode(text),
    filename: "note.txt",
    mimeType: "text/plain",
  };
}

export function failure(
  cause: sdk.AttachmentFailureCause,
  fields: Partial<sdk.AttachmentFailure> = {},
): sdk.AttachmentFailure {
  return {
    cause,
    credentialKind: undefined,
    retryable: false,
    missingScope: false,
    httpStatus: undefined,
    ...fields,
  };
}

/** Compare plain values: records, arrays, and bigints. */
export function same(
  actual: unknown,
  expected: unknown,
  message: string,
): void {
  const text = (value: unknown) =>
    JSON.stringify(value, (_key, field: unknown) =>
      typeof field === "bigint" ? `${field}n` : field,
    );
  equal(text(actual), text(expected), message);
}

export async function thrownError(
  action: Promise<unknown>,
): Promise<InstanceType<typeof sdk.XmtpError.Attachment>> {
  try {
    await action;
  } catch (error) {
    expect(
      error instanceof sdk.XmtpError.Attachment,
      `expected an attachment error, got ${String(error)}`,
    );
    equal(error.details.code, "Attachment", "attachment error code");
    return error;
  }
  throw new Error("expected an attachment error, got success");
}

export async function thrownFailure(
  action: Promise<unknown>,
): Promise<sdk.AttachmentFailure> {
  return (await thrownError(action)).attachmentFailure;
}

export async function rejectsClosed(
  action: Promise<unknown>,
  label: string,
): Promise<void> {
  try {
    await within(action, label);
  } catch (error) {
    if (error instanceof sdk.XmtpError.ClientClosed) return;
    throw error;
  }
  throw new Error(`${label} did not fail with ClientClosed`);
}

export function attachmentFilter(
  kinds: sdk.EventKind[] = ATTACHMENT_KINDS,
): sdk.EventFilter {
  return {
    kinds: [...kinds, "conversation.joined"],
    referencesOwnMessages: false,
  };
}

export async function within<T>(
  promise: Promise<T>,
  what: string,
  milliseconds = 10_000,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${what} timed out`)),
      milliseconds,
    );
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/** The members `drain` uses, from the shipped build or the fixture. */
type GroupCreator = {
  readonly conversations: {
    createGroup(members: []): Promise<{ readonly id: string }>;
  };
};
type EventSource = { next(): Promise<IteratorResult<sdk.ClientEvent>> };

/** Read attachment events up to the group this creates, which marks the end. */
export async function drain(
  client: GroupCreator,
  stream: EventSource,
): Promise<AttachmentEvent[]> {
  const marker = (await client.conversations.createGroup([])).id;
  const events: AttachmentEvent[] = [];
  for (;;) {
    const next = await within(stream.next(), "attachment events");
    expect(!next.done, "the event stream ended");
    const event = next.value;
    if (event.kind === "conversation.joined") {
      if (event.conversationId === marker) return events;
      continue;
    }
    expect("attachment" in event, `unexpected ${event.kind} event`);
    events.push(event);
  }
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", Uint8Array.from(bytes));
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

/** The directory core names for a deployment with a file-safe identifier. */
export async function deploymentComponent(identifier: string): Promise<string> {
  expect(
    /^[\x20-\x7e]{1,190}$/.test(identifier) &&
      !/[<>:"|?*/\\]/.test(identifier) &&
      !/^[. ]|[. ]$/.test(identifier),
    `the expected deployment directory assumes a file-safe identifier: ${identifier}`,
  );
  return `${identifier.toLowerCase()}-${await sha256Hex(new TextEncoder().encode(identifier))}`;
}

function segments(path: string): string[] {
  const names = path.split("/").filter((name) => name !== "");
  expect(names.length > 0, `empty OPFS path ${path}`);
  return names;
}

async function opfsDirectory(
  names: string[],
  create: boolean,
): Promise<FileSystemDirectoryHandle> {
  let directory = await navigator.storage.getDirectory();
  for (const name of names)
    directory = await directory.getDirectoryHandle(name, { create });
  return directory;
}

async function opfsFile(
  path: string,
  create: boolean,
): Promise<FileSystemFileHandle> {
  const names = segments(path);
  const name = names.pop()!;
  return (await opfsDirectory(names, create)).getFileHandle(name, { create });
}

export async function readOpfs(path: string): Promise<Uint8Array> {
  const file = await (await opfsFile(path, false)).getFile();
  return new Uint8Array(await file.arrayBuffer());
}

export async function readOpfsText(path: string): Promise<string> {
  return new TextDecoder().decode(await readOpfs(path));
}

export async function writeOpfs(path: string, text: string): Promise<void> {
  const writable = await (await opfsFile(path, true)).createWritable();
  await writable.write(text);
  await writable.close();
}

export async function removeOpfs(path: string): Promise<void> {
  const names = segments(path);
  const name = names.pop()!;
  await (
    await opfsDirectory(names, false)
  ).removeEntry(name, {
    recursive: true,
  });
}

export async function existsOpfs(
  path: string,
  kind: "file" | "directory" = "file",
): Promise<boolean> {
  try {
    if (kind === "file") await opfsFile(path, false);
    else await opfsDirectory(segments(path), false);
    return true;
  } catch (error) {
    if (error instanceof DOMException && error.name === "NotFoundError")
      return false;
    throw error;
  }
}

/** The attachments directory beside a client's database. */
export function attachmentsDir(databasePath: string | undefined): string {
  expect(databasePath !== undefined, "file-backed client has no path");
  return `${databasePath.slice(0, databasePath.lastIndexOf("/"))}/attachments`;
}

/**
 * A relay on the loopback object store. It forwards to the backend until
 * `refuse`, then answers 503 and counts the requests it refused.
 */
export function relay(store: string): {
  url: string;
  refuse(): Promise<void>;
  requests(): Promise<number>;
} {
  const id = crypto.randomUUID();
  return {
    url: `${store}/relay/${id}`,
    async refuse() {
      equal((await fetch(`${store}/refuse/${id}`)).status, 200, "refuse");
    },
    async requests() {
      return Number(await (await fetch(`${store}/count/${id}`)).text());
    },
  };
}

/** Store bytes on the loopback object store and return their URL. */
export async function servedObject(
  store: string,
  body: Uint8Array,
): Promise<string> {
  const url = `${store}/fixtures/${crypto.randomUUID()}`;
  const response = await fetch(url, {
    method: "PUT",
    body: Uint8Array.from(body),
  });
  equal(response.status, 200, "the object store did not take the fixture");
  return url;
}
