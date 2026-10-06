import { ClientForwarders, type ClientBinding } from "./client-forwarding.gen.js";
import type { HostClientOptions } from "./host-message.gen.js";
import { createInWorker } from "./package-session.gen.js";
import * as P from "./proxy.gen.js";
import { EventStream } from "./runtime/events/reader.js";
import { openStorageAdmin, type StorageAdmin } from "./storage-admin.gen.js";
import * as B from "./xmtp_sdk.js";

// The worker proxy stays private to the package. Each proxy has at most one
// public Client, so a Message resolves to the Client the app holds.
const bindings = new WeakMap<Client, P.Client>();
const clients = new WeakMap<P.Client, WeakRef<Client>>();

let construct!: (raw: P.Client) => Client;

/**
 * The public Client of a worker proxy. A Message uses this to return the
 * Client that the app holds. The package root does not export it.
 */
export function wrapClient(raw: P.Client): Client {
  return clients.get(raw)?.deref() ?? construct(raw);
}

/** The worker proxy of a public Client. The package root does not export it. */
export function bindingClient(client: Client): P.Client {
  return bindingOf(client);
}

function bindingOf(client: Client): P.Client {
  const raw = bindings.get(client);
  if (raw === undefined) throw new TypeError("not an SDK client");
  return raw;
}

/**
 * The browser package Client. Package initialization owns the worker session,
 * so apps create and use a Client without a session or a worker handle.
 */
export class Client extends ClientForwarders {
  private readonly listeners = new Map<bigint, { stopped: boolean }>();
  private readonly pendingListeners = new Set<{ stopped: boolean }>();

  static {
    construct = (raw) => new Client(raw);
  }

  private constructor(raw: P.Client) {
    super();
    bindings.set(this, raw);
    clients.set(raw, new WeakRef(this));
  }

  protected binding(): ClientBinding {
    return bindingOf(this);
  }

  static async create(
    signer: B.Signer,
    options: HostClientOptions,
  ): Promise<Client> {
    return wrapClient(
      await createInWorker((session) =>
        P.Client.create(session, signer, options),
      ),
    );
  }

  static async build(
    identity: B.PublicIdentity,
    options: HostClientOptions,
    inboxId?: B.InboxId,
  ): Promise<Client> {
    return wrapClient(
      await createInWorker((session) =>
        P.Client.build(session, identity, options, inboxId),
      ),
    );
  }

  async events(filter: B.EventFilter): Promise<EventStream> {
    return new EventStream(await bindingOf(this).events(filter));
  }

  async startListener(
    filter: B.EventFilter,
    callback: (event: B.ClientEvent) => void | Promise<void>,
  ): Promise<bigint> {
    const gate = { stopped: false };
    this.pendingListeners.add(gate);
    try {
      const id = await bindingOf(this).startListener(filter, {
        async onEvent(event: B.ClientEvent): Promise<void> {
          if (gate.stopped) return;
          try {
            await callback(event);
          } catch {
            throw B.ListenerError.Failed.new();
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
    return bindingOf(this).stopListener(id);
  }

  storage(): B.StorageLike {
    return bindingOf(this).storage();
  }

  async end(): Promise<void> {
    for (const gate of this.listeners.values()) gate.stopped = true;
    for (const gate of this.pendingListeners) gate.stopped = true;
    this.listeners.clear();
    await bindingOf(this).end();
  }
}

/**
 * Browser storage. `Storage.admin()` opens an independent lease on the
 * package storage worker, and needs no Client. `Client.storage()` returns
 * the storage of one Client.
 */
export abstract class Storage implements B.StorageLike {
  abstract path(asyncOpts_?: {
    signal: AbortSignal;
  }): Promise<string | undefined>;

  static admin(): Promise<StorageAdmin> {
    return openStorageAdmin();
  }
}
