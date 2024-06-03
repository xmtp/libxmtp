import { toBytes } from "viem";
import { join } from "node:path";
import process from "node:process";
import { TextEncoder } from "node:util";
import { createClient } from "../dist/index.js";

export const initEcdsaClient = async (wallet) => {
  const dbPath = join(process.cwd(), `${wallet.account.address}.db3`);

  const client = await createClient(
    "http://localhost:5556",
    false,
    dbPath,
    wallet.account.address
  );

  console.log("client address", client.accountAddress);
  console.log("inbox ID", client.inboxId());
  console.log("installation ID", client.installationId());
  console.log("registered?", client.isRegistered());

  if (!client.isRegistered()) {
    const signatureText = client.signatureText();

    const signature = await wallet.signMessage({
      message: signatureText,
    });
    const sigBytes = toBytes(signature);

    try {
      console.log(`registering identity for ${wallet.account.address}...`);
      client.addEcdsaSignature(sigBytes);
      await client.registerIdentity();
    } catch (e) {
      console.error("failed to register identity", e);
    }
  }

  return client;
};

export const checkCanMessage = async (client, addresses) => {
  const canMessage = await client.canMessage(addresses);
  console.log("can message?", canMessage);
};

export const encodeTextMessage = (text) => {
  return {
    type: {
      authorityId: "xmtp.org",
      typeId: "text",
      versionMajor: 1,
      versionMinor: 0,
    },
    parameters: {
      encoding: "UTF-8",
    },
    content: new TextEncoder().encode(text),
  };
};

export const syncGroups = async (client) => {
  console.log("syncing groups...");
  await client.conversations().sync();

  console.log("fetching groups...");
  const groups = await client.conversations().list();
  console.log("group count", groups.length);

  if (groups.length) {
    const group = groups[0];

    console.log(`syncing group with id ${group.id()}`);
    await group.sync();

    console.log(
      `group "${group?.groupName()}"`,
      group?.listMembers()?.map((member) => member.inboxId)
    );

    console.log(`group created at`, group?.createdAtNs());
    console.log(`group creator`, group?.groupMetadata().creatorInboxId());
    console.log(
      `group conversation type`,
      group?.groupMetadata().conversationType()
    );

    console.log(`group is active?`, group?.isActive());

    console.log(`group addedByInboxId`, group?.addedByInboxId());
  }

  return groups;
};

export const createGroup = async (client, users, name) => {
  console.log("creating group");
  const newGroup = await client
    .conversations()
    .createGroup(users.map((user) => user.account.address));
  console.log(
    `group created with id "${newGroup.id()}" and name "${newGroup.groupName()}"`
  );

  if (name) {
    console.log(`updating group name to "${name}"`);
    await newGroup.updateGroupName(name);
    console.log(`updated group name to "${newGroup.groupName()}"`);
  }

  return newGroup;
};

export const sendGroupMessage = async (group, message) => {
  console.log("sending message to group...");
  const encoded = encodeTextMessage(message);
  console.log("encoded message", encoded);
  await group.send(encoded);
};

export const listGroupMessages = async (group) => {
  console.log("listing group messages...");
  const messages = group.findMessages();
  console.log("group messages", messages.length);

  if (messages.length) {
    messages.forEach((message, idx) => {
      const content = {
        ...message.content,
        content: new TextDecoder().decode(message.content.content),
      };
      console.log("==============================================");
      console.log(`message ${idx}`, message.id);
      console.log("delivery status", message.deliveryStatus);
      console.log("from", message.addrFrom);
      console.log("convoId", message.convoId);
      console.log("kind", message.kind);
      console.log("sentAtNs", message.sentAtNs);
      console.log("----------------------------------------------");
      console.log("content", content);
    });
  }
};
