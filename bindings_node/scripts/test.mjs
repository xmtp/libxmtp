import {
  initEcdsaClient,
  syncGroups,
  checkCanMessage,
  createGroup,
  sendGroupMessage,
  listGroupMessages,
} from "./utils.mjs";
import { addresses, users } from "./users.mjs";

const client1 = await initEcdsaClient(users[0].wallet);
await checkCanMessage(client1, addresses);
const groups = await syncGroups(client1);

await createGroup(client1, [users[1], users[2]], "test stream group");

await sendGroupMessage(groups[0], "test");
await listGroupMessages(groups[0]);
