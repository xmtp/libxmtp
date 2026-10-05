// Benchmark-only adapters. Every SDK operation uses an installed public root.
export function publicApi(sdk, pure, side, target, backend, accounts) {
  const modern = side === "new";
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
  function signer(key, delay = 0, called = () => {}) {
    const account = accounts.privateKeyToAccount(`0x${key}`);
    const sign = async (text) => {
      called(performance.now());
      if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
      return bytes((await account.signMessage({ message: text })).slice(2));
    };
    return modern
      ? {
          identity: async () => ({
            kind: "ethereum",
            identifier: account.address.toLowerCase(),
          }),
          kind: async () => ({ kind: "eoa" }),
          sign: async (request) => ({
            kind: "ecdsa",
            value: await sign(request.text),
          }),
        }
      : {
          type: "EOA",
          getIdentifier: () => ({
            identifier: account.address.toLowerCase(),
            identifierKind: sdk.IdentifierKind.Ethereum,
          }),
          signMessage: sign,
        };
  }
  function options(path) {
    if (modern)
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
    return {
      env: "local",
      apiUrl: backend,
      dbPath: path,
      dbEncryptionKey: new Uint8Array(32).fill(7),
      disableDeviceSync: true,
    };
  }
  const open = async (key, path, inboxId) => {
    const who = signer(key);
    return modern
      ? sdk.Client.build(await who.identity(), options(path), inboxId)
      : sdk.Client.build(await who.getIdentifier(), options(path));
  };
  async function prepare(group, row, ids, inboxId) {
    if (!modern) {
      const opts = { optimistic: true };
      if (row.reply_to !== null)
        return group.sendReply(
          {
            reference: ids[Number(row.reply_to)],
            referenceInboxId: inboxId,
            content: {
              type: type("text"),
              parameters: {},
              content: new TextEncoder().encode(row.text),
            },
          },
          opts,
        );
      if (row.attachment)
        return group.sendAttachment(
          {
            filename: row.attachment.filename,
            mimeType: row.attachment.mime_type,
            content: bytes(row.attachment.bytes_hex),
          },
          opts,
        );
      return group.sendText(row.text, opts);
    }
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
    if (!modern)
      return group.sendReaction(
        {
          reference,
          referenceInboxId: inboxId,
          content: reaction.content,
          action: sdk.ReactionAction.Added,
          schema: sdk.ReactionSchema.Unicode,
        },
        { optimistic: true },
      );
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
    const kind = modern ? content.kind : message.contentType.typeId;
    let text = null;
    let attachment = null;
    let reply = null;
    let parentText = null;
    if (kind === "text") text = modern ? content.value : content;
    else if (kind === "attachment") {
      const value = modern ? content.value : content;
      attachment = {
        filename: value.filename,
        mime_type: value.mimeType,
        bytes_hex: hex(value.content),
      };
    } else if (kind === "reply") {
      const reference = modern ? content.referenceId : content.referenceId;
      reply = keyById.get(reference);
      if (reply === undefined) throw new Error("Reply parent ID is absent");
      if (modern) {
        if (
          content.body.kind !== "text" ||
          message.inReplyToContent?.kind !== "text"
        ) {
          throw new Error("Reply body or eager parent was not decoded");
        }
        text = content.body.value;
        parentText = message.inReplyToContent.value;
      } else {
        text = content.content;
        parentText = content.inReplyTo?.content;
      }
    } else throw new Error(`Unexpected content type ${kind}`);
    const reactions = message.reactions.map((entry) => {
      const value = modern ? entry.reaction : entry.content;
      if (!value) throw new Error("Reaction was not decoded");
      return {
        content: value.content,
        action: modern
          ? value.action
          : value.action === sdk.ReactionAction.Added
            ? "added"
            : "unexpected",
        schema: modern
          ? value.schema
          : value.schema === sdk.ReactionSchema.Unicode
            ? "unicode"
            : "unexpected",
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
  function live(message) {
    const content = message.content;
    const kind = modern ? content?.kind : message.contentType?.typeId;
    const event = { id: message.id, kind };
    const reaction = (value) => {
      if (!value) throw new Error("Missing live reaction body");
      return {
        content: value.content,
        action: modern
          ? value.action
          : value.action === sdk.ReactionAction.Added
            ? "added"
            : "unexpected",
        schema: modern
          ? value.schema
          : value.schema === sdk.ReactionSchema.Unicode
            ? "unicode"
            : "unexpected",
      };
    };
    if (kind === "text") event.text = modern ? content.value : content;
    else if (kind === "attachment") {
      const value = modern ? content.value : content;
      if (!value?.content) throw new Error("Missing live attachment bytes");
      event.attachment = {
        filename: value.filename,
        mime_type: value.mimeType,
        bytes_hex: hex(value.content),
      };
    } else if (kind === "reply") {
      event.reference = content.referenceId;
      if (modern) {
        if (
          content.body?.kind !== "text" ||
          message.inReplyToContent?.kind !== "text"
        )
          throw new Error("Missing live reply body or eager parent");
        event.text = content.body.value;
        event.eager_parent_text = message.inReplyToContent.value;
      } else {
        event.text = content.content;
        if (content.inReplyTo != null)
          event.eager_parent_text = content.inReplyTo.content;
      }
    } else if (kind === "reaction") {
      event.reference = content.reference;
      event.reaction = reaction(modern ? content.reaction : content);
    } else throw new Error("Missing or unsupported live content");
    if (message.reactions !== undefined && message.reactions !== null)
      event.eager_reactions = message.reactions.map((entry) =>
        reaction(modern ? entry.reaction : entry.content),
      );
    return event;
  }
  return {
    newKey: () => accounts.generatePrivateKey().slice(2),
    create: (key, path, delay, called) =>
      sdk.Client.create(signer(key, delay, called), options(path)),
    open,
    inbox: (client) => client.inboxId,
    close: async (client) => {
      await (modern ? client.end() : client.close());
    },
    group: async (client, id) => {
      const group = await (modern
        ? client.conversations.getById(id)
        : client.conversations.getConversationById(id));
      if (!group) throw new Error("Seeded group is absent");
      return group;
    },
    createGroup: (client, members) => client.conversations.createGroup(members),
    syncConversations: (client) => client.conversations.sync(),
    groupId: (group) => group.id,
    prepare,
    prepareReaction,
    countUnpublished: async (group) =>
      Number(
        await group.countMessages({
          deliveryStatus: modern
            ? "unpublished"
            : sdk.DeliveryStatus.Unpublished,
        }),
      ),
    countPublished: async (group) =>
      Number(
        await group.countMessages({
          deliveryStatus: modern ? "published" : sdk.DeliveryStatus.Published,
        }),
      ),
    publish: (group) => group.publishMessages(),
    page: (group, count) =>
      group.messages(
        modern
          ? {
              limit: count,
              direction: "ascending",
              contentTypes: [type("text"), type("reply"), type("attachment")],
            }
          : {
              limit: count,
              direction: sdk.SortDirection.Ascending,
              contentTypes: [
                sdk.ContentType.Text,
                sdk.ContentType.Reply,
                sdk.ContentType.Attachment,
              ],
            },
      ),
    stream: async (client, group) => {
      if (!modern) return group.stream({ retryOnFail: false });
      const stream = sdk.MessageStream.openGroup(client, group);
      await stream.ready();
      return stream;
    },
    normalize,
    live,
  };
}
