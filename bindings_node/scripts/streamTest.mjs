import process from "node:process";
import { AsyncStream } from "./AsyncStream.mjs";
import { initEcdsaClient, syncGroups } from "./utils.mjs";
import { wallets } from "./users.mjs";

const client1 = await initEcdsaClient(wallets[0]);
await syncGroups(client1);

console.log("creating group stream...");
const stream = new AsyncStream();
const groupStream = client1.conversations().stream(stream.callback);

["SIGINT", "SIGTERM", "SIGQUIT"].forEach((signal) =>
  process.on(signal, () => {
    console.log("stopping stream...", signal);
    groupStream.end();
    stream.stop();
  })
);

console.log("waiting for new groups to be created...");

for await (const v of stream) {
  console.log("new streaming value", v.id());
}
