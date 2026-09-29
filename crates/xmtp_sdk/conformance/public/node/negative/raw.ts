import { Client, bindingClient } from "xmtp-sdk";
import { Client as BindingClient } from "xmtp-sdk/xmtp_sdk";

// Apps must use the host Client. The binding Client and its accessor stay
// private to the package.
export function consumeRaw(client: Client): void {
  void client.raw;
  void bindingClient;
  void BindingClient;
}
