import { sdkLogSinkHandoff } from "xmtp-sdk-browser";
import { Client, type PublicIdentity, wrapClient } from "xmtp-sdk-browser";
import { MainSession } from "xmtp-sdk-browser/typescript-wasm/runtime/bridge/main/session";

// Apps use the public Client. The worker proxy, its session, and the proxy
// wrapper stay private to the package.
export async function consumeRaw(
  client: Client,
  identity: PublicIdentity,
): Promise<void> {
  // The unions replace the identity routes.
  await client.conversations.createGroupWithIdentities([identity]);
  void client.raw;
  void wrapClient;
  void MainSession;
}

void sdkLogSinkHandoff;
