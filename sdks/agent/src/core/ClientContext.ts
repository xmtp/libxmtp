import type { Client } from "@xmtp/node-sdk";

/** Context emitted for agent lifecycle events and supplied to error handlers. */
export class ClientContext<_ContentTypes = unknown> {
  #client: Client;

  /** Create a context for a client. */
  constructor({
    client,
  }: {
    /** The client that emitted the event. */
    client: Client;
  }) {
    this.#client = client;
  }

  /** Return the account identifier used by the client, when available. */
  getClientAddress() {
    return this.#client.identity.identifier;
  }

  /** Return the wrapped XMTP client. */
  get client() {
    return this.#client;
  }
}
