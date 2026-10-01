import { realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  exercise,
  type Signer,
} from "../../../../target/sdk-codec-author/node/entry.ts";
import { verifyWirePolicy } from "./wire-policy.test.mjs";

const viem = realpathSync(
  fileURLToPath(
    new URL("../../../../sdks/node/node_modules/viem", import.meta.url),
  ),
);
const { generatePrivateKey, privateKeyToAccount } = await import(
  pathToFileURL(join(viem, "_esm/accounts/index.js")).href
);
const { toBytes } = await import(
  pathToFileURL(join(viem, "_esm/index.js")).href
);
function signer(): Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    identity: async () => ({
      kind: "ethereum",
      identifier: account.address.toLowerCase(),
    }),
    kind: async () => ({ kind: "eoa" }),
    sign: async (request) => ({
      kind: "ecdsa",
      value: Uint8Array.from(
        toBytes(await account.signMessage({ message: request.text })),
      ),
    }),
  };
}
const backend = process.env.XMTP_BACKEND_URL;
if (!backend) throw new Error("XMTP_BACKEND_URL is required");
const results = await exercise([signer(), signer(), signer()], backend);
verifyWirePolicy(results);
console.log(
  `Node independent codec author: ${results.checks.join("; ")}; wire push flags and no default compression`,
);
