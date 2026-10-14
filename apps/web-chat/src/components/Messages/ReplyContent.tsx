import { Badge, Group, Stack, Text, Tooltip } from "@mantine/core";
import { Dm, type MessageContent as Content } from "@xmtp/browser-sdk";

import { Identity } from "@/components/Identity";
import { MessageContent } from "@/components/Messages/MessageContent";
import type { MessageContentAlign } from "@/components/Messages/MessageContentWrapper";
import { useConversationContext } from "@/contexts/ConversationContext";
import { getMemberAddress } from "@/helpers/xmtp";
import { useConversation } from "@/hooks/useConversation";

import classes from "./ReplyContent.module.css";
export type ReplyContentProps = {
  align: MessageContentAlign;
  reply: Extract<Content, { kind: "reply" }>;
  scrollToMessage: (id: string) => void;
};
export const ReplyContent: React.FC<ReplyContentProps> = ({
  align,
  reply,
  scrollToMessage,
}) => {
  const { conversationId } = useConversationContext();
  const { conversation, messages, members } = useConversation(conversationId);
  const original = messages.find((message) => message.id === reply.referenceId);
  const sender = original ? members.get(original.senderInboxId) : undefined;
  return (
    <Stack gap="xs" align={align === "left" ? "flex-start" : "flex-end"}>
      <Group gap={4}>
        <Text size="xs">{original ? "Replied to" : "Replied to a"}</Text>
        {original &&
          (sender ? (
            <Identity
              address={getMemberAddress(sender)}
              avatar={null}
              conversationId={conversationId}
              description={null}
              displayName={sender.inboxId}
              handle={sender.handle ?? null}
              otherHandles={sender.otherHandles}
              inboxId={sender.inboxId}
              permissionLevel={sender.permissionLevel}
              showDm={!(conversation instanceof Dm)}
              position="top"
              size="sm"
            />
          ) : (
            <Badge
              radius="md"
              variant="default"
              size="sm"
              styles={{ label: { textTransform: "none" } }}>
              {original.senderInboxId}
            </Badge>
          ))}
        <Tooltip label="Click to scroll to the original message">
          <Text
            size="xs"
            className={classes.text}
            onClick={() => scrollToMessage(reply.referenceId)}>
            message
          </Text>
        </Tooltip>
      </Group>
      <MessageContent
        content={reply.body}
        align={align}
        scrollToMessage={scrollToMessage}
      />
    </Stack>
  );
};
