import {
  Client,
  type CatchUpSummary,
  type ClientOptions,
  type Credential,
  type EventFilter,
  type GroupSyncSummary,
  type InboxId,
  type InboxState,
  type InstallationId,
  type KeyPackageStatus,
  type MessageContent,
  type MessageMetadataEntry,
  type NotificationConfig,
  type NotificationState,
  type PublicIdentity,
  type ServerConfiguration,
  type SignatureRequest,
  type Signer,
  type BackendSource,
  encodeText,
} from "xmtp-sdk";

// This list comes from the retained Client surface in the approved plan, not
// from the generator. Removing a public forwarder must fail this compile.
export async function consumeClient(
  signer: Signer,
  options: ClientOptions,
  identity: PublicIdentity,
  credential: Credential,
  request: SignatureRequest,
  filter: EventFilter,
  config: NotificationConfig,
): Promise<void> {
  const client = await Client.create(signer, options);
  const built = await Client.build(identity, options, client.inboxId);
  const inboxId: InboxId = client.inboxId;
  const installationId: InstallationId = client.installationId;
  const installationBytes: Uint8Array = client.installationIdBytes;
  const clientIdentity: PublicIdentity = client.identity;
  const inMemory: boolean = client.isInMemory;
  const path: string | undefined = client.storagePath;
  const version: string = client.libxmtpVersion;
  const appVersion: string | undefined = client.appVersion;
  const clientOptions: ClientOptions = client.options;
  const objects = [
    client.conversations,
    client.preferences,
    client.diagnostics,
    client.archives,
    client.storage,
  ];
  await client.register();
  const registered: boolean = await client.isRegistered();
  const state: InboxState = await client.inboxState(true);
  const states: InboxState[] = await client.inboxStates([inboxId], false);
  const found: InboxId | undefined = await client.inboxIdFor(identity);
  const reachable: Map<string, boolean> = await client.canMessage([identity]);
  const latest: Map<string, bigint> = await client.latestInboxUpdatesCount(
    [inboxId],
    false,
  );
  const own: bigint = await client.ownInboxUpdatesCount(false);
  const packages: Map<string, KeyPackageStatus> =
    await client.keyPackageStatuses([installationId]);
  const configuration: ServerConfiguration = client.serverConfiguration;
  const refreshed: ServerConfiguration =
    await client.refreshServerConfiguration();
  await client.setCredential(credential);
  const signature: Uint8Array = await client.signWithInstallationKey("text");
  const verified: boolean = await client.verifySignedWithInstallationKey(
    "text",
    signature,
  );
  const caughtUp: CatchUpSummary = await client.catchUpToLive(undefined);
  const synced: GroupSyncSummary = await client.syncAllDeviceSyncGroups();
  const decoded: MessageContent = await client.decodeContent(
    encodeText("text"),
  );
  const createInbox: SignatureRequest | undefined =
    await client.unsafeCreateInboxSignatureRequest();
  const addAccount: SignatureRequest =
    await client.unsafeAddAccountSignatureRequest(identity, false);
  const removeAccount: SignatureRequest =
    await client.unsafeRemoveAccountSignatureRequest(identity);
  const revoke: SignatureRequest =
    await client.unsafeRevokeInstallationsSignatureRequest([]);
  const revokeOthers: SignatureRequest | undefined =
    await client.unsafeRevokeAllOtherInstallationsSignatureRequest();
  const recovery: SignatureRequest =
    await client.unsafeChangeRecoveryIdentifierSignatureRequest(identity);
  await client.unsafeApplySignatureRequest(request);
  await client.unsafeAddAccount(signer, false);
  await client.removeAccount(signer, identity);
  await client.revokeInstallations(signer, []);
  await client.revokeAllOtherInstallations(signer);
  await client.changeRecoveryIdentifier(signer, identity);
  const enabled: NotificationState = await client.enableNotifications(config);
  const notificationState: NotificationState = client.notificationState();
  await client.disableNotifications();
  const events = await client.events(filter);
  const listener = await client.startListener(filter, () => undefined);
  await client.stopListener(listener);
  void [
    objects,
    installationBytes,
    clientIdentity,
    inMemory,
    path,
    version,
    appVersion,
    clientOptions,
    registered,
    state,
    states,
    found,
    reachable,
    latest,
    own,
    packages,
    configuration,
    refreshed,
    verified,
    caughtUp,
    synced,
    decoded,
    createInbox,
    addAccount,
    removeAccount,
    revoke,
    revokeOthers,
    recovery,
    enabled,
    notificationState,
    events,
  ];
  await built.end();
  await client.end();
}

export async function consumeStaticHelpers(
  signer: Signer,
  identity: PublicIdentity,
  backend: BackendSource,
): Promise<void> {
  const configuration: ServerConfiguration =
    await Client.fetchServerConfiguration(backend);
  const reachable: Map<string, boolean> = await Client.canMessage(
    [identity],
    backend,
  );
  const inbox: InboxId = await Client.inboxIdFor(identity, backend);
  const states: InboxState[] = await Client.inboxStates([inbox], backend);
  const packages: Map<string, KeyPackageStatus> =
    await Client.keyPackageStatuses([], backend);
  const newest: Map<string, MessageMetadataEntry> =
    await Client.newestMessageMetadata([], backend);
  await Client.revokeInstallations(signer, inbox, [], backend);
  const address: boolean = await Client.isAddressAuthorized(
    inbox,
    "address",
    backend,
  );
  const installation: boolean = await Client.isInstallationAuthorized(
    inbox,
    "installation",
    backend,
  );
  const verified: boolean = await Client.verifySignedWithPublicKey(
    "text",
    new Uint8Array(0),
    new Uint8Array(0),
  );
  void [
    configuration,
    reachable,
    states,
    packages,
    newest,
    address,
    installation,
    verified,
  ];
}
