import { expect, test } from "vitest";
import init, {
  Client,
  Conversation,
  createTestClient,
} from "@xmtp/wasm-bindings";

await init();

test("adds 1 + 2 to equal 3", () => {
  expect(1 + 2).toBe(3);
});

// Client test with ephemeral DB are possible
// OPFS tests not possible since that requires web worker

test("streams groups local", async () => {
  const alix = await createTestClient();
  const bo = await createTestClient();
  const stream = await alix.conversations().streamLocal();
  const g = await alix.conversations().createGroupByInboxIds([bo.inboxId]);
  let reader = stream.getReader();
  let { done, value } = await reader.read();
  let group_id = value.id();
  expect(group_id).toBe(g.id());
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
