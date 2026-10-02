import {
  ActionIcon,
  Box,
  Button,
  Group,
  Popover,
  SegmentedControl,
  TextInput,
} from "@mantine/core";
import {
  type Message as XmtpMessage,
  type Reaction,
  type ReactionV2Content,
} from "@xmtp/browser-sdk";
import { useMemo, useState } from "react";

import { useConversationContext } from "@/contexts/ConversationContext";
import { useClient } from "@/contexts/XMTPContext";
import { useConversation } from "@/hooks/useConversation";

const EMOJIS = ["👍", "❤️", "😂", "🔥", "😮", "🙏", "🎉", "👀"];

const schemaToValue = (schema: Reaction["schema"]) => {
  switch (schema) {
    case "unicode":
      return "unicode";
    case "shortcode":
      return "shortcode";
    default:
      return "custom";
  }
};

export type ReactionBarProps = {
  message: XmtpMessage;
};

export const ReactionPopover: React.FC<ReactionBarProps> = ({ message }) => {
  const { conversationId } = useConversationContext();
  const { sendReaction } = useConversation(conversationId);
  const client = useClient();
  const [opened, setOpened] = useState(false);
  const [schema, setSchema] = useState<Reaction["schema"]>("unicode");
  const [text, setText] = useState("");

  const userReactions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of message.reactions) {
      if (
        r.senderInboxId === client.inboxId &&
        r.reaction.schema === "unicode"
      ) {
        const key = r.reaction.content;
        const prev = counts.get(key) ?? 0;
        counts.set(key, r.reaction.action === "added" ? prev + 1 : prev - 1);
      }
    }
    const result = new Set<string>();
    for (const [emoji, count] of counts) {
      if (count > 0) {
        result.add(emoji);
      }
    }
    return result;
  }, [message.reactions, client.inboxId]);

  const send = async (content: string) => {
    if (userReactions.has(content)) {
      return;
    }
    const payload: ReactionV2Content = {
      reaction: { action: "added", schema, content },
      reference: message.id,
      referenceInboxId: message.senderInboxId,
    };
    await sendReaction(payload);
    setOpened(false);
    setText("");
    setSchema("unicode");
  };

  return (
    <Popover opened={opened} onChange={setOpened} width="auto" position="top">
      <Popover.Target>
        <Button
          size="compact-xs"
          variant="subtle"
          onClick={() => {
            setOpened((opened) => !opened);
          }}>
          React
        </Button>
      </Popover.Target>
      <Popover.Dropdown p="sm">
        <SegmentedControl
          value={schemaToValue(schema)}
          onChange={(schema: string) => {
            switch (schema) {
              case "unicode":
                setSchema("unicode");
                break;
              case "shortcode":
                setSchema("shortcode");
                break;
              default:
                setSchema("custom");
            }
          }}
          data={[
            { label: "Unicode", value: "unicode" },
            { label: "Shortcode", value: "shortcode" },
            { label: "Custom", value: "custom" },
          ]}
          mb="sm"
          size="sm"
        />
        <Box
          style={{
            height: 36,
            display: "flex",
            alignItems: "center",
          }}>
          {schema === "unicode" ? (
            <Group gap={4}>
              {EMOJIS.map((emoji) => (
                <ActionIcon
                  key={emoji}
                  size="sm"
                  variant="light"
                  onClick={() => void send(emoji)}>
                  {emoji}
                </ActionIcon>
              ))}
            </Group>
          ) : (
            <Group gap="sm">
              <TextInput
                value={text}
                onChange={(event) => {
                  setText(event.currentTarget.value);
                }}
                placeholder={schema === "shortcode" ? ":xmtp:" : "Enter custom"}
                size="sm"
                style={{ width: 180 }}
                onKeyDown={(event) => {
                  if (
                    event.key === "Enter" &&
                    ["shortcode", "custom"].includes(schema)
                  ) {
                    event.preventDefault();
                    void send(text);
                  }
                }}
              />
              <ActionIcon
                size="sm"
                variant="filled"
                onClick={() => void send(text)}>
                ➤
              </ActionIcon>
            </Group>
          )}
        </Box>
      </Popover.Dropdown>
    </Popover>
  );
};
