// @ts-expect-error Use the repository's published viem test dependency.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-expect-error Use the repository's published viem test dependency.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-expect-error Use the repository's published viem test dependency.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  exercise,
  type Signer,
} from "../../../../target/sdk-codec-author/browser/entry.ts";

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
export async function run(): Promise<Awaited<ReturnType<typeof exercise>>> {
  return exercise([signer(), signer(), signer()], `${location.origin}/backend`);
}
