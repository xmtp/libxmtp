import { Group, Stack, Text, Tooltip } from "@mantine/core";
import type { MessageContent as Content } from "@xmtp/browser-sdk";

import { MessageContent } from "@/components/Messages/MessageContent";
import type { MessageContentAlign } from "@/components/Messages/MessageContentWrapper";

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
}) => (
  <Stack gap="xs" align={align === "left" ? "flex-start" : "flex-end"}>
    <Group gap={4}>
      <Text size="xs">Replied to a</Text>
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
