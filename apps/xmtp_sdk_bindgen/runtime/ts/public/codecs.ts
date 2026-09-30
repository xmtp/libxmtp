import {
  currentProjection,
  liftActions,
  liftAttachment,
  liftContentTypeId,
  liftEncodedContent,
  liftGroupUpdated,
  liftIntent,
  liftLeaveRequest,
  liftMultiRemoteAttachment,
  liftRemoteAttachment,
  liftStandardContent,
  liftTransactionReference,
  liftWalletSendCalls,
  lowerActions,
  lowerAttachment,
  lowerEncodedContent,
  lowerGroupUpdated,
  lowerIntent,
  lowerLeaveRequest,
  lowerMultiRemoteAttachment,
  lowerRemoteAttachment,
  lowerStandardContent,
  lowerTransactionReference,
  lowerWalletSendCalls,
  publicError,
  type Actions,
  type Attachment,
  type ContentTypeId,
  type EncodedContent,
  type GroupUpdated,
  type Intent,
  type LeaveRequest,
  type MultiRemoteAttachment,
  type ObjectProjection,
  type RemoteAttachment,
  type StandardContent,
  type TransactionReference,
  type WalletSendCalls,
} from "../../public-values.gen";
import type { ContentCodec as HostContentCodec } from "../codec-type";
// Standalone standard codecs over public values. Each one wraps the host
// codec, which encodes and decodes in Rust.
import {
  ActionsCodec as HostActions,
  AttachmentCodec as HostAttachment,
  DeleteMessageCodec as HostDeleteMessage,
  GroupUpdatedCodec as HostGroupUpdated,
  IntentCodec as HostIntent,
  LeaveRequestCodec as HostLeaveRequest,
  MarkdownCodec as HostMarkdown,
  MultiRemoteAttachmentCodec as HostMultiRemoteAttachment,
  ReactionV2Codec as HostReaction,
  ReadReceiptCodec as HostReadReceipt,
  RemoteAttachmentCodec as HostRemoteAttachment,
  ReplyCodec as HostReply,
  TextCodec as HostText,
  TransactionReferenceCodec as HostTransactionReference,
  WalletSendCallsCodec as HostWalletSendCalls,
} from "../codecs";
import type { ContentCodec } from "./codec";

type Convert<From, To> = (value: From, projection: ObjectProjection) => To;
const same = <T>(value: T): T => value;

abstract class StandardCodec<Value, Host> implements ContentCodec<Value> {
  readonly type: ContentTypeId;

  protected constructor(
    private readonly host: HostContentCodec<Host>,
    private readonly lower: Convert<Value, Host>,
    private readonly lift: Convert<Host, Value>,
  ) {
    this.type = liftContentTypeId(host.type, currentProjection());
  }

  encode(value: Value): EncodedContent {
    const projection = currentProjection();
    try {
      return liftEncodedContent(
        this.host.encode(this.lower(value, projection)),
        projection,
      );
    } catch (error) {
      throw publicError(error);
    }
  }

  decode(encoded: EncodedContent): Value {
    const projection = currentProjection();
    try {
      return this.lift(
        this.host.decode(lowerEncodedContent(encoded, projection)),
        projection,
      );
    } catch (error) {
      throw publicError(error);
    }
  }
}

export class TextCodec extends StandardCodec<string, string> {
  constructor() {
    super(new HostText(), same, same);
  }
}

export class MarkdownCodec extends StandardCodec<string, string> {
  constructor() {
    super(new HostMarkdown(), same, same);
  }
}

export class ReadReceiptCodec extends StandardCodec<void, void> {
  constructor() {
    super(new HostReadReceipt(), same, same);
  }
}

export class ReactionV2Codec extends StandardCodec<
  StandardContent,
  ReturnType<typeof lowerStandardContent>
> {
  constructor() {
    super(new HostReaction(), lowerStandardContent, liftStandardContent);
  }
}

export class ReplyCodec extends StandardCodec<
  StandardContent,
  ReturnType<typeof lowerStandardContent>
> {
  constructor() {
    super(new HostReply(), lowerStandardContent, liftStandardContent);
  }
}

export class DeleteMessageCodec extends StandardCodec<
  StandardContent,
  ReturnType<typeof lowerStandardContent>
> {
  constructor() {
    super(new HostDeleteMessage(), lowerStandardContent, liftStandardContent);
  }
}

export class AttachmentCodec extends StandardCodec<
  Attachment,
  ReturnType<typeof lowerAttachment>
> {
  constructor() {
    super(new HostAttachment(), lowerAttachment, liftAttachment);
  }
}

export class RemoteAttachmentCodec extends StandardCodec<
  RemoteAttachment,
  ReturnType<typeof lowerRemoteAttachment>
> {
  constructor() {
    super(
      new HostRemoteAttachment(),
      lowerRemoteAttachment,
      liftRemoteAttachment,
    );
  }
}

export class MultiRemoteAttachmentCodec extends StandardCodec<
  MultiRemoteAttachment,
  ReturnType<typeof lowerMultiRemoteAttachment>
> {
  constructor() {
    super(
      new HostMultiRemoteAttachment(),
      lowerMultiRemoteAttachment,
      liftMultiRemoteAttachment,
    );
  }
}

export class TransactionReferenceCodec extends StandardCodec<
  TransactionReference,
  ReturnType<typeof lowerTransactionReference>
> {
  constructor() {
    super(
      new HostTransactionReference(),
      lowerTransactionReference,
      liftTransactionReference,
    );
  }
}

export class WalletSendCallsCodec extends StandardCodec<
  WalletSendCalls,
  ReturnType<typeof lowerWalletSendCalls>
> {
  constructor() {
    super(new HostWalletSendCalls(), lowerWalletSendCalls, liftWalletSendCalls);
  }
}

export class ActionsCodec extends StandardCodec<
  Actions,
  ReturnType<typeof lowerActions>
> {
  constructor() {
    super(new HostActions(), lowerActions, liftActions);
  }
}

export class IntentCodec extends StandardCodec<
  Intent,
  ReturnType<typeof lowerIntent>
> {
  constructor() {
    super(new HostIntent(), lowerIntent, liftIntent);
  }
}

export class GroupUpdatedCodec extends StandardCodec<
  GroupUpdated,
  ReturnType<typeof lowerGroupUpdated>
> {
  constructor() {
    super(new HostGroupUpdated(), lowerGroupUpdated, liftGroupUpdated);
  }
}

export class LeaveRequestCodec extends StandardCodec<
  LeaveRequest,
  ReturnType<typeof lowerLeaveRequest>
> {
  constructor() {
    super(new HostLeaveRequest(), lowerLeaveRequest, liftLeaveRequest);
  }
}
