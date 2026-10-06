// The Node target of the public layer: the host Client owns the native binding
// Client, the custom codec registry, and listener gates. The private binding
// root initializes the native binding when it loads.
import "../../binding";
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

import { ClientRegistry, type Client } from "../client";

export function streamOwner(source: {
  sdkStreamOwnerKey(): bigint;
}): Client | undefined {
  return ClientRegistry.get(source.sdkStreamOwnerKey());
}
