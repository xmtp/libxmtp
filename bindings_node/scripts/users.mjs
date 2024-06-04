import { readFile } from "node:fs/promises";
import { privateKeyToAccount } from "viem/accounts";
import { sepolia } from "viem/chains";
import { createWalletClient, http } from "viem";

const json = JSON.parse(
  await readFile(new URL("./users.json", import.meta.url))
);

const users = json.users.map((user) => {
  const account = privateKeyToAccount(user.key);
  return {
    ...user,
    account,
    wallet: createWalletClient({
      account,
      chain: sepolia,
      transport: http(),
    }),
  };
});

const addresses = users.map((user) => user.account.address);
const wallets = users.map((user) => user.wallet);

export { users, addresses, wallets };
