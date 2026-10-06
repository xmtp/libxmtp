// The SDK calls of the Node and Chromium hosts. Every call uses a public root.
export function publicApi(sdk, pure, target, backend, accounts) {
  const bytes = (hex) =>
    Uint8Array.from(hex.match(/../g).map((x) => Number.parseInt(x, 16)));
  const hex = (value) =>
    Array.from(value, (x) => x.toString(16).padStart(2, "0")).join("");
  const type = (name) => ({
    authorityId: "xmtp.org",
    typeId: name,
    versionMajor: 1,
    versionMinor: 0,
  });
  // A real ECDSA signer with a generated test key.
  function signer(key) {
    const account = accounts.privateKeyToAccount(`0x${key}`);
    const sign = async (text) =>
      bytes((await account.signMessage({ message: text })).slice(2));
    return {
      identity: async () => ({
        kind: "ethereum",
        identifier: account.address.toLowerCase(),
      }),
      kind: async () => ({ kind: "eoa" }),
      sign: async (request) => ({
        kind: "ecdsa",
        value: await sign(request.text),
      }),
    };
  }
  function options(path) {
    return {
      backend: { url: backend },
      deviceSync: false,
      storage: {
        location: { dbPath: path, attachmentsDir: `${path}-attachments` },
        ...(target === "browser"
          ? {}
          : { encryptionKey: new Uint8Array(32).fill(7) }),
      },
    };
  }
  const open = async (key, path, inboxId) =>
    sdk.Client.build(await signer(key).identity(), options(path), inboxId);
  async function prepare(group, row, ids, inboxId) {
    let value;
    if (row.reply_to !== null)
      value = {
        kind: "reply",
        reference: ids[Number(row.reply_to)],
        referenceInboxId: inboxId,
        content: pure.encodeStandard({ kind: "text", value: row.text }),
      };
    else if (row.attachment)
      value = {
        kind: "attachment",
        value: {
          filename: row.attachment.filename,
          mimeType: row.attachment.mime_type,
          content: bytes(row.attachment.bytes_hex),
        },
      };
    else value = { kind: "text", value: row.text };
    return group.prepareMessage(pure.encodeStandard(value));
  }
  async function prepareReaction(group, reference, inboxId, reaction) {
    return group.prepareMessage(
      pure.encodeStandard({
        kind: "reaction",
        reference,
        referenceInboxId: inboxId,
        reaction,
      }),
    );
  }
  function normalize(message, keyById) {
    const key = keyById.get(message.id);
    if (key === undefined) throw new Error(`Unexpected message ${message.id}`);
    const content = message.content;
    let text = null;
    let attachment = null;
    let reply = null;
    let parentText = null;
    if (content.kind === "text") text = content.value;
    else if (content.kind === "attachment") {
      attachment = {
        filename: content.value.filename,
        mime_type: content.value.mimeType,
        bytes_hex: hex(content.value.content),
      };
    } else if (content.kind === "reply") {
      reply = keyById.get(content.referenceId);
      if (reply === undefined) throw new Error("Reply parent ID is absent");
      if (
        content.body.kind !== "text" ||
        message.inReplyToContent?.kind !== "text"
      ) {
        throw new Error("Reply body or eager parent was not decoded");
      }
      text = content.body.value;
      parentText = message.inReplyToContent.value;
    } else throw new Error(`Unexpected content type ${content.kind}`);
    const reactions = message.reactions.map((entry) => {
      if (!entry.reaction) throw new Error("Reaction was not decoded");
      return {
        content: entry.reaction.content,
        action: entry.reaction.action,
        schema: entry.reaction.schema,
      };
    });
    return {
      key,
      text,
      reply_to: reply,
      parent_text: parentText,
      attachment,
      reactions,
    };
  }
  return {
    newKey: () => accounts.generatePrivateKey().slice(2),
    create: (key, path) => sdk.Client.create(signer(key), options(path)),
    open,
    inbox: (client) => client.inboxId,
    close: (client) => client.end(),
    group: async (client, id) => {
      const group = await client.conversations.getById(id);
      if (!group) throw new Error("Seeded group is absent");
      return group;
    },
    createGroup: (client, members) => client.conversations.createGroup(members),
    syncConversations: (client) => client.conversations.sync(),
    groupId: (group) => group.id,
    prepare,
    prepareReaction,
    publish: (group) => group.publishMessages(),
    page: (group, count) =>
      group.messages({
        limit: count,
        direction: "ascending",
        contentTypes: [type("text"), type("reply"), type("attachment")],
      }),
    stream: async (client, group) => {
      const stream = group.streamMessages();
      await stream.ready();
      return stream;
    },
    normalize,
  };
}
