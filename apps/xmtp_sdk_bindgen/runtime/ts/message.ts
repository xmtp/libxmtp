import { ErrorCategory, XmtpError, type MessageData } from "../xmtp_sdk";
import { ClientRegistry, type Client } from "./client";

export class Message {
  constructor(readonly data: MessageData) {}

  get id() {
    return this.data.id;
  }

  get conversationID() {
    return this.data.conversationID;
  }

  get senderInboxID() {
    return this.data.senderInboxID;
  }

  get sentAt() {
    return this.data.sentAt;
  }

  get kind() {
    return this.data.kind;
  }

  get deliveryStatus() {
    return this.data.deliveryStatus;
  }

  get contentType() {
    return this.data.contentType;
  }

  get fallback() {
    return this.data.fallback;
  }

  get content() {
    return this.data.content;
  }

  client(): Client {
    const client = ClientRegistry.get(this.data.clientKey);
    if (client === undefined)
      throw new XmtpError.ClientClosed({
        code: "ClientClosed",
        category: ErrorCategory.Lifecycle,
        retryable: false,
        message: "client is closed",
      });
    return client;
  }
}
