import { ClientForwarders } from "../client-forwarding.gen";
import {
  Client as RawClient,
  StorageLocation,
  StorageLocation_Tags,
  type ClientLike,
  type ClientOptions,
  type ClientEvent,
  type EventFilter,
  ListenerError,
  type EncodedContent,
  type InboxId,
  type PublicIdentity,
  type Signer,
} from "../xmtp_sdk";
import {
  codecKey,
  decodeCustom,
  type AnyCodec,
  type DecodedCustom,
} from "./custom-codec";
import { EventStream } from "./events/reader";
export type { ContentCodec } from "./codec-type";

declare const process: { cwd(): string } | undefined;

export type SDKClientOptions = ClientOptions & {
  codecs?: readonly AnyCodec[];
};

class CodecRegistry {
  private readonly codecs: ReadonlyMap<string, AnyCodec>;

  constructor(codecs: readonly AnyCodec[]) {
    this.codecs = new Map(codecs.map((codec) => [codecKey(codec.type), codec]));
  }

  decode(encoded: EncodedContent): DecodedCustom | undefined {
    return decodeCustom(this.codecs, encoded);
  }
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
      location: new StorageLocation.Directory({ directory }),
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

// The binding Client stays private to the runtime. Runtime modules and
// conformance read it through `bindingClient`; the package root does not
// export that function.
const bindings = new WeakMap<Client, ClientLike>();

export function bindingClient(client: Client): ClientLike {
  const raw = bindings.get(client);
  if (raw === undefined) throw new TypeError("not an SDK client");
  return raw;
}

export class Client extends ClientForwarders {
  private readonly key: bigint;
  private readonly listeners = new Map<bigint, { stopped: boolean }>();
  private readonly pendingListeners = new Set<{ stopped: boolean }>();
  private readonly codecs: CodecRegistry;

  private constructor(raw: ClientLike, codecs: readonly AnyCodec[]) {
    super();
    bindings.set(this, raw);
    this.key = raw.clientKey();
    this.codecs = new CodecRegistry(codecs);
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
    inboxId?: InboxId,
  ): Promise<Client> {
    const { codecs = [], ...rustOptions } = options;
    return new Client(
      await RawClient.build(identity, resolvedOptions(rustOptions), inboxId),
      codecs,
    );
  }

  protected binding(): ClientLike {
    return bindingClient(this);
  }

  async events(filter: EventFilter): Promise<EventStream> {
    return new EventStream(await this.binding().events(filter));
  }

  async startListener(
    filter: EventFilter,
    callback: (event: ClientEvent) => void | Promise<void>,
  ): Promise<bigint> {
    const gate = { stopped: false };
    this.pendingListeners.add(gate);
    try {
      const id = await this.binding().startListener(filter, {
        async onEvent(event: ClientEvent): Promise<void> {
          if (gate.stopped) return;
          try {
            await callback(event);
          } catch {
            throw new ListenerError.Failed();
          }
        },
      });
      this.listeners.set(id, gate);
      return id;
    } finally {
      this.pendingListeners.delete(gate);
    }
  }

  stopListener(id: bigint): Promise<void> {
    const gate = this.listeners.get(id);
    if (gate) gate.stopped = true;
    this.listeners.delete(id);
    return this.binding().stopListener(id);
  }

  storage() {
    return this.binding().storage();
  }

  decodeCustom(encoded: EncodedContent): DecodedCustom | undefined {
    return this.codecs.decode(encoded);
  }

  async end(): Promise<void> {
    for (const gate of this.listeners.values()) gate.stopped = true;
    for (const gate of this.pendingListeners) gate.stopped = true;
    this.listeners.clear();
    try {
      await this.binding().end();
    } finally {
      ClientRegistry.delete(this.key);
    }
  }
}
