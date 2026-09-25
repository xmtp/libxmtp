import {
  StandardContent,
  StandardContentKind,
  StandardContent_Tags,
  decodeStandard,
  encodeStandard,
  standardContentType,
  type ContentTypeID,
  type EncodedContent,
  type Attachment,
  type RemoteAttachment,
  type MultiRemoteAttachment,
  type TransactionReference,
  type WalletSendCalls,
  type Actions,
  type Intent,
  type GroupUpdated,
  type LeaveRequest,
} from "../xmtp_sdk";
import type { ContentCodec } from "./client";

function wrongValue(): never {
  throw new TypeError("wrong standard codec value");
}

function tupleValue<T>(value: StandardContent, tag: StandardContent_Tags): T {
  if (value.tag !== tag) wrongValue();
  return (value as unknown as { inner: readonly [T] }).inner[0];
}

function matchingValue(
  value: StandardContent,
  tag: StandardContent_Tags,
): StandardContent {
  if (value.tag !== tag) wrongValue();
  return value;
}

abstract class PureCodec<T> implements ContentCodec<T> {
  readonly type: ContentTypeID;

  protected constructor(
    kind: StandardContentKind,
    private readonly wrap: (value: T) => StandardContent,
    private readonly take: (value: StandardContent) => T,
  ) {
    this.type = standardContentType(kind);
  }

  encode(value: T): EncodedContent {
    return encodeStandard(this.wrap(value));
  }

  decode(encoded: EncodedContent): T {
    return this.take(decodeStandard(encoded));
  }
}

export class TextCodec extends PureCodec<string> {
  constructor() {
    super(
      StandardContentKind.Text,
      (value) => new StandardContent.Text(value),
      (value) => tupleValue(value, StandardContent_Tags.Text),
    );
  }
}

export class MarkdownCodec extends PureCodec<string> {
  constructor() {
    super(
      StandardContentKind.Markdown,
      (value) => new StandardContent.Markdown(value),
      (value) => tupleValue(value, StandardContent_Tags.Markdown),
    );
  }
}

export class ReadReceiptCodec extends PureCodec<void> {
  constructor() {
    super(
      StandardContentKind.ReadReceipt,
      () => new StandardContent.ReadReceipt(),
      (value) => {
        matchingValue(value, StandardContent_Tags.ReadReceipt);
      },
    );
  }
}

export class ReactionV2Codec extends PureCodec<StandardContent> {
  constructor() {
    super(
      StandardContentKind.Reaction,
      (value) => matchingValue(value, StandardContent_Tags.Reaction),
      (value) => matchingValue(value, StandardContent_Tags.Reaction),
    );
  }
}

export class AttachmentCodec extends PureCodec<Attachment> {
  constructor() {
    super(
      StandardContentKind.Attachment,
      (value) => new StandardContent.Attachment(value),
      (value) => tupleValue(value, StandardContent_Tags.Attachment),
    );
  }
}

export class RemoteAttachmentCodec extends PureCodec<RemoteAttachment> {
  constructor() {
    super(
      StandardContentKind.RemoteAttachment,
      (value) => new StandardContent.RemoteAttachment(value),
      (value) => tupleValue(value, StandardContent_Tags.RemoteAttachment),
    );
  }
}

export class MultiRemoteAttachmentCodec extends PureCodec<MultiRemoteAttachment> {
  constructor() {
    super(
      StandardContentKind.MultiRemoteAttachment,
      (value) => new StandardContent.MultiRemoteAttachment(value),
      (value) => tupleValue(value, StandardContent_Tags.MultiRemoteAttachment),
    );
  }
}

export class TransactionReferenceCodec extends PureCodec<TransactionReference> {
  constructor() {
    super(
      StandardContentKind.TransactionReference,
      (value) => new StandardContent.TransactionReference(value),
      (value) => tupleValue(value, StandardContent_Tags.TransactionReference),
    );
  }
}

export class WalletSendCallsCodec extends PureCodec<WalletSendCalls> {
  constructor() {
    super(
      StandardContentKind.WalletSendCalls,
      (value) => new StandardContent.WalletSendCalls(value),
      (value) => tupleValue(value, StandardContent_Tags.WalletSendCalls),
    );
  }
}

export class ActionsCodec extends PureCodec<Actions> {
  constructor() {
    super(
      StandardContentKind.Actions,
      (value) => new StandardContent.Actions(value),
      (value) => tupleValue(value, StandardContent_Tags.Actions),
    );
  }
}

export class IntentCodec extends PureCodec<Intent> {
  constructor() {
    super(
      StandardContentKind.Intent,
      (value) => new StandardContent.Intent(value),
      (value) => tupleValue(value, StandardContent_Tags.Intent),
    );
  }
}

export class ReplyCodec extends PureCodec<StandardContent> {
  constructor() {
    super(
      StandardContentKind.Reply,
      (value) => matchingValue(value, StandardContent_Tags.Reply),
      (value) => matchingValue(value, StandardContent_Tags.Reply),
    );
  }
}

export class GroupUpdatedCodec extends PureCodec<GroupUpdated> {
  constructor() {
    super(
      StandardContentKind.GroupUpdated,
      (value) => new StandardContent.GroupUpdated(value),
      (value) => tupleValue(value, StandardContent_Tags.GroupUpdated),
    );
  }
}

export class DeleteMessageCodec extends PureCodec<StandardContent> {
  constructor() {
    super(
      StandardContentKind.DeleteMessage,
      (value) => matchingValue(value, StandardContent_Tags.DeleteMessage),
      (value) => matchingValue(value, StandardContent_Tags.DeleteMessage),
    );
  }
}

export class LeaveRequestCodec extends PureCodec<LeaveRequest> {
  constructor() {
    super(
      StandardContentKind.LeaveRequest,
      (value) => new StandardContent.LeaveRequest(value),
      (value) => tupleValue(value, StandardContent_Tags.LeaveRequest),
    );
  }
}
