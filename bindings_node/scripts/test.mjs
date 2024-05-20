import { initEcdsaClient, syncGroups, checkCanMessage } from "./utils.mjs";
import { addresses, users } from "./users.mjs";

const client1 = await initEcdsaClient(users[0].wallet);
await checkCanMessage(client1, addresses);
const groups = await syncGroups(client1);

// const group = await createGroup(
//   client1,
//   [wallets[1], wallets[2]],
//   "test stream group"
// );

// await sendGroupMessage(groups[0], "test3");
// await listGroupMessages(groups[0]);
