import { expect, test } from "vitest";
import init, {
  Client,
  Conversation,
  createTestClient,
} from "@xmtp/wasm-bindings";

await init();

// Client test with ephemeral DB are possible
// OPFS tests not possible since that requires web worker
test("streams groups local", async () => {
  const alix = await createTestClient();
  const bo = await createTestClient();
  const caro = await createTestClient();
  const stream = await alix.conversations().streamLocal();
  const g = await alix.conversations().createGroupByInboxIds([bo.inboxId]);
  const bo_g = await bo
    .conversations()
    .createGroupByInboxIds([alix.inboxId, caro.inboxId]);
  const caro_g = await caro
    .conversations()
    .createGroupByInboxIds([alix.inboxId, bo.inboxId]);

  let groups = new Array();
  let reader = stream.getReader();
  let i = 0;
  while (i < 3) {
    var { done, value } = await reader.read();
    groups.push(value.id());
    i++;
  }
  expect(groups.length).toBe(3);
  expect(groups.includes(g.id())).toBe(true);
  expect(groups.includes(bo_g.id())).toBe(true);
  expect(groups.includes(caro_g.id())).toBe(true);
});

test("streams groups", async () => {
  let groups = new Array();
  const streamCallback = async (conversation) => {
    groups.push(conversation);
  };
  const alix = await createTestClient();
  const bo = await createTestClient();
  const stream = await alix
    .conversations()
    .stream({ on_conversation: streamCallback });
  const g = await alix.conversations().createGroupByInboxIds([bo.inboxId]);
  while (groups.length == 0) {
    await new Promise((r) => setTimeout(r, 100));
  }
  // let group_id = value.id();
  expect(groups[0].id()).toBe(g.id());
});
