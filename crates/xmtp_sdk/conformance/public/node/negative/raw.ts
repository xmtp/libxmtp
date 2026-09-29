import { Client, type PublicIdentity, bindingClient } from "xmtp-sdk";
import { Client as BindingClient } from "xmtp-sdk/xmtp_sdk";

// Apps must use the host Client. The binding Client and its accessor stay
// private to the package.
export async function consumeRaw(
  client: Client,
  identity: PublicIdentity,
): Promise<void> {
  // The identity routes are private; the unions replace them.
  await client.conversations().createGroupWithIdentities([identity], undefined);
  void client.raw;
  void bindingClient;
  void BindingClient;
}
