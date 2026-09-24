import {
  Client as RawClient,
  type BackendSource as BackendSourceLike,
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
  type CanMessageEntry,
  type ClientLike,
  type ClientOptions,
  type ContentTypeID,
  type EncodedContent,
  type InboxState,
  type KeyPackageStatusEntry,
  type MessageMetadataEntry,
  type PublicIdentity,
  type ServerConfiguration,
  type Signer,
} from "../xmtp_sdk";
import type { ConversationID, InboxID, InstallationID } from "./ids";

declare const process: { cwd(): string } | undefined;

export interface ContentCodec<T> {
  readonly type: ContentTypeID;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
}

type AnyCodec = {
  readonly type: ContentTypeID;
  decode(encoded: EncodedContent): unknown;
  encode(value: never): EncodedContent;
};

export type SDKClientOptions = ClientOptions & {
  codecs?: readonly AnyCodec[];
};

function codecKey(type: ContentTypeID): string {
  return `${type.authorityID}/${type.typeID}/${type.versionMajor}`;
}

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
  private readonly codecs: ReadonlyMap<string, AnyCodec>;

  private constructor(
    readonly raw: ClientLike,
    codecs: readonly AnyCodec[],
  ) {
    this.key = raw.clientKey();
    this.codecs = new Map(codecs.map((codec) => [codecKey(codec.type), codec]));
    ClientRegistry.set(this.key, this);
  }

  static async create(
    signer: Signer,
    options: SDKClientOptions,
  ): Promise<Client> {
    const { codecs = [], ...rustOptions } = options;
    return new Client(
      await RawClient.create(signer, resolvedOptions(rustOptions)),
      codecs,
    );
  }

  static async build(
    identity: PublicIdentity,
    options: SDKClientOptions,
    inboxID?: InboxID,
  ): Promise<Client> {
    const { codecs = [], ...rustOptions } = options;
    return new Client(
      await RawClient.build(identity, resolvedOptions(rustOptions), inboxID),
      codecs,
    );
  }

  static fetchServerConfiguration(
    backend: BackendSourceLike,
  ): Promise<ServerConfiguration> {
    return fetchServerConfiguration(backend);
  }

  static canMessage(
    identities: PublicIdentity[],
    backend: BackendSourceLike,
  ): Promise<CanMessageEntry[]> {
    return canMessageWithBackend(backend, identities);
  }

  static inboxIDFor(
    identity: PublicIdentity,
    backend: BackendSourceLike,
  ): Promise<InboxID> {
    return inboxIdForWithBackend(backend, identity);
  }

  static inboxStates(
    ids: InboxID[],
    backend: BackendSourceLike,
  ): Promise<InboxState[]> {
    return inboxStatesWithBackend(backend, ids);
  }

  static keyPackageStatuses(
    ids: InstallationID[],
    backend: BackendSourceLike,
  ): Promise<KeyPackageStatusEntry[]> {
    return keyPackageStatusesWithBackend(backend, ids);
  }

  static newestMessageMetadata(
    ids: ConversationID[],
    backend: BackendSourceLike,
  ): Promise<MessageMetadataEntry[]> {
    return newestMessageMetadataWithBackend(backend, ids);
  }

  static revokeInstallations(
    signer: Signer,
    inboxID: InboxID,
    ids: InstallationID[],
    backend: BackendSourceLike,
  ): Promise<void> {
    return revokeInstallationsWithBackend(backend, signer, inboxID, ids);
  }

  static isAddressAuthorized(
    inboxID: InboxID,
    address: string,
    backend: BackendSourceLike,
  ): Promise<boolean> {
    return isAddressAuthorizedWithBackend(backend, inboxID, address);
  }

  static isInstallationAuthorized(
    inboxID: InboxID,
    installationID: InstallationID,
    backend: BackendSourceLike,
  ): Promise<boolean> {
    return isInstallationAuthorizedWithBackend(
      backend,
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

  decodeCustom(
    encoded: EncodedContent,
  ): { value?: unknown; error?: string } | undefined {
    const codec = this.codecs.get(codecKey(encoded.type));
    if (codec === undefined) return undefined;
    try {
      return { value: codec.decode(encoded) };
    } catch (error) {
      return { error: String(error) };
    }
  }

  async end(): Promise<void> {
    try {
      await this.raw.end();
    } finally {
      ClientRegistry.delete(this.key);
    }
  }
}
