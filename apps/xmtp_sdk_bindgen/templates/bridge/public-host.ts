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
import { Message as BoundMessage } from "../../host-message.gen.js";
import { XmtpError } from "../../public-values.gen.js";
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

import { owner } from "../../host-message.gen.js";
import { wrapClient } from "../../public-client.gen.js";
import { RemoteObject, sessionOf } from "../bridge/main/remote-object.js";

export function streamOwner(
  source: object,
  ownerKey: () => bigint,
): import("../../public-client.gen.js").Client | undefined {
  if (!(source instanceof RemoteObject))
    throw new TypeError("not an XMTP receiver");
  const client = owner(sessionOf(source), ownerKey())?.client.deref();
  return client === undefined ? undefined : wrapClient(client);
}
