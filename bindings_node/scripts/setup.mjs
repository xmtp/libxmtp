import { wallets, addresses } from "./users.mjs";
import { initEcdsaClient, checkCanMessage } from "./utils.mjs";

wallets.forEach(async (wallet) => {
  try {
    await initEcdsaClient(wallet);
  } catch (e) {
    console.error(e);
  }
});
