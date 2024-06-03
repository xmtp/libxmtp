import { wallets } from "./users.mjs";
import { initEcdsaClient } from "./utils.mjs";

wallets.forEach(async (wallet) => {
  try {
    await initEcdsaClient(wallet);
  } catch (e) {
    console.error(e);
  }
});
