import { Client, wrapClient } from "xmtp-sdk-browser";
import { MainSession } from "xmtp-sdk-browser/typescript-wasm/runtime/bridge/main/session";

// Apps use the package Client. The worker proxy, its session, and the proxy
// wrapper stay private to the package.
export function consumeRaw(client: Client): void {
  void client.raw;
  void wrapClient;
  void MainSession;
}
