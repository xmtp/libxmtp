import {
  Timestamp,
  ConversationStream,
  MessageStream,
  type Conversation,
  type CreateGroupOptions,
  type Message as XmtpMessage,
  type PublicIdentity,
} from "@xmtp/browser-sdk";
import { useCallback, useState } from "react";

import { useClient } from "@/contexts/XMTPContext";
import { dateToNs } from "@/helpers/date";
import { isReaction } from "@/helpers/messages";
import {
  useActions,
  useConversations as useConversationsState,
} from "@/stores/inbox/hooks";
import { inboxStore } from "@/stores/inbox/store";

export const useConversations = () => {
  const client = useClient();
  const { addConversations, addConversation, addMessage, setLastSyncedAt } =
    useActions();
  const conversations = useConversationsState();
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);

  const refreshConversationsList = useCallback(async () => {
    setLoading(true);
    try {
      const convos = await client.conversations.list({
        createdAfter:
          inboxStore.getState().lastCreatedAt === undefined
            ? undefined
            : new Timestamp(inboxStore.getState().lastCreatedAt!),
      });
      await addConversations(convos);
      setLastSyncedAt(dateToNs(new Date()));
      return convos;
    } finally {
      setLoading(false);
    }
  }, [addConversations, client, setLastSyncedAt]);

  const sync = useCallback(
    async (fromNetwork: boolean = false) => {
      if (fromNetwork) {
        setSyncing(true);

        try {
          await client.conversations.sync();
        } finally {
          setSyncing(false);
        }
      }

      await refreshConversationsList();
    },
    [client, refreshConversationsList],
  );

  const syncAll = useCallback(async () => {
    setSyncing(true);

    try {
      await client.conversations.syncAll(undefined);
    } finally {
      setSyncing(false);
    }

    await refreshConversationsList();
  }, [client, refreshConversationsList]);

  const getConversationById = async (conversationId: string) => {
    setLoading(true);

    try {
      const conversation = await client.conversations.getById(conversationId);
      return conversation;
    } finally {
      setLoading(false);
    }
  };

  const getDmByInboxId = async (inboxId: string) => {
    setLoading(true);

    try {
      const dm = await client.conversations.getDmByInboxId(inboxId);
      return dm;
    } finally {
      setLoading(false);
    }
  };

  const getMessageById = async (messageId: string) => {
    setLoading(true);

    try {
      const message = await client.conversations.getMessageById(messageId);
      return message;
    } finally {
      setLoading(false);
    }
  };

  const createGroup = async (
    inboxIds: string[],
    options?: CreateGroupOptions,
  ) => {
    setLoading(true);

    try {
      const conversation = await client.conversations.createGroup(
        inboxIds,
        options,
      );
      void addConversation(conversation);
      return conversation;
    } finally {
      setLoading(false);
    }
  };

  const createGroupWithIdentifiers = async (
    identifiers: PublicIdentity[],
    options?: CreateGroupOptions,
  ) => {
    setLoading(true);

    try {
      const conversation = await client.conversations.createGroup(
        identifiers,
        options,
      );
      void addConversation(conversation);
      return conversation;
    } finally {
      setLoading(false);
    }
  };

  const createDm = async (inboxId: string) => {
    setLoading(true);

    try {
      const conversation = await client.conversations.createDm(inboxId);
      void addConversation(conversation);
      return conversation;
    } finally {
      setLoading(false);
    }
  };

  const createDmWithIdentifier = async (identifier: PublicIdentity) => {
    setLoading(true);

    try {
      const conversation = await client.conversations.createDm(identifier);
      void addConversation(conversation);
      return conversation;
    } finally {
      setLoading(false);
    }
  };

  const stream = useCallback(async () => {
    const onValue = (conversation: Conversation) => {
      void addConversation(conversation);
    };

    const stream = ConversationStream.open(client);
    await stream.ready();
    void stream.onValue(onValue).catch(console.error);

    return () => {
      void stream.end();
    };
  }, [addConversation, client]);

  const streamAllMessages = useCallback(async () => {
    const onValue = async (message: XmtpMessage) => {
      if (isReaction(message) && message.content.reference) {
        const updatedMessage = await client.conversations.getMessageById(
          message.content.reference,
        );
        if (updatedMessage) {
          await addMessage(updatedMessage.conversationId, updatedMessage);
        }
        return;
      }
      await addMessage(message.conversationId, message);
    };

    const stream = MessageStream.open(client);
    await stream.ready();
    void stream.onValue(onValue).catch(console.error);

    return () => {
      void stream.end();
    };
  }, [addMessage, client]);

  return {
    conversations,
    getConversationById,
    getDmByInboxId,
    getMessageById,
    loading,
    createDm,
    createDmWithIdentifier,
    createGroup,
    createGroupWithIdentifiers,
    stream,
    streamAllMessages,
    sync,
    syncAll,
    syncing,
  };
};
