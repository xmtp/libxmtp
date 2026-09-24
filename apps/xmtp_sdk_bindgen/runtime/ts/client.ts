import {
  Client as RawClient,
  BackendSource,
  StorageLocation,
  StorageLocation_Tags,
  canMessageWithBackend,
  fetchServerConfiguration,
  inboxIdForWithBackend,
  inboxStatesWithBackend,
  isAddressAuthorizedWithBackend,
  isInstallationAuthorizedWithBackend,
  keyPackageStatusesWithBackend,
  newestMessageMetadataWithBackend,
  revokeInstallationsWithBackend,
  verifySignedWithPublicKey,
  type BackendOptions,
  type BackendLike,
  type CanMessageEntry,
  type ClientLike,
  type ClientOptions,
  type InboxState,
  type KeyPackageStatusEntry,
  type MessageMetadataEntry,
  type PublicIdentity,
  type ServerConfiguration,
  type Signer,
} from "../xmtp_sdk";
import type { ConversationID, InboxID, InstallationID } from "./ids";

declare const process: { cwd(): string } | undefined;

function resolvedOptions(options: ClientOptions): ClientOptions {
  if (options.storage.location.tag !== StorageLocation_Tags.Default)
    return options;
  const directory =
    typeof process === "undefined" ? "xmtp-sdk" : `${process.cwd()}/xmtp`;
  return {
    ...options,
    storage: {
      ...options.storage,
      location: new StorageLocation.Directory(directory),
    },
  };
}

export class ClientRegistry {
  private static readonly entries = new Map<bigint, WeakRef<Client>>();

  static set(key: bigint, client: Client): void {
    for (const [oldKey, reference] of this.entries) {
      if (reference.deref() === undefined) this.entries.delete(oldKey);
    }
    this.entries.set(key, new WeakRef(client));
  }

  static get(key: bigint): Client | undefined {
    const client = this.entries.get(key)?.deref();
    if (client === undefined) this.entries.delete(key);
    return client;
  }

  static delete(key: bigint): void {
    this.entries.delete(key);
  }
}

export class Client {
  private readonly key: bigint;

  private constructor(readonly raw: ClientLike) {
    this.key = raw.clientKey();
    ClientRegistry.set(this.key, this);
  }

  static async create(signer: Signer, options: ClientOptions): Promise<Client> {
    return new Client(await RawClient.create(signer, resolvedOptions(options)));
  }

  static async build(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxID?: InboxID,
  ): Promise<Client> {
    return new Client(
      await RawClient.build(identity, resolvedOptions(options), inboxID),
    );
  }

  static fetchServerConfiguration(
    options: BackendOptions,
  ): Promise<ServerConfiguration> {
    return fetchServerConfiguration(new BackendSource.Options(options));
  }

  static canMessage(
    identities: PublicIdentity[],
    backend: BackendLike,
  ): Promise<CanMessageEntry[]> {
    return canMessageWithBackend(
      new BackendSource.Connected(backend),
      identities,
    );
  }

  static inboxIDFor(
    identity: PublicIdentity,
    backend: BackendLike,
  ): Promise<InboxID> {
    return inboxIdForWithBackend(
      new BackendSource.Connected(backend),
      identity,
    );
  }

  static inboxStates(
    ids: InboxID[],
    backend: BackendLike,
  ): Promise<InboxState[]> {
    return inboxStatesWithBackend(new BackendSource.Connected(backend), ids);
  }

  static keyPackageStatuses(
    ids: InstallationID[],
    backend: BackendLike,
  ): Promise<KeyPackageStatusEntry[]> {
    return keyPackageStatusesWithBackend(
      new BackendSource.Connected(backend),
      ids,
    );
  }

  static newestMessageMetadata(
    ids: ConversationID[],
    backend: BackendLike,
  ): Promise<MessageMetadataEntry[]> {
    return newestMessageMetadataWithBackend(
      new BackendSource.Connected(backend),
      ids,
    );
  }

  static revokeInstallations(
    signer: Signer,
    inboxID: InboxID,
    ids: InstallationID[],
    backend: BackendLike,
  ): Promise<void> {
    return revokeInstallationsWithBackend(
      new BackendSource.Connected(backend),
      signer,
      inboxID,
      ids,
    );
  }

  static isAddressAuthorized(
    inboxID: InboxID,
    address: string,
    backend: BackendLike,
  ): Promise<boolean> {
    return isAddressAuthorizedWithBackend(
      new BackendSource.Connected(backend),
      inboxID,
      address,
    );
  }

  static isInstallationAuthorized(
    inboxID: InboxID,
    installationID: InstallationID,
    backend: BackendLike,
  ): Promise<boolean> {
    return isInstallationAuthorizedWithBackend(
      new BackendSource.Connected(backend),
      inboxID,
      installationID,
    );
  }

  static verifySignedWithPublicKey(
    text: string,
    signature: ArrayBuffer,
    publicKey: ArrayBuffer,
  ): Promise<boolean> {
    return verifySignedWithPublicKey(text, signature, publicKey);
  }

  inboxID(): InboxID {
    return this.raw.inboxID();
  }

  installationID(): InstallationID {
    return this.raw.installationID();
  }

  conversations() {
    return this.raw.conversations();
  }

  async end(): Promise<void> {
    try {
      await this.raw.end();
    } finally {
      ClientRegistry.delete(this.key);
    }
  }
}
