import type { Message as XmtpMessage } from "@xmtp/browser-sdk";

import { GroupUpdatedContent } from "@/components/Messages/GroupUpdatedContent";
import { IntentContent } from "@/components/Messages/IntentContent";
import { MessageContent } from "@/components/Messages/MessageContent";
import {
  MessageContentWrapper,
  type MessageContentAlign,
} from "@/components/Messages/MessageContentWrapper";

export type MessageContentWithWrapperProps = {
  align: MessageContentAlign;
  senderInboxId: string;
  message: XmtpMessage;
  scrollToMessage: (id: string) => void;
};

export const MessageContentWithWrapper: React.FC<
  MessageContentWithWrapperProps
> = ({ message, align, senderInboxId, scrollToMessage }) => {
  if (message.content.kind === "groupUpdated") {
    return (
      <GroupUpdatedContent
        content={message.content.value}
        sentAtNs={message.sentAt.ns}
      />
    );
  }

  if (message.content.kind === "intent") {
    return (
      <IntentContent
        content={message.content.value}
        sentAtNs={message.sentAt.ns}
        senderInboxId={senderInboxId}
      />
    );
  }

  return (
    <MessageContentWrapper
      align={align}
      senderInboxId={senderInboxId}
      sentAtNs={message.sentAt.ns}>
      <MessageContent
        content={message.content}
        fallback={message.fallback}
        align={align}
        scrollToMessage={scrollToMessage}
      />
    </MessageContentWrapper>
  );
};
