import {
  ConsentState,
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
    client: Client<ContentTypes>;
  }) {
    super({ client });
    this.#conversation = conversation;
  }

  /** Narrow this context to a direct-message conversation. */
  isDm(): this is ConversationContext<ContentTypes, Dm<ContentTypes>> {
    return filter.isDM(this.#conversation);
  }

  /** Narrow this context to a group conversation. */
  isGroup(): this is ConversationContext<ContentTypes, Group<ContentTypes>> {
    return filter.isGroup(this.#conversation);
  }

  /** Encrypt and send a remote attachment through the supplied upload callback. */
  async sendRemoteAttachment(
    unencryptedFile: File,
    uploadCallback: AttachmentUploadCallback,
  ): Promise<void> {
    const remoteAttachment = await createRemoteAttachmentFromFile(
      unencryptedFile,
      uploadCallback,
    );
    await this.#conversation.sendRemoteAttachment(remoteAttachment);
  }

  /** Return the conversation that triggered this context. */
  get conversation() {
    return this.#conversation;
  }

  /** Whether the conversation consent state is `allowed`. */
  get isAllowed() {
    return this.#conversation.consentState() === ConsentState.Allowed;
  }

  /** Whether the conversation consent state is `denied`. */
  get isDenied() {
    return this.#conversation.consentState() === ConsentState.Denied;
  }

  /** Whether the conversation consent state is `unknown`. */
  get isUnknown() {
    return this.#conversation.consentState() === ConsentState.Unknown;
  }
}
