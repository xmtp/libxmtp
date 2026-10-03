import {
  ClientMembers,
  attachClientBinding,
  currentProjection,
  liftClientEvent,
  liftEncodedContent,
  liftInboxState,
  liftKeyPackageStatus,
  liftMessageMetadataEntry,
  liftServerConfiguration,
  lowerBackendSource,
  lowerClientOptions,
  lowerContentTypeId,
  lowerEncodedContent,
  lowerEventFilter,
  lowerPublicIdentity,
  lowerSigner,
  publicError,
  type BackendSource,
  type ClientEvent,
  type ClientOptions as ProjectedClientOptions,
  type ConversationId,
  type EventFilter,
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
} from "../../xmtp_sdk";
import type { AnyContentCodec } from "./codec";
import { publicEventStream, type EventStream } from "./events";
import {
  HostClient,
  bindingClient,
  checkStorage,
  resolveLegacyStorage,
  type HostClientOptions,
} from "./host";

/** Client options with the custom codecs that this client decodes. */
export type ClientOptions = ProjectedClientOptions & {
  readonly codecs?: readonly AnyContentCodec[];
};

type HostCodec = {
  readonly type: BoundTypeId;
  encode(value: never): BoundEncoded;
  decode(encoded: BoundEncoded): unknown;
};

function hostCodec(
  codec: AnyContentCodec,
  projection: ObjectProjection,
): HostCodec {
  return {
    type: lowerContentTypeId(codec.type, projection),
    encode: (value) => lowerEncodedContent(codec.encode(value), projection),
    decode: (encoded) => codec.decode(liftEncodedContent(encoded, projection)),
  };
}

/** The host options of public Client options. Exported for conformance. */
export function hostOptions(
  options: ClientOptions,
  projection: ObjectProjection,
): HostClientOptions {
  checkStorage(options.storage);
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

/** Run a host call; a binding error leaves it as the public error. */
async function rethrow<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw publicError(error);
  }
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
      attachClientBinding(client, bindingClient(host));
      return client;
    };
  }

  private constructor() {
    super();
  }

  static async create(signer: Signer, options: ClientOptions): Promise<Client> {
    const projection = currentProjection();
    const host = await rethrow(async () =>
      HostClient.create(
        lowerSigner(signer, projection),
        hostOptions(
          await resolveLegacyStorage(options, () => signer.identity()),
          projection,
        ),
      ),
    );
    return publicClient(host);
  }

  static async build(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId?: InboxId,
  ): Promise<Client> {
    const projection = currentProjection();
    const host = await rethrow(async () =>
      HostClient.build(
        lowerPublicIdentity(identity, projection),
        hostOptions(
          await resolveLegacyStorage(options, () => Promise.resolve(identity), inboxId),
          projection,
        ),
        inboxId,
      ),
    );
    return publicClient(host);
  }

  static async fetchServerConfiguration(
    backend: BackendSource,
  ): Promise<ServerConfiguration> {
    const projection = currentProjection();
    const configuration = await rethrow(() =>
      HostClient.fetchServerConfiguration(
        lowerBackendSource(backend, projection),
      ),
    );
    return liftServerConfiguration(configuration, projection);
  }

  static canMessage(
    identities: PublicIdentity[],
    backend: BackendSource,
  ): Promise<Map<string, boolean>> {
    const projection = currentProjection();
    return rethrow(() =>
      HostClient.canMessage(
        identities.map((identity) => lowerPublicIdentity(identity, projection)),
        lowerBackendSource(backend, projection),
      ),
    );
  }

  static inboxIdFor(
    identity: PublicIdentity,
    backend: BackendSource,
  ): Promise<InboxId> {
    const projection = currentProjection();
    return rethrow(() =>
      HostClient.inboxIdFor(
        lowerPublicIdentity(identity, projection),
        lowerBackendSource(backend, projection),
      ),
    );
  }

  static async inboxStates(
    ids: InboxId[],
    backend: BackendSource,
  ): Promise<InboxState[]> {
    const projection = currentProjection();
    const states = await rethrow(() =>
      HostClient.inboxStates(ids, lowerBackendSource(backend, projection)),
    );
    return states.map((state) => liftInboxState(state, projection));
  }

  static async keyPackageStatuses(
    ids: InstallationId[],
    backend: BackendSource,
  ): Promise<Map<string, KeyPackageStatus>> {
    const projection = currentProjection();
    const statuses = await rethrow(() =>
      HostClient.keyPackageStatuses(
        ids,
        lowerBackendSource(backend, projection),
      ),
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
    const entries = await rethrow(() =>
      HostClient.newestMessageMetadata(
        ids,
        lowerBackendSource(backend, projection),
      ),
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
    return rethrow(() =>
      HostClient.revokeInstallations(
        lowerSigner(signer, projection),
        inboxId,
        ids,
        lowerBackendSource(backend, projection),
      ),
    );
  }

  static isAddressAuthorized(
    inboxId: InboxId,
    address: string,
    backend: BackendSource,
  ): Promise<boolean> {
    const projection = currentProjection();
    return rethrow(() =>
      HostClient.isAddressAuthorized(
        inboxId,
        address,
        lowerBackendSource(backend, projection),
      ),
    );
  }

  static isInstallationAuthorized(
    inboxId: InboxId,
    installationId: InstallationId,
    backend: BackendSource,
  ): Promise<boolean> {
    const projection = currentProjection();
    return rethrow(() =>
      HostClient.isInstallationAuthorized(
        inboxId,
        installationId,
        lowerBackendSource(backend, projection),
      ),
    );
  }

  static verifySignedWithPublicKey(
    text: string,
    signature: Uint8Array,
    publicKey: Uint8Array,
  ): Promise<boolean> {
    return rethrow(() =>
      HostClient.verifySignedWithPublicKey(
        text,
        Uint8Array.from(signature).buffer,
        Uint8Array.from(publicKey).buffer,
      ),
    );
  }

  /** A stream of the client events that `filter` selects. */
  async events(filter: EventFilter): Promise<EventStream> {
    const projection = currentProjection();
    const host = hostOf(this);
    return publicEventStream(
      await rethrow(() => host.events(lowerEventFilter(filter, projection))),
    );
  }

  /**
   * Call `callback` for each client event that `filter` selects. A callback
   * failure does not stop the listener.
   */
  startListener(
    filter: EventFilter,
    callback: (event: ClientEvent) => void | Promise<void>,
  ): Promise<bigint> {
    const projection = currentProjection();
    const host = hostOf(this);
    return rethrow(() =>
      host.startListener(lowerEventFilter(filter, projection), (event) =>
        callback(liftClientEvent(event, projection)),
      ),
    );
  }

  /** Stop a listener. Stopping an unknown or stopped listener does nothing. */
  stopListener(id: bigint): Promise<void> {
    const host = hostOf(this);
    return rethrow(() => host.stopListener(id));
  }

  /** End the client. Messages from it then fail with `ClientClosed`. */
  end(): Promise<void> {
    const host = hostOf(this);
    return rethrow(() => host.end());
  }
}
