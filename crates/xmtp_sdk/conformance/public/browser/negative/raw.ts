import { Client, type PublicIdentity, wrapClient } from "xmtp-sdk-browser";
import { MainSession } from "xmtp-sdk-browser/typescript-wasm/runtime/bridge/main/session";

// Apps use the package Client. The worker proxy, its session, and the proxy
// wrapper stay private to the package.
export async function consumeRaw(
  client: Client,
  identity: PublicIdentity,
): Promise<void> {
  // The identity routes are private; the unions replace them.
  await client.conversations().createGroupWithIdentities([identity], undefined);
  void client.raw;
  void wrapClient;
  void MainSession;
}
