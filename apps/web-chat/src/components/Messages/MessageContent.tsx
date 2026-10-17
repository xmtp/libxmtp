import { Code } from "@mantine/core";
import type { MessageBody, MessageContent as Content } from "@xmtp/browser-sdk";

import { ActionsContent } from "@/components/Messages/ActionsContent";
import { FallbackContent } from "@/components/Messages/FallbackContent";
import { MarkdownContent } from "@/components/Messages/MarkdownContent";
import type { MessageContentAlign } from "@/components/Messages/MessageContentWrapper";
import { RemoteAttachmentContent } from "@/components/Messages/RemoteAttachmentContent";
import { ReplyContent } from "@/components/Messages/ReplyContent";
import { TextContent } from "@/components/Messages/TextContent";
import { TransactionReferenceContent } from "@/components/Messages/TransactionReferenceContent";
import { WalletSendCallsContent } from "@/components/Messages/WalletSendCallsContent";
import { jsonStringify } from "@/helpers/strings";

export type MessageContentProps = {
  align: MessageContentAlign;
  scrollToMessage: (id: string) => void;
  content: Content | MessageBody;
  fallback?: string;
};
export const MessageContent: React.FC<MessageContentProps> = ({
  content,
  align,
  scrollToMessage,
  fallback,
}) => {
  switch (content.kind) {
    case "transactionReference":
      return <TransactionReferenceContent content={content.value} />;
    case "walletSendCalls":
      return <WalletSendCallsContent content={content.value} />;
    case "reply":
      return (
        <ReplyContent
          align={align}
          reply={content}
          scrollToMessage={scrollToMessage}
        />
      );
    case "remoteAttachment":
      return <RemoteAttachmentContent align={align} content={content.value} />;
    case "actions":
      return <ActionsContent content={content.value} />;
    case "markdown":
      return <MarkdownContent content={content.value} />;
    case "text":
      return <TextContent text={content.value} />;
    default:
      if (fallback !== undefined) return <FallbackContent text={fallback} />;
      return (
        <Code
          block
          w="100%"
          style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
          {jsonStringify(content)}
        </Code>
      );
  }
};
