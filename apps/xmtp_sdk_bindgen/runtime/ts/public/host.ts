// The Node target of the public layer: the host Client owns the native binding
// Client, the custom codec registry, and listener gates. The private binding
// root initializes the native binding when it loads.
import "../../binding";
import type { Dirent } from "node:fs";
import { readdir, stat } from "node:fs/promises";
import { join } from "node:path";

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

export type LegacyStorageMatch =
  | { readonly kind: "none" }
  | {
      readonly kind: "location";
      readonly location: {
        readonly dbPath: string;
        readonly attachmentsDir: string;
      };
    }
  | { readonly kind: "ambiguous"; readonly message: string };

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
  inboxIds: () => Promise<readonly string[]>,
): Promise<LegacyStorageMatch> {
  const directory = process.cwd();
  const files = (await readdir(directory, { withFileTypes: true })).flatMap(
    (entry) => {
      if (!entry.isFile()) return [];
      const match = /^xmtp-(.+)-([0-9a-f]{64})\.db3$/i.exec(entry.name);
      return match === null
        ? []
        : [{ name: entry.name, inboxId: match[2].toLowerCase() }];
    },
  );
  if (files.length === 0) return { kind: "none" };

  const ids = new Set((await inboxIds()).map((id) => id.toLowerCase()));
  const matches = files.filter((entry) => ids.has(entry.inboxId));
  if (matches.length === 0) return { kind: "none" };
  if (matches.length > 1)
    return {
      kind: "ambiguous",
      message:
        "More than one legacy XMTP database matches this inbox. Set an explicit storage location.",
    };

  const match = matches[0];
  const root = join(directory, "xmtp");
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
      return {
        kind: "ambiguous",
        message:
          "Both legacy and current XMTP databases match this inbox. Set an explicit storage location.",
      };
  }

  const dbPath = join(directory, match.name);
  return {
    kind: "location",
    location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
  };
}
