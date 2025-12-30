import { expect, test } from "vitest";
import init, {
  AuthHandle,
  Conversation,
  createAuthTestClient,
  createTestClient,
} from "../";

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

  let groups: string[] = [];
  let reader = stream.getReader();
  let i = 0;
  while (i < 3) {
    const { value } = await reader.read();
    groups.push(value.id());
    i++;
  }
  expect(groups.length).toBe(3);
  expect(groups.includes(g.id())).toBe(true);
  expect(groups.includes(bo_g.id())).toBe(true);
  expect(groups.includes(caro_g.id())).toBe(true);
});

test("streams groups", async () => {
  const groups: Conversation[] = [];
  const streamCallback = async (conversation: Conversation) => {
    groups.push(conversation);
  };
  const alix = await createTestClient();
  const bo = await createTestClient();
  const stream = alix
    .conversations()
    .stream({ on_conversation: streamCallback });
  const g = await alix.conversations().createGroupByInboxIds([bo.inboxId]);
  while (groups.length == 0) {
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  expect(groups[0].id()).toBe(g.id());
  await stream.endAndWait();
});

test("auth callback", async () => {
  const handle = new AuthHandle();
  console.log("creating client");
  let called = false;
  await createAuthTestClient(
    {
      on_auth_required: async () => {
        console.log("on_auth_required js");
        called = true;
        return {
          value: "Bearer 1234567890",
          expiresAtSeconds: BigInt(Date.now() + 1000),
        };
      },
    },
    handle,
  );
  expect(called).toBe(true);
});

test("auth callback throws error", async () => {
  const handle = new AuthHandle();
  console.log("creating client");
  let called = false;
  await expect(
    createAuthTestClient(
      {
        on_auth_required: async () => {
          console.log("on_auth_required js");
          called = true;
          throw new Error("on_auth_required js error");
        },
      },

      handle,
    ),
  ).rejects.toThrow("Auth callback failed");
  expect(called).toBe(true);
});
