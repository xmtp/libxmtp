import { test, expect } from "bun:test";
import {
  createRegisteredClient,
  createUser,
  encodeTextMessage,
} from "./helpers";
import { createOldRegisteredClient } from "./oldHelpers";
import { GroupMessageKind, PublicIdentifierKind } from "current-bindings";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { generateInboxId as generateInboxIdOld } from "legacy-bindings";
import { generateInboxId } from "current-bindings";

test("Can communicate in a DM", async () => {
  // Create test users
  const user1 = createUser();
  const user2 = createUser();

  // Create clients for each user
  const client1 = await createRegisteredClient(user1);
  const client2 = await createOldRegisteredClient(user2);

  // User1 creates DM and sends a message
  const dm1 = await client1.conversations().createDm({
    identifier: client2.accountAddress,
    identifierKind: PublicIdentifierKind.Ethereum,
  });
  await dm1.send(encodeTextMessage("hey"));

  // User2 syncs, creates DM, and replies
  await client2.conversations().syncAllConversations();
  const dm2 = await client2.conversations().createDm(user1.account.address);
  await dm2.send(encodeTextMessage("ho"));

  // User1 syncs to receive User2's message
  await client1.conversations().syncAllConversations();

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

// This test will go away when the legacy bindings are updated
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
