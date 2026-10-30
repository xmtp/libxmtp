import { Box, List, Space, Text } from "@mantine/core";
import type { WalletSendCalls } from "@xmtp/browser-sdk";

export type WalletSendCallsContentProps = {
  content: WalletSendCalls;
};

const callFields = [
  ["to", "To"],
  ["value", "Value"],
  ["data", "Data"],
  ["gas", "Gas"],
] as const;

export const WalletSendCallsContent: React.FC<WalletSendCallsContentProps> = ({
  content,
}) => (
  <Box style={{ wordBreak: "break-word" }}>
    <Text size="sm">Transaction request from a conversation member.</Text>
    <Text size="sm">xmtp.chat does not send transactions.</Text>
    <Space h="xs" />
    <Text size="sm">
      Chain ID:{" "}
      <Text span inherit>
        {content.chainId}
      </Text>
    </Text>
    <Text size="sm">
      From (provided by sender):{" "}
      <Text span inherit>
        {content.from}
      </Text>
    </Text>
    <Space h="xs" />
    <List size="sm" spacing="sm">
      {content.calls.map((call, idx) => (
        <List.Item key={idx}>
          <Text size="sm" fw={500}>
            Description from sender
          </Text>
          <Text size="sm">{call.metadata?.description ?? "none"}</Text>
          {callFields.map(([field, label]) => (
            <Text key={field} size="sm">
              {label}:{" "}
              <Text span inherit>
                {call[field] ?? "none"}
              </Text>
            </Text>
          ))}
        </List.Item>
      ))}
    </List>
  </Box>
);
