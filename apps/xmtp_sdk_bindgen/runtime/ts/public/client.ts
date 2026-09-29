import {
  ClientMembers,
  currentProjection,
  liftEncodedContent,
  liftInboxState,
  liftKeyPackageStatus,
  liftMessageMetadataEntry,
  liftServerConfiguration,
  lowerBackendSource,
  lowerClientOptions,
  lowerContentTypeId,
  lowerEncodedContent,
  lowerPublicIdentity,
  lowerSigner,
  type BackendSource,
  type ClientOptions as ProjectedClientOptions,
  type ConversationId,
  type InboxId,
  type InboxState,
  type InstallationId,
  type KeyPackageStatus,
  type MessageMetadataEntry,
  type ObjectProjection,
  type PublicIdentity,
  type ServerConfiguration,
  type Signer,
} from "../../public-values.gen";
import type {
  ContentTypeId as BoundTypeId,
  EncodedContent as BoundEncoded,
  ClientLike,
} from "../../xmtp_sdk";
import type { ContentCodec } from "./codec";
import { HostClient, bindingClient, type HostClientOptions } from "./host";

/** Client options with the custom codecs that this client decodes. */
export type ClientOptions = ProjectedClientOptions & {
  readonly codecs?: readonly ContentCodec<never>[];
};

type HostCodec = {
  readonly type: BoundTypeId;
  encode(value: never): BoundEncoded;
  decode(encoded: BoundEncoded): unknown;
};

function hostCodec(
  codec: ContentCodec<never>,
  projection: ObjectProjection,
): HostCodec {
  return {
    type: lowerContentTypeId(codec.type, projection),
    encode: (value) => lowerEncodedContent(codec.encode(value), projection),
    decode: (encoded) => codec.decode(liftEncodedContent(encoded, projection)),
  };
}

function hostOptions(
  options: ClientOptions,
  projection: ObjectProjection,
): HostClientOptions {
  const { codecs = [], ...rest } = options;
  return {
    ...lowerClientOptions(rest, projection),
    codecs: codecs.map((codec) => hostCodec(codec, projection)),
  };
}

const hosts = new WeakMap<Client, HostClient>();
const clients = new WeakMap<HostClient, Client>();
let create!: (host: HostClient) => Client;

/** The public Client of a host Client. One host has one public Client. */
export function publicClient(host: HostClient): Client {
  return clients.get(host) ?? create(host);
}

function hostOf(client: Client): HostClient {
  const host = hosts.get(client);
  if (host === undefined) throw new TypeError("not an XMTP Client");
  return host;
}

/**
 * An XMTP client. Its inputs and results are public values; the binding
 * Client and the target transport stay private to the package.
 */
export class Client extends ClientMembers {
  static {
    create = (host) => {
      const client = new Client();
      hosts.set(client, host);
      clients.set(host, client);
      return client;
    };
  }

  private constructor() {
    super();
  }

  protected bindingClient(): ClientLike {
    return bindingClient(hostOf(this));
  }

  static async create(signer: Signer, options: ClientOptions): Promise<Client> {
    const projection = currentProjection();
    return publicClient(
      await HostClient.create(
        lowerSigner(signer, projection),
        hostOptions(options, projection),
      ),
    );
  }

  static async build(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId?: InboxId,
  ): Promise<Client> {
    const projection = currentProjection();
    return publicClient(
      await HostClient.build(
        lowerPublicIdentity(identity, projection),
        hostOptions(options, projection),
        inboxId,
      ),
    );
  }

  static async fetchServerConfiguration(
    backend: BackendSource,
  ): Promise<ServerConfiguration> {
    const projection = currentProjection();
    return liftServerConfiguration(
      await HostClient.fetchServerConfiguration(
        lowerBackendSource(backend, projection),
      ),
      projection,
    );
  }

  static canMessage(
    identities: PublicIdentity[],
    backend: BackendSource,
  ): Promise<Map<string, boolean>> {
    const projection = currentProjection();
    return HostClient.canMessage(
      identities.map((identity) => lowerPublicIdentity(identity, projection)),
      lowerBackendSource(backend, projection),
    );
  }

  static inboxIdFor(
    identity: PublicIdentity,
    backend: BackendSource,
  ): Promise<InboxId> {
    const projection = currentProjection();
    return HostClient.inboxIdFor(
      lowerPublicIdentity(identity, projection),
      lowerBackendSource(backend, projection),
    );
  }

  static async inboxStates(
    ids: InboxId[],
    backend: BackendSource,
  ): Promise<InboxState[]> {
    const projection = currentProjection();
    const states = await HostClient.inboxStates(
      ids,
      lowerBackendSource(backend, projection),
    );
    return states.map((state) => liftInboxState(state, projection));
  }

  static async keyPackageStatuses(
    ids: InstallationId[],
    backend: BackendSource,
  ): Promise<Map<string, KeyPackageStatus>> {
    const projection = currentProjection();
    const statuses = await HostClient.keyPackageStatuses(
      ids,
      lowerBackendSource(backend, projection),
    );
    return new Map(
      [...statuses].map(([id, status]) => [
        id,
        liftKeyPackageStatus(status, projection),
      ]),
    );
  }

  static async newestMessageMetadata(
    ids: ConversationId[],
    backend: BackendSource,
  ): Promise<Map<string, MessageMetadataEntry>> {
    const projection = currentProjection();
    const entries = await HostClient.newestMessageMetadata(
      ids,
      lowerBackendSource(backend, projection),
    );
    return new Map(
      [...entries].map(([id, entry]) => [
        id,
        liftMessageMetadataEntry(entry, projection),
      ]),
    );
  }

  static revokeInstallations(
    signer: Signer,
    inboxId: InboxId,
    ids: InstallationId[],
    backend: BackendSource,
  ): Promise<void> {
    const projection = currentProjection();
    return HostClient.revokeInstallations(
      lowerSigner(signer, projection),
      inboxId,
      ids,
      lowerBackendSource(backend, projection),
    );
  }

  static isAddressAuthorized(
    inboxId: InboxId,
    address: string,
    backend: BackendSource,
  ): Promise<boolean> {
    return HostClient.isAddressAuthorized(
      inboxId,
      address,
      lowerBackendSource(backend, currentProjection()),
    );
  }

  static isInstallationAuthorized(
    inboxId: InboxId,
    installationId: InstallationId,
    backend: BackendSource,
  ): Promise<boolean> {
    return HostClient.isInstallationAuthorized(
      inboxId,
      installationId,
      lowerBackendSource(backend, currentProjection()),
    );
  }

  static verifySignedWithPublicKey(
    text: string,
    signature: Uint8Array,
    publicKey: Uint8Array,
  ): Promise<boolean> {
    return HostClient.verifySignedWithPublicKey(
      text,
      Uint8Array.from(signature).buffer,
      Uint8Array.from(publicKey).buffer,
    );
  }

  /** End the client. Messages from it then fail with `ClientClosed`. */
  end(): Promise<void> {
    return hostOf(this).end();
  }
}
