import { test, expect } from "bun:test";
import {
  createRegisteredClient,
  createUser,
  decodeGroupMessages,
  encodeTextMessage,
  sleep,
} from "./helpers";
import { createOldRegisteredClient } from "./oldHelpers";
import {
  Client,
  GroupMessageKind,
  PublicIdentifierKind,
} from "current-bindings";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { generateInboxId as generateInboxIdOld } from "legacy-bindings";
import { generateInboxId } from "current-bindings";

test("Can communicate in a Group", async () => {
  const newUser = createUser();
  const oldUser = createUser();

  let newClient = await createRegisteredClient(newUser);
  let oldClient = await createOldRegisteredClient(oldUser);

  let newGroupId = (
    await newClient.conversations().createGroupByInboxId([oldClient.inboxId()])
  ).id();

  let oldGroupId = (
    await oldClient.conversations().createGroupByInboxId([newClient.inboxId()])
  ).id();

  let i = 1;
  let clients = [newClient, oldClient];
  // Send 4 messages in each group
  for (let client of clients) {
    await client.conversations().syncAllConversations();
    let newGroup = client.conversations().findGroupById(newGroupId);
    await newGroup.send(encodeTextMessage(`hi ${i}`));

    let oldGroup = client.conversations().findGroupById(oldGroupId);
    await oldGroup.send(encodeTextMessage(`hi ${i}`));

    await sleep(500);

    i += 1;
  }

  i = 1;
  for (let client of clients) {
    // await client.conversations().syncAllConversations();
    let newGroup = client.conversations().findGroupById(newGroupId);
    await newGroup.send(encodeTextMessage(`hi ${i}`));

    let oldGroup = client.conversations().findGroupById(oldGroupId);
    await oldGroup.send(encodeTextMessage(`hi ${i}`));
    await sleep(500);
    i = +1;
  }

  for (let client of clients) {
    let newGroup = client.conversations().findGroupById(newGroupId);
    let newGroupMsgs = await decodeGroupMessages(newGroup);
    expect(newGroupMsgs).toContain("hi 1");
    expect(newGroupMsgs).toContain("hi 2");

    let oldGroup = client.conversations().findGroupById(oldGroupId);
    let oldGroupMsgs = await decodeGroupMessages(oldGroup);
    expect(oldGroupMsgs).toContain("hi 1");
    expect(oldGroupMsgs).toContain("hi 2");
  }
});

test("Can communicate in a DM", async () => {
  // Create test users
  const newUser = createUser();
  const oldUser = createUser();

  // Create clients for each user
  const newClient = await createRegisteredClient(newUser);
  const oldClient = await createOldRegisteredClient(oldUser);

  // User1 creates DM and sends a message
  const dm1 = await newClient
    .conversations()
    .createDmByInboxId(oldClient.inboxId());
  await dm1.send(encodeTextMessage("hey"));

  // User2 syncs, creates DM, and replies
  await oldClient.conversations().syncAllConversations();
  const dm2 = await oldClient
    .conversations()
    .createDmByInboxId(newClient.inboxId());
  await dm2.send(encodeTextMessage("ho"));

  // User1 syncs to receive User2's message
  await newClient.conversations().syncAllConversations();

  // Helper function to extract message content
  const extractMessageContent = async (conversation) => {
    const messages = await conversation.findMessages();
    return messages
      .filter((msg) => msg.kind === GroupMessageKind.Application)
      .map((msg) => new TextDecoder().decode(msg.content.content));
  };

  // Verify messages are correctly exchanged in both clients
  const msgsContent = await extractMessageContent(dm1);
  expect(msgsContent).toEqual(["hey", "ho"]);

  const msgs2Content = await extractMessageContent(dm2);
  expect(msgs2Content).toEqual(["hey", "ho"]);
});

// This test will probably go away when the legacy bindings are updated.
// We just want to ensure that inboxId generation has not changed during the identity migration.
test("Produces the same inboxId", async () => {
  let key = generatePrivateKey();
  let account = privateKeyToAccount(key);

  let inboxId = generateInboxIdOld(account.address);
  let inboxId2 = generateInboxId({
    identifier: account.address,
    identifierKind: PublicIdentifierKind.Ethereum,
  });

  expect(inboxId).toEqual(inboxId2);
});
