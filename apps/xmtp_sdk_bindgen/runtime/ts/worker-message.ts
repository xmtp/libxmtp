import type { MessageData } from "../xmtp_sdk";

/** The WASM converter lifts messages as data. Host actions use the bridge. */
export class Message {
  constructor(readonly data: MessageData) {}

  get id(): MessageData["id"] {
    return this.data.id;
  }

  get kind(): MessageData["kind"] {
    return this.data.kind;
  }

  get deliveryStatus(): MessageData["deliveryStatus"] {
    return this.data.deliveryStatus;
  }

  get encoded(): MessageData["encoded"] {
    return this.data.encoded;
  }
}
