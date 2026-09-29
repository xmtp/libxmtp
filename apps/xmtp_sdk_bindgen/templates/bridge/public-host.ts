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
import { Message as BoundMessage } from "../../host-message.gen.js";
import type { Message as RuntimeMessage } from "../message.js";

/** A binding message from a worker proxy is always the host Message. */
export function boundMessageOf(value: RuntimeMessage): BoundMessage {
  if (!(value instanceof BoundMessage))
    throw new TypeError("not an XMTP host Message");
  return value;
}
