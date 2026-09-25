import { realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = realpathSync(
  fileURLToPath(
    new URL("../../../../sdks/node/node_modules/viem", import.meta.url),
  ),
);
const { privateKeyToAccount } = await import(
  pathToFileURL(join(root, "_esm/accounts/index.js")).href
);
const account = privateKeyToAccount(`0x${process.env.SDK_SIGN_KEY}`);
if (process.argv[2] === "identity") {
  console.log(account.address.toLowerCase());
} else if (process.argv[2] === "sign") {
  console.log(await account.signMessage({ message: process.argv[3] }));
} else {
  throw new Error("expected identity or sign");
}
