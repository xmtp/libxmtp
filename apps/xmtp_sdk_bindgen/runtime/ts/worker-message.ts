import type {
  DeliveryStatus,
  EncodedContent,
  MessageData,
  MessageKind,
  MessageId,
} from "../xmtp_sdk";

/** The WASM converter lifts messages as data. Host actions use the bridge. */
export class Message {
  constructor(readonly data: MessageData) {}

  get deliveryCursor(): string | null {
    return this.data.deliveryCursor ?? null;
  }

  get id(): MessageId {
    return this.data.id;
  }

  get kind(): MessageKind {
    return this.data.kind;
  }

  get deliveryStatus(): DeliveryStatus {
    return this.data.deliveryStatus;
  }

  get encoded(): EncodedContent | undefined {
    return this.data.encoded;
  }
}
