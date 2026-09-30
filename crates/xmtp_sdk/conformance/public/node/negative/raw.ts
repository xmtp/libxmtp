import { Client, type PublicIdentity, bindingClient } from "xmtp-sdk";
import { Client as BindingClient } from "xmtp-sdk/xmtp_sdk";

// Apps must use the public Client. The binding Client, its accessor, and the
// identity routes stay private to the package.
export async function consumeRaw(
  client: Client,
  identity: PublicIdentity,
): Promise<void> {
  // The unions replace the identity routes.
  await client.conversations.createGroupWithIdentities([identity]);
  void client.raw;
  void bindingClient;
  void BindingClient;
}
