// The Node target of the public layer: the host Client owns the native binding
// Client, the custom codec registry, and listener gates. The package index
// initializes the native binding when it loads.
import "../../index";

export {
  Client as HostClient,
  bindingClient,
  type SDKClientOptions as HostClientOptions,
} from "../client";
export type { Message as BoundMessage } from "../message";
