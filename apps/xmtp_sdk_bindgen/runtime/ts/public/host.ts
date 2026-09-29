// The Node target of the public layer: the host Client owns the native binding
// Client, the custom codec registry, and listener gates. The package index
// initializes the native binding when it loads.
import "../../index";
import type { Message as RuntimeMessage } from "../message";

export {
  Client as HostClient,
  bindingClient,
  type SDKClientOptions as HostClientOptions,
} from "../client";
export type { Message as BoundMessage } from "../message";
export { encodeText } from "../../xmtp_sdk";

/** On Node, the binding message is the host Message. */
export function boundMessageOf(value: RuntimeMessage): RuntimeMessage {
  return value;
}
