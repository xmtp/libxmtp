import { Client } from "../../dist/typescript-wasm/xmtp_sdk.js";
import "../../dist/typescript-wasm/worker-entry.gen.js";

const nativeEnd = Client.prototype.end;
let failNextEnd = true;
Client.prototype.end = async function (
  this: Client,
  ...args: Parameters<Client["end"]>
) {
  await nativeEnd.apply(this, args);
  // Core is closed, but the worker must retain its handles for cleanup retry.
  if (failNextEnd) {
    failNextEnd = false;
    throw new Error("post-shutdown failure");
  }
};
