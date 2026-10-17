import {
  AttachmentCodec,
  Backend,
  Client,
  TextCodec,
  Timestamp,
  XmtpError,
  decodeEncodedContent,
  decryptEncodedContent,
  encodeEncodedContent,
  encryptEncodedContent,
  initLogging,
  latestInboxUpdatesCount,
  metadataFieldRef,
  remoteAttachmentFromEncrypted,
  setLogSink,
  standardContentType,
  type ClientEvent,
  type ContentCodec,
  type Conversation,
  type Dm,
  type Group,
  type Message,
  type PublicIdentity,
  type Signer,
} from "@xmtp/node-sdk";

export function eoaSigner(
  address: string,
  signText: (text: string) => Promise<Uint8Array>,
): Signer {
  // #region signer
  const signer: Signer = {
    identity: async () => ({ identifier: address, kind: "ethereum" }),
    kind: async () => ({ kind: "eoa" }),
    sign: async ({ text }) => ({ kind: "ecdsa", value: await signText(text) }),
  };
  // #endregion signer
  return signer;
}

export function scwSigner(
  address: string,
  chainId: bigint,
  blockNumber: bigint | undefined,
  signText: (text: string) => Promise<Uint8Array>,
): Signer {
  // #region scw-signer
  const signer: Signer = {
    identity: async () => ({ identifier: address, kind: "ethereum" }),
    kind: async () => ({ kind: "scw", chainId, blockNumber }),
    sign: async ({ text }) => ({
      kind: "scw",
      bytes: await signText(text),
      address,
      chainId,
      blockNumber,
    }),
  };
  // #endregion scw-signer
  return signer;
}

export async function create(signer: Signer, key: Uint8Array) {
  // #region create
  const client = await Client.create(signer, {
    backend: { url: "https://xmtp.example.com", appVersion: "chat/8.0.0" },
    storage: {
      location: { directory: "./xmtp-data" },
      label: "chat",
      encryptionKey: key,
      pool: { min: 5, max: 25 },
    },
    deviceSync: true,
    registration: { auto: true },
  });
  // #endregion create
  return client;
}

export async function build(identity: PublicIdentity, key: Uint8Array) {
  // #region build
  const client = await Client.build(identity, {
    backend: { url: "https://xmtp.example.com" },
    storage: {
      location: { dbPath: "./chat.db3", attachmentsDir: "./chat-attachments" },
      encryptionKey: key,
    },
  });
  // #endregion build
  return client;
}

export async function authenticate(
  signer: Signer,
  fetchToken: () => Promise<{ token: string; expiresAtSeconds: bigint }>,
) {
  // #region auth
  const client = await Client.create(signer, {
    backend: {
      url: "https://xmtp.example.com",
      credentials: {
        async credential() {
          const credential = await fetchToken();
          return {
            value: `Bearer ${credential.token}`,
            expiresAtSeconds: credential.expiresAtSeconds,
          };
        },
      },
    },
    storage: { location: "default" },
  });
  // #endregion auth
  return client;
}

export async function standalone(identity: PublicIdentity, inboxId: string) {
  // #region standalone
  const backend = await Backend.connect({ url: "https://xmtp.example.com" });
  const availability = await Client.canMessage([identity], backend);
  const available = availability.get(
    `${identity.kind}:${identity.identifier.toLowerCase()}`,
  );
  const states = await Client.inboxStates([inboxId], backend);
  const counts = await latestInboxUpdatesCount([inboxId], backend);
  // #endregion standalone
  return { available, states, counts };
}

export async function identityChanges(
  client: Client,
  recoverySigner: Signer,
  identity: PublicIdentity,
  installationIds: string[],
) {
  // #region identity-changes
  const registered = await client.isRegistered();
  const state = await client.inboxState(true);
  const states = await client.inboxStates([client.inboxId], false);
  await client.removeAccount(recoverySigner, identity);
  await client.revokeInstallations(recoverySigner, installationIds);
  await client.revokeAllOtherInstallations(recoverySigner);
  // #endregion identity-changes
  return { registered, state, states };
}

export async function manualRegistration(client: Client, signer: Signer) {
  // #region signature-request
  const request = await client.unsafeCreateInboxSignatureRequest();
  if (request) {
    const text = await request.signatureText();
    await request.sign(signer);
    await client.unsafeApplySignatureRequest(request);
    console.log(text);
  }
  // #endregion signature-request
}

export async function conversations(
  client: Client,
  peer: PublicIdentity,
  inboxIds: string[],
) {
  // #region conversations
  const dm = await client.conversations.createDm(peer);
  const group = await client.conversations.createGroup(inboxIds, {
    name: "Project",
    imageUrl: "https://example.com/project.png",
    description: "Project chat",
    permissions: { kind: "adminOnly" },
    appData: "{}",
  });
  const draft = await client.conversations.createGroupOptimistic({
    name: "Draft",
  });
  const result = await draft.addMembers([peer]);
  console.log(result.added, result.failedInstallationIds);
  // #endregion conversations
  return { dm, group, draft };
}

export async function state(group: Group, dm: Dm) {
  // #region state
  const snapshot = await group.state();
  console.log(snapshot.name, snapshot.common.isActive, snapshot.permissions);
  const members = await group.members();
  for (const member of members) console.log(member.inboxId, member.identities);
  const peer = await dm.peerInboxId();
  const admins = await group.listAdmins();
  const creator = group.creatorInboxId;
  const isAdmin = creator === null ? false : await group.isAdmin(creator);
  // #endregion state
  return { snapshot, peer, admins, isAdmin };
}

export async function list(client: Client) {
  // #region list
  const groups = await client.conversations.listGroups({
    kind: "group",
    consentStates: ["allowed"],
    createdAfter: new Timestamp(0n),
    includeDuplicateDms: false,
  });
  const summary = await client.conversations.syncAll(["allowed"]);
  console.log(summary.eligible, summary.synced);
  // #endregion list
  return groups;
}

export async function disappearing(group: Group, fromNs: bigint) {
  // #region disappearing
  await group.updateDisappearingSettings({
    from: new Timestamp(fromNs),
    retentionNs: 86_400_000_000_000n,
  });
  const settings = (await group.state()).common.disappearingSettings;
  await group.updateDisappearingSettings(undefined);
  // #endregion disappearing
  return settings;
}

export async function send(group: Group, parent: Message) {
  // #region send
  const id = await group.sendText("Hello", { shouldPush: false });
  await group.sendReaction(parent.id, parent.senderInboxId, {
    content: "👍",
    action: "added",
    schema: "unicode",
  });
  await group.sendReply(
    parent.id,
    parent.senderInboxId,
    new TextCodec().encode("Reply"),
  );
  await parent.reply("Another reply");
  await parent.react({ content: "👍", action: "added", schema: "unicode" });
  const prepared = await group.prepareMessage(new TextCodec(), "Draft");
  await group.publishMessage(prepared);
  // #endregion send
  return id;
}

export async function read(group: Group) {
  // #region read
  const messages = await group.messages({
    limit: 20,
    sentAfter: new Timestamp(0n),
    sortBy: "sentAt",
    direction: "descending",
    contentTypes: [standardContentType("text")],
  });
  for (const message of messages) {
    if (message.content.kind === "text") console.log(message.content.value);
    if (
      message.content.kind === "reply" &&
      message.content.body.kind === "text"
    ) {
      console.log(message.content.referenceId, message.content.body.value);
    }
    if (message.content.kind === "unknown")
      console.error(message.content.error);
    console.log(message.sentAt.date, message.sentAt.ns, message.replyCount);
  }
  const count = await group.countMessages(undefined);
  // #endregion read
  return { messages, count };
}

type Point = { readonly x: number; readonly y: number };

export function pointCodec(): ContentCodec<Point> {
  // #region codec
  const codec: ContentCodec<Point> = {
    type: {
      authorityId: "example.com",
      typeId: "point",
      versionMajor: 1,
      versionMinor: 0,
    },
    encode: ({ x, y }) => ({
      type: codec.type,
      parameters: new Map(),
      content: new TextEncoder().encode(`${x},${y}`),
    }),
    decode: ({ content }) => {
      const parts = new TextDecoder().decode(content).split(",").map(Number);
      if (
        parts.length !== 2 ||
        parts.some((value) => !Number.isFinite(value))
      ) {
        throw new Error("Invalid point");
      }
      return { x: parts[0], y: parts[1] };
    },
    fallback: ({ x, y }) => `Point ${x}, ${y}`,
    shouldPush: () => true,
  };
  // #endregion codec
  return codec;
}

export async function registerCodec(signer: Signer) {
  // #region register-codec
  const client = await Client.create(signer, {
    backend: { url: "https://xmtp.example.com" },
    storage: { location: "default" },
    codecs: [pointCodec()],
  });
  // #endregion register-codec
  return client;
}

export async function customContent(group: Group, message: Message) {
  const codec = pointCodec();
  // #region custom-content
  await group.send(codec, { x: 1, y: 2 });
  await message.reply(codec, { x: 3, y: 4 });
  if (message.content.kind === "custom") {
    const value = message.content.value;
    if (
      typeof value === "object" &&
      value !== null &&
      "x" in value &&
      "y" in value
    ) {
      console.log(value.x, value.y);
    }
    if (message.content.error) console.error(message.content.error);
  }
  // #endregion custom-content
}

export async function stream(
  client: Client,
  process: (message: Message) => Promise<void>,
) {
  // #region stream
  const messages = client.conversations.streamAllMessages({
    consentStates: ["allowed"],
    onClose: (reason) => {
      if (reason.kind === "failed") console.error(reason.error);
    },
  });
  await messages.ready();
  try {
    for await (const message of messages) await process(message);
  } finally {
    await messages.end();
  }
  // #endregion stream
}

export async function callbackStream(
  group: Group,
  process: (message: Message) => Promise<void>,
) {
  // #region callback-stream
  const messages = group.streamMessages();
  await messages.onValue(async (message) => {
    await process(message);
  });
  // #endregion callback-stream
}

export async function historyAndLive(
  group: Group,
  process: (message: Message) => Promise<void>,
) {
  // #region history-live
  const history = await group.messageHistorySnapshot(100);
  for (const message of history.messages) await process(message);
  const live = group.streamMessages({ from: history.cursor });
  for await (const message of live) await process(message);
  // #endregion history-live
}

export async function conversationStream(
  client: Client,
  process: (conversation: Conversation) => Promise<void>,
) {
  // #region conversation-stream
  const stream = client.conversations.stream({ conversationKind: "group" });
  for await (const conversation of stream) await process(conversation);
  // #endregion conversation-stream
}

export async function events(
  client: Client,
  process: (event: ClientEvent) => Promise<void>,
) {
  // #region events
  const events = await client.events({
    kinds: [
      "consent.changed",
      "message.deleted",
      "conversation.metadata_changed",
    ],
  });
  for await (const event of events) await process(event);
  // #endregion events
}

export async function consent(
  client: Client,
  conversationId: string,
  inboxId: string,
) {
  // #region consent
  await client.preferences.setConsentStates([
    { entity: { kind: "conversation", conversationId }, state: "allowed" },
    { entity: { kind: "inbox", inboxId }, state: "denied" },
  ]);
  const state = await client.preferences.consentState({
    kind: "inbox",
    inboxId,
  });
  // #endregion consent
  return state;
}

export async function archives(client: Client, key: Uint8Array) {
  // #region archives
  const metadata = await client.archives.exportToFile("./archive.xmtp", key, {
    start: new Timestamp(0n),
    elements: ["messages", "consent"],
    excludeDisappearingMessages: true,
  });
  await client.archives.importFromFile("./archive.xmtp", key);
  const inspected = await client.archives.metadataFromFile(
    "./archive.xmtp",
    key,
  );
  console.log(metadata.exportedAt.date, inspected.backupVersion);
  // #endregion archives
  return metadata;
}

export async function notifications(
  client: Client,
  group: Group,
  token: string,
) {
  // #region notifications
  await client.enableNotifications({ channel: { kind: "fcm", token } });
  await group.setNotifications("disabled");
  const enabled = (await group.state()).common.notificationsEnabled;
  await group.setNotifications("default");
  const state = client.notificationState();
  if (state.kind === "failed") console.error(state.error);
  await client.disableNotifications();
  // #endregion notifications
  return enabled;
}

export async function attachment(client: Client, group: Group, path: string) {
  // #region attachment
  if (!client.attachments.offered)
    throw new Error("Attachments are unavailable");
  const pending = await client.attachments.create({
    kind: "path",
    path,
    filename: "photo.png",
    mimeType: "image/png",
  });
  await pending.upload();
  await group.sendRemoteAttachment(pending.remoteAttachment);
  const downloaded = await client.attachments.download(
    pending.remoteAttachment,
  );
  // #endregion attachment
  return downloaded;
}

export async function encryptedAttachment(url: string, data: Uint8Array) {
  // #region attachment-crypto
  const codec = new AttachmentCodec();
  const envelope = encodeEncodedContent(
    codec.encode({
      filename: "photo.png",
      mimeType: "image/png",
      content: data,
    }),
  );
  const encrypted = await encryptEncodedContent(envelope);
  const remote = remoteAttachmentFromEncrypted(url, encrypted, "photo.png");
  const decrypted = codec.decode(
    decodeEncodedContent(await decryptEncodedContent(encrypted)),
  );
  // #endregion attachment-crypto
  return { remote, encrypted, decrypted };
}

export async function metadata(group: Group) {
  // #region metadata
  const field = metadataFieldRef("userDisplayName");
  await group.updateUserData([
    { field, value: { kind: "string", value: "Sam" } },
  ]);
  const profiles = await group.userData([field], undefined);
  const fields = await group.metadataFields();
  // #endregion metadata
  return { profiles, fields };
}

export async function logging() {
  // #region logging
  await initLogging({ level: "info", structured: true });
  await setLogSink({
    async log(record) {
      console.log(record.timestamp.date, record.level, record.message);
    },
  });
  // #endregion logging
}

export async function diagnostics(client: Client) {
  // #region diagnostics
  const api = await client.diagnostics.apiStatistics();
  const identity = await client.diagnostics.identityStatistics();
  console.log(
    api.publish,
    api.queryNewest,
    identity.verifySmartContractWalletSignatures,
  );
  await client.diagnostics.clearStatistics();
  // #endregion diagnostics
}

export function errorDetails(error: unknown) {
  // #region errors
  if (error instanceof XmtpError) {
    console.error(
      error.details.code,
      error.details.category,
      error.details.retryable,
    );
    if (error.details.streamFailure)
      console.error(error.details.streamFailure.barriers);
  }
  // #endregion errors
}

export async function close(client: Client) {
  // #region close
  await client.end();
  // #endregion close
}
