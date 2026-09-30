// Type view for linting the maintained runtime before generation. The
// generator writes the real forwarders from the exported Client methods.
import type { ClientLike } from "./xmtp_sdk";

export abstract class ClientForwarders {
  protected abstract binding(): ClientLike;

  inboxId(): ReturnType<ClientLike["inboxId"]> {
    return this.binding().inboxId();
  }

  installationId(): ReturnType<ClientLike["installationId"]> {
    return this.binding().installationId();
  }

  conversations(): ReturnType<ClientLike["conversations"]> {
    return this.binding().conversations();
  }
}
