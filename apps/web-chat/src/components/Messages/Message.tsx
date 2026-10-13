import { Badge, Box, Button, Group, Text, Tooltip } from "@mantine/core";
import {
  type ReactionAction,
  type Message as XmtpMessage,
} from "@xmtp/browser-sdk";
import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { useConversationContext } from "@/contexts/ConversationContext";
import { useClient } from "@/contexts/XMTPContext";
import { isActionable } from "@/helpers/messages";
import { shortAddress } from "@/helpers/strings";
import { getMemberAddress } from "@/helpers/xmtp";
import { useConversation } from "@/hooks/useConversation";

import { MessageContentWithWrapper } from "./MessageContentWithWrapper";
import { ReactionPopover } from "./ReactionPopover";

import classes from "./Message.module.css";

type Reaction = {
  count: number;
  didAdd: boolean;
  inboxIds: string[];
};

export type MessageProps = {
  message: XmtpMessage;
  scrollToMessage: (id: string) => void;
};

export const Message: React.FC<MessageProps> = ({
  message,
  scrollToMessage,
}) => {
  const navigate = useNavigate();
  const { conversationId, setReplyTarget } = useConversationContext();
  const { members } = useConversation(conversationId);
  const client = useClient();

  const isSender = client.inboxId === message.senderInboxId;
  const align = isSender ? "right" : "left";
  const hasActions = isActionable(message);

  const handleReaction = useCallback(
    (content: string, action: ReactionAction) => () => {
      void client.conversations.reactToMessage(
        message.id,
        {
          action,
          schema: "unicode",
          content,
        },
        { shouldPush: true },
      );
    },
    [client.conversations, message.id],
  );

  const reactions = useMemo(() => {
    return message.reactions
      .filter((r) => r.reaction.schema === "unicode")
      .reduce<Record<string, Reaction | undefined>>((acc, r) => {
        const reactionContent = r.reaction;
        const { content: reaction, action } = reactionContent;
        const count = acc[reaction]?.count || 0;
        const isAdding = action === "added";
        const prevDidAdd = acc[reaction]?.didAdd ?? false;
        const didAdd =
          r.senderInboxId === client.inboxId ? isAdding : prevDidAdd;
        const previousInboxIds = acc[reaction]?.inboxIds ?? [];
        const inboxIds = isAdding
          ? [...new Set([...previousInboxIds, r.senderInboxId])]
          : previousInboxIds.filter((inboxId) => inboxId !== r.senderInboxId);
        const newCount = isAdding ? count + 1 : count - 1;
        if (newCount <= 0) {
          const { [reaction]: _, ...rest } = acc;
          return rest;
        }
        return {
          ...acc,
          [reaction]: { count: newCount, didAdd, inboxIds },
        };
      }, {});
  }, [client.inboxId, message.reactions]);

  return (
    <Box p="md" tabIndex={0} className={classes.root}>
      <Box
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            void navigate(
              `/conversations/${message.conversationId}/message/${message.id}`,
            );
          }
        }}
        onClick={() =>
          void navigate(
            `/conversations/${message.conversationId}/message/${message.id}`,
          )
        }>
        <ErrorBoundary key={message.id}>
          <MessageContentWithWrapper
            message={message}
            align={align}
            senderInboxId={message.senderInboxId}
            scrollToMessage={scrollToMessage}
          />
        </ErrorBoundary>
      </Box>
      <Group
        justify={align === "left" ? "flex-start" : "flex-end"}
        mt="xs"
        gap="xxxs">
        {Object.entries(reactions).map(([reaction, reactionState]) => {
          if (!reactionState) return null;
          const { count, didAdd, inboxIds } = reactionState;
          const names = inboxIds
            .map((inboxId) => {
              const member = members.get(inboxId);
              return (
                member?.handle ||
                shortAddress((member && getMemberAddress(member)) || inboxId)
              );
            })
            .join(", ");
          return (
            <Tooltip key={reaction} label={names} disabled={!names}>
              <Badge
                className={classes.reaction}
                aria-label={`${reaction}: ${names}`}
                size="xl"
                variant={didAdd ? "outline" : "default"}
                radius="lg"
                rightSection={
                  count > 1 ? <Text size="sm">{count}</Text> : undefined
                }
                onClick={handleReaction(
                  reaction,
                  didAdd ? "removed" : "added",
                )}>
                {reaction}
              </Badge>
            </Tooltip>
          );
        })}
      </Group>
      {hasActions && (
        <Group justify={align === "left" ? "flex-start" : "flex-end"} mt={4}>
          <ReactionPopover message={message} />
          <Button
            size="compact-xs"
            variant="subtle"
            onClick={() => {
              setReplyTarget(message);
            }}>
            Reply
          </Button>
        </Group>
      )}
    </Box>
  );
};
