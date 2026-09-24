import {
  Client as RawClient,
  StorageLocation,
  StorageLocation_Tags,
  type ClientLike,
  type ClientOptions,
  type PublicIdentity,
  type Signer,
} from "../xmtp_sdk";
import type { InboxID, InstallationID } from "./ids";

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
