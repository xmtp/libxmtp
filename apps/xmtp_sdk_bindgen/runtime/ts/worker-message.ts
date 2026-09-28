import type {
  DeliveryStatus,
  EncodedContent,
  MessageData,
  MessageKind,
} from "../xmtp_sdk";
import type { MessageID } from "./ids";

/** The WASM converter lifts messages as data. Host actions use the bridge. */
export class Message {
  constructor(readonly data: MessageData) {}

  get id(): MessageID {
    return this.data.id;
  }

  get kind(): MessageKind {
    return this.data.kind;
  }

  get deliveryStatus(): DeliveryStatus {
    return this.data.deliveryStatus;
  }

  get encoded(): EncodedContent {
    return this.data.encoded;
  }
}
