// The Node target of the public layer: the host Client owns the native binding
// Client, the custom codec registry, and listener gates. The private binding
// root initializes the native binding when it loads.
import "../../binding";
import type { Dirent } from "node:fs";
import { readdir, stat } from "node:fs/promises";
import { join } from "node:path";

import {
  XmtpError,
  generateInboxId,
  inboxIdForWithBackend,
  type ClientOptions,
  type InboxId,
  type PublicIdentity,
} from "../../public-values.gen";
import type { Message as RuntimeMessage } from "../message";

export {
  Client as HostClient,
  bindingClient,
  type SDKClientOptions as HostClientOptions,
} from "../client";
export type { Message as BoundMessage } from "../message";
export { encodeText } from "../../xmtp_sdk";
// The one catalogue predicate for the send push default (Decision 24).
export { isCatalogueContentType } from "../../public-values.gen";

/** On Node, the binding message is the host Message. */
export function boundMessageOf(value: RuntimeMessage): RuntimeMessage {
  return value;
}

/** Node storage accepts every public storage option. */
export function checkStorage(_storage: object): void {}

function ambiguousStorage(message: string): XmtpError {
  return new XmtpError.StorageLocation({
    code: "StorageLocation",
    category: "storage",
    retryable: false,
    message,
  });
}

async function fileExists(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
}

/** Reuse one old Node database when the caller asks for default storage. */
export async function resolveLegacyStorage(
  options: ClientOptions,
  identity: () => Promise<PublicIdentity>,
  inboxId?: InboxId,
): Promise<ClientOptions> {
  if (options.storage.location !== "default") return options;
  const label = options.storage.label || undefined;
  if (
    label &&
    (label === "." ||
      label === ".." ||
      /[\\/:\0]/.test(label) ||
      (process.platform === "win32" && /[. ]$/.test(label)))
  )
    return options;

  const directory = process.cwd();
  const files = (await readdir(directory, { withFileTypes: true })).flatMap(
    (entry) => {
      if (!entry.isFile()) return [];
      const match = /^xmtp-(.+)-([0-9a-f]{64})\.db3$/i.exec(entry.name);
      return match === null
        ? []
        : [
            {
              name: entry.name,
              environment: match[1],
              inboxId: match[2].toLowerCase(),
            },
          ];
    },
  );
  if (files.length === 0) return options;

  let user: PublicIdentity;
  try {
    user = await identity();
  } catch {
    throw new XmtpError.InvalidArgument({
      code: "InvalidArgument",
      category: "input",
      retryable: false,
      message: "Signer identity could not be read.",
    });
  }
  const ids = new Set<InboxId>([
    inboxId ?? generateInboxId(user, options.registration?.nonce),
  ]);
  if (inboxId === undefined && !options.allowOffline)
    ids.add(await inboxIdForWithBackend(options.backend ?? { url: "" }, user));
  const matches = files.filter(
    (entry) =>
      ids.has(entry.inboxId) &&
      (label === undefined || entry.environment === label),
  );
  if (matches.length === 0) return options;
  if (matches.length > 1)
    throw ambiguousStorage(
      "More than one legacy XMTP database matches this inbox. Set an explicit storage location.",
    );

  const match = matches[0];
  const root =
    label === undefined
      ? join(directory, "xmtp")
      : join(directory, "xmtp", label);
  let deployments: Dirent[];
  try {
    deployments = await readdir(root, { withFileTypes: true });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    deployments = [];
  }
  for (const deployment of deployments) {
    if (
      deployment.isDirectory() &&
      (await fileExists(join(root, deployment.name, match.inboxId, "xmtp.db3")))
    )
      throw ambiguousStorage(
        "Both legacy and current XMTP databases match this inbox. Set an explicit storage location.",
      );
  }

  const dbPath = join(directory, match.name);
  return {
    ...options,
    storage: {
      ...options.storage,
      location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
    },
  };
}
