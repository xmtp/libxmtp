import { Box, Text } from "@mantine/core";
import type { Message as XmtpMessage } from "@xmtp/browser-sdk";

import { MessageList } from "./MessageList";

export type ConversationProps = {
  messages: XmtpMessage[];
};

export const Messages: React.FC<ConversationProps> = ({ messages }) => {
  return messages.length === 0 ? (
    <Box
      display="flex"
      style={{
        flexGrow: 1,
        alignItems: "center",
        justifyContent: "center",
      }}>
      <Text>No messages</Text>
    </Box>
  ) : (
    <MessageList messages={messages} />
  );
};
