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

test("streams groups", async () => {
  const alix = await createTestClient();
  const bo = await createTestClient();
  const stream = await alix.conversations().streamLocal();
  const g = await alix.conversations().createGroupByInboxIds([bo.inboxId]);
  let reader = stream.getReader();
  let { done, value } = await reader.read();
  let group_id = value.id();
  expect(group_id).toBe(g.id());
});
