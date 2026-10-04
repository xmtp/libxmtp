// The browser target of the public layer: the host Client owns the worker
// proxy of the binding Client, the custom codec registry, and listener gates.
// The package session starts the worker on the first create.
export {
  Client as HostClient,
  bindingClient,
} from "../../public-client.gen.js";
export type {
  HostClientOptions,
  Message as BoundMessage,
} from "../../host-message.gen.js";
// The main thread encodes text with the pure module, as the host Message does.
export { encodeText } from "../../../typescript-pure/xmtp_sdk.js";
// The one catalogue predicate for the send push default (Decision 24). It is
// a pure function, so the main thread calls the pure module.
export { isCatalogueContentType } from "../../../typescript-pure/public-values.gen.js";
import { generateInboxId } from "../../../typescript-pure/public-values.gen.js";
import { Message as BoundMessage } from "../../host-message.gen.js";
import {
  Storage,
  XmtpError,
  inboxIdForWithBackend,
  type ClientOptions,
  type InboxId,
  type PublicIdentity,
} from "../../public-values.gen.js";
import type { Message as RuntimeMessage } from "../message.js";

/** A binding message from a worker proxy is always the host Message. */
export function boundMessageOf(value: RuntimeMessage): BoundMessage {
  if (!(value instanceof BoundMessage))
    throw new TypeError("not an XMTP host Message");
  return value;
}

/**
 * Browser storage has no database encryption (SDK-037). A key is refused, not
 * dropped, so an app that sets one does not get an unencrypted database.
 */
export function checkStorage(storage: object): void {
  if (Reflect.get(storage, "encryptionKey") !== undefined)
    throw new XmtpError.InvalidInput({
      code: "InvalidInput",
      category: "input",
      retryable: false,
      message: "browser storage does not support encryptionKey",
    });
}

function ambiguousStorage(message: string): XmtpError {
  return new XmtpError.StorageLocation({
    code: "StorageLocation",
    category: "storage",
    retryable: false,
    message,
  });
}

/** Reuse one version 7 OPFS database when the caller asks for default storage. */
export async function resolveLegacyStorage(
  options: ClientOptions,
  identity: () => Promise<PublicIdentity>,
  inboxId?: InboxId,
): Promise<ClientOptions> {
  if (options.storage.location !== "default" || options.storage.label)
    return options;

  const admin = await Storage.admin();
  let paths: string[];
  try {
    paths = await admin.listFiles();
  } finally {
    await admin.end();
  }
  const files = paths.flatMap((path) => {
    const name = path.replace(/^\/+/, "");
    const match = /^xmtp-(.+)-([0-9a-f]{64})\.db3$/i.exec(name);
    return match === null
      ? []
      : [{ path: name, inboxId: match[2].toLowerCase() }];
  });
  if (files.length === 0) return options;

  const user = await identity();
  const legacyInboxId =
    options.registration?.nonce === undefined
      ? generateInboxId(user, 1n)
      : undefined;
  const ids = new Set<InboxId>([
    inboxId ?? generateInboxId(user, options.registration?.nonce),
  ]);
  if (inboxId === undefined && legacyInboxId !== undefined)
    ids.add(legacyInboxId);
  if (inboxId === undefined && !options.allowOffline)
    ids.add(await inboxIdForWithBackend(options.backend ?? { url: "" }, user));
  const matches = files.filter((entry) => ids.has(entry.inboxId));
  if (matches.length === 0) return options;
  if (matches.length > 1)
    throw ambiguousStorage(
      "More than one legacy XMTP database matches this inbox. Set an explicit storage location.",
    );

  const match = matches[0];
  if (
    paths.some((path) =>
      new RegExp(`^/?xmtp-sdk/[^/]+/${match.inboxId}/xmtp\\.db3$`, "i").test(
        path,
      ),
    )
  )
    throw ambiguousStorage(
      "Both legacy and current XMTP databases match this inbox. Set an explicit storage location.",
    );

  const dbPath = match.path;
  return {
    ...options,
    ...(options.registration?.nonce === undefined &&
    match.inboxId === legacyInboxId
      ? { registration: { ...options.registration, nonce: 1n } }
      : {}),
    storage: {
      ...options.storage,
      location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
    },
  };
}
