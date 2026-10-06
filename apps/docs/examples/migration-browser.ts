import {
  Client,
  type Group,
  Storage,
  Timestamp,
  XmtpError,
  type BackendSource,
  type ClientOptions,
  type ContentCodec,
  type Conversation,
  type DeliveryCursor,
  type EncodedContent,
  type Message,
  type PublicIdentity,
  type Signer,
} from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";

// #region signer
export function walletSigner(
  address: string,
  signText: (text: string) => Promise<Uint8Array>,
): Signer {
  return {
    identity: async () => ({ identifier: address, kind: "ethereum" }),
    kind: async () => ({ kind: "eoa" }),
    sign: async (request) => ({
      kind: "ecdsa",
      value: await signText(request.text),
    }),
  };
}
// #endregion signer

// #region create
export async function createClient(signer: Signer) {
  return Client.create(signer, {
    backend: { url: "https://xmtp.example.com", appVersion: "chat/1.0" },
    storage: { location: "default" },
  });
}
// #endregion create

// #region reopen
export async function reopenCurrentDatabase(
  identity: PublicIdentity,
  backend: BackendSource,
  currentDbPath: string,
  attachmentsDir: string,
) {
  // Use a database already created by this SDK. End its client first.
  // Both paths name OPFS entries.
  const options: ClientOptions = {
    backend,
    storage: { location: { dbPath: currentDbPath, attachmentsDir } },
  };
  const client = await Client.build(identity, options);
  const inboxId = client.inboxId;
  try {
    if ((await client.storage.path()) !== currentDbPath) {
      throw new Error("The client opened another database");
    }
  } finally {
    await client.end();
  }
  return Client.build(identity, { ...options, allowOffline: true }, inboxId);
}
// #endregion reopen

// #region conversations
export async function createConversations(
  client: Client,
  peer: PublicIdentity,
  extraMember: PublicIdentity,
) {
  const dm = await client.conversations.createDm(peer);
  const group = await client.conversations.createGroup([peer], {
    name: "Team",
    permissions: { kind: "adminOnly" },
  });
  const result = await group.addMembers([extraMember]);
  const listed = await client.conversations.list({
    kind: "group",
    consentStates: ["allowed"],
    orderBy: "lastActivity",
    limit: 20,
  });
  return { dm, group, result, listed };
}
// #endregion conversations

// #region state
export async function readGroupState(group: Group) {
  const state = await group.state();
  await group.updateAppData("updated", state.appData);
  await group.updateDisappearingSettings({
    from: new Timestamp(BigInt(Date.now()) * 1_000_000n),
    retentionNs: 86_400_000_000_000n,
  });
  // Pass undefined to remove the disappearing settings.
  await group.updateDisappearingSettings(undefined);
  return {
    name: state.name,
    admins: state.admins,
    active: state.common.isActive,
  };
}
// #endregion state

// #region send
export async function sendMessages(conversation: Conversation) {
  const messageId = await conversation.sendText("Hello");
  await conversation.sendMarkdown("**Hello**", { shouldPush: true });
  await conversation.sendReadReceipt();
  return messageId;
}
// #endregion send

// #region optimistic
export async function prepareMessage(conversation: Conversation) {
  await initPureWasm();
  const codec = new TextCodec();
  const messageId = await conversation.prepareMessage(codec, "Send later");
  await conversation.publishMessage(messageId);
  return messageId;
}
// #endregion optimistic

// #region history
export async function readHistory(conversation: Conversation) {
  const firstPage = await conversation.messages({
    limit: 20,
    sortBy: "sentAt",
    direction: "descending",
  });
  const last = firstPage.at(-1);
  const nextPage = last
    ? await conversation.messages({
        limit: 20,
        sortBy: "sentAt",
        direction: "descending",
        sentBefore: last.sentAt,
      })
    : [];
  return { firstPage, nextPage };
}
// #endregion history

// #region content
export function displayMessage(message: Message): string {
  switch (message.content.kind) {
    case "text":
    case "markdown":
      return message.content.value;
    case "custom":
      return message.fallback ?? "Custom content";
    case "unknown":
      return message.fallback ?? "Cannot decode this content";
    default:
      return message.fallback ?? message.content.kind;
  }
}
// #endregion content

// #region actions
export async function actOnMessage(message: Message) {
  const replyId = await message.reply("Received");
  const reactionId = await message.react({
    action: "added",
    schema: "unicode",
    content: "👍",
  });
  const parent = await message.parent();
  const conversation = await message.conversation();
  return { replyId, reactionId, parent, conversation };
}

export async function deleteMessage(message: Message) {
  return message.delete();
}
// #endregion actions

// #region stream
export async function receiveMessages(
  client: Client,
  handle: (message: Message) => Promise<void>,
  signal: AbortSignal,
) {
  const stream = client.conversations.streamAllMessages({
    consentStates: ["allowed", "unknown"],
    signal,
  });
  try {
    for await (const message of stream) {
      await handle(message);
    }
  } finally {
    await stream.end();
  }
}
// #endregion stream

// #region snapshot
export async function receiveHistoryAndLive(
  client: Client,
  handle: (message: Message) => Promise<void>,
  saveCursor: (cursor: DeliveryCursor) => Promise<void>,
  signal: AbortSignal,
) {
  const selection = { consentStates: ["allowed" as const] };
  const snapshot = await client.conversations.messageHistorySnapshot(
    100,
    selection,
  );
  for (const message of snapshot.messages) await handle(message);
  await saveCursor(snapshot.cursor);
  const stream = client.conversations.streamAllMessages({
    ...selection,
    from: snapshot.cursor,
    signal,
  });
  try {
    for await (const message of stream) {
      await handle(message);
      if (message.deliveryCursor !== null) {
        await saveCursor(message.deliveryCursor);
      }
    }
  } finally {
    await stream.end();
  }
}
// #endregion snapshot

// #region conversation-stream
export async function receiveConversations(
  client: Client,
  handle: (conversation: Conversation) => Promise<void>,
  signal: AbortSignal,
) {
  const stream = client.conversations.stream({
    conversationKind: "group",
    signal,
  });
  try {
    for await (const conversation of stream) await handle(conversation);
  } finally {
    await stream.end();
  }
}
// #endregion conversation-stream

// #region consent
export async function allowConversation(
  client: Client,
  conversationId: string,
) {
  const entity = { kind: "conversation" as const, conversationId };
  await client.preferences.setConsentStates([{ entity, state: "allowed" }]);
  return client.preferences.consentState(entity);
}

export async function consentEvents(client: Client) {
  return client.events({
    kinds: ["consent.changed"],
    references_own_messages: false,
  });
}
// #endregion consent

// #region identity
export async function identityQueries(
  client: Client,
  identity: PublicIdentity,
  backend: BackendSource,
) {
  const inboxId = await Client.inboxIdFor(identity, backend);
  const canMessage = await Client.canMessage([identity], backend);
  const ownState = await client.inboxState(true);
  const states = await Client.inboxStates([inboxId], backend);
  return { inboxId, canMessage, ownState, states };
}

export async function signAccountRequest(
  client: Client,
  identity: PublicIdentity,
  signer: Signer,
) {
  const request = await client.unsafeRemoveAccountSignatureRequest(identity);
  const text = await request.signatureText();
  await request.sign(signer);
  await client.unsafeApplySignatureRequest(request);
  return text;
}
// #endregion identity

// #region archives
export async function exportArchive(client: Client, key: Uint8Array) {
  return client.archives.exportToBytes(key, {
    elements: ["messages", "consent"],
    excludeDisappearingMessages: true,
  });
}

export async function importArchive(
  client: Client,
  data: Uint8Array,
  key: Uint8Array,
) {
  const metadata = await client.archives.metadataFromBytes(data, key);
  await client.archives.importFromBytes(data, key);
  return metadata;
}
// #endregion archives

// #region storage-admin
export async function exportClosedDatabase(dbPath: string) {
  // End every client that owns this database before an import or replacement.
  const admin = await Storage.admin();
  try {
    if (!(await admin.fileExists(dbPath))) {
      throw new Error("The database does not exist");
    }
    return await admin.exportDb(dbPath);
  } finally {
    await admin.end();
  }
}
// #endregion storage-admin

// #region codec
export const noteCodec: ContentCodec<string> = {
  type: {
    authorityId: "example.com",
    typeId: "note",
    versionMajor: 1,
    versionMinor: 0,
  },
  encode: (value): EncodedContent => ({
    type: noteCodec.type,
    parameters: new Map<string, string>(),
    content: new TextEncoder().encode(value),
  }),
  decode: (encoded) => new TextDecoder().decode(encoded.content),
  fallback: (value) => value,
  shouldPush: () => true,
};

export async function createWithCodec(signer: Signer) {
  return Client.create(signer, {
    backend: { url: "https://xmtp.example.com" },
    storage: { location: "default" },
    codecs: [noteCodec],
  });
}

export async function sendCustom(conversation: Conversation) {
  return conversation.send(noteCodec, "A typed note");
}
// #endregion codec

// #region errors
export function describeError(error: unknown): string {
  if (error instanceof XmtpError) {
    const { code, retryable, message } = error.details;
    return `${code}: ${message}; retryable=${retryable}`;
  }
  return error instanceof Error ? error.message : String(error);
}
// #endregion errors
