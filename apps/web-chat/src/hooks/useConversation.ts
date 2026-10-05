import { Timestamp, type Intent } from "@xmtp/browser-sdk";
import type {
  EncodedContent,
  RemoteAttachment,
  SendOptions,
  ReplyContent,
  ReactionV2Content,
} from "@xmtp/browser-sdk";
import { useCallback, useState } from "react";

import {
  useActions,
  useConversation as useConversationState,
  useMembers,
  useMessages,
  useMetadata,
  usePermissions,
} from "@/stores/inbox/hooks";
import { inboxStore } from "@/stores/inbox/store";

export const useConversation = (conversationId: string) => {
  const { addMessages } = useActions();
  const conversation = useConversationState(conversationId);
  const members = useMembers(conversationId);
  const permissions = usePermissions(conversationId);
  const { name, description, imageUrl } = useMetadata(conversationId);
  const messages = useMessages(conversationId);
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [sending, setSending] = useState(false);

  if (!conversation) {
    throw new Error(
      `useConversation: Conversation ${conversationId} not found`,
    );
  }

  const sync = useCallback(
    async (fromNetwork: boolean = false) => {
      if (fromNetwork) {
        setSyncing(true);

        try {
          const isActive = await conversation
            .state()
            .then((state) =>
              "common" in state ? state.common.isActive : state.isActive,
            );
          // ensure group is active before syncing
          if (isActive) {
            await conversation.sync();
          }
        } finally {
          setSyncing(false);
        }
      }

      setLoading(true);

      try {
        const msgs = await conversation.messages({
          sentAfter:
            inboxStore.getState().lastSentAt.get(conversationId) === undefined
              ? undefined
              : new Timestamp(
                  inboxStore.getState().lastSentAt.get(conversationId)!,
                ),
        });
        await addMessages(conversation.id, msgs);
        return msgs;
      } finally {
        setLoading(false);
      }
    },
    [addMessages, conversation, conversationId],
  );

  const send = useCallback(
    async (content: EncodedContent, options?: SendOptions) => {
      setSending(true);

      try {
        await conversation.send(content, { shouldPush: true, ...options });
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  const sendText = useCallback(
    async (text: string) => {
      setSending(true);

      try {
        await conversation.sendText(text, { shouldPush: true });
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  type Reply = ReplyContent;

  const sendReply = useCallback(
    async (reply: Reply) => {
      setSending(true);
      try {
        await conversation.sendReply(
          reply.reference,
          reply.referenceInboxId,
          reply.content,
          { shouldPush: true },
        );
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  const sendRemoteAttachment = useCallback(
    async (remoteAttachment: RemoteAttachment) => {
      setSending(true);
      try {
        await conversation.sendRemoteAttachment(remoteAttachment, {
          shouldPush: true,
        });
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  const sendIntent = useCallback(
    async (intent: Intent) => {
      setSending(true);
      try {
        await conversation.sendIntent(intent, { shouldPush: true });
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  const sendReaction = useCallback(
    async (reaction: ReactionV2Content) => {
      setSending(true);
      try {
        await conversation.sendReaction(
          reaction.reference,
          reaction.referenceInboxId,
          reaction.reaction,
          { shouldPush: true },
        );
      } finally {
        setSending(false);
      }
    },
    [conversation],
  );

  return {
    conversation,
    description,
    imageUrl,
    loading,
    members,
    messages,
    name,
    permissions,
    send,
    sendText,
    sendReply,
    sendRemoteAttachment,
    sendIntent,
    sendReaction,
    sending,
    sync,
    syncing,
  };
};
