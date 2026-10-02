import {
  type Client,
  type Conversation,
  type Dm,
  type Group,
} from "@xmtp/node-sdk";

import { filter } from "@/core/filter";
import {
  createRemoteAttachmentFromFile,
  type AttachmentUploadCallback,
} from "@/util/AttachmentUtil";

import { ClientContext } from "./ClientContext";

/** Context for a conversation event and its client. */
export class ConversationContext<
  ContentTypes = unknown,
  ConversationType extends Conversation = Conversation,
> extends ClientContext<ContentTypes> {
  #conversation: ConversationType;

  /** Create a context for a conversation. */
  constructor({
    conversation,
    client,
  }: {
    /** The conversation that emitted the event. */
    conversation: ConversationType;
    /** The client that owns the conversation. */
    client: Client;
  }) {
    super({ client });
    this.#conversation = conversation;
  }

  /** Narrow this context to a direct-message conversation. */
  isDm(): this is ConversationContext<ContentTypes, Dm> {
    return filter.isDM(this.#conversation);
  }

  /** Narrow this context to a group conversation. */
  isGroup(): this is ConversationContext<ContentTypes, Group> {
    return filter.isGroup(this.#conversation);
  }

  /** Encrypt and send a remote attachment through the supplied upload callback. */
  async sendRemoteAttachment(
    unencryptedFile: File,
    uploadCallback?: AttachmentUploadCallback,
  ): Promise<void> {
    const remoteAttachment = await createRemoteAttachmentFromFile(
      this.client,
      unencryptedFile,
      uploadCallback,
    );
    await this.#conversation.sendRemoteAttachment(remoteAttachment, {
      shouldPush: false,
    });
  }

  /** Return the conversation that triggered this context. */
  get conversation() {
    return this.#conversation;
  }

  /** Return the conversation consent state. */
  async consentState() {
    const state = await this.#conversation.state();
    return "common" in state ? state.common.consentState : state.consentState;
  }

  /** Whether the conversation consent state is `allowed`. */
  get isAllowed() {
    return this.consentState().then((state) => state === "allowed");
  }

  /** Whether the conversation consent state is `denied`. */
  get isDenied() {
    return this.consentState().then((state) => state === "denied");
  }

  /** Whether the conversation consent state is `unknown`. */
  get isUnknown() {
    return this.consentState().then((state) => state === "unknown");
  }
}
