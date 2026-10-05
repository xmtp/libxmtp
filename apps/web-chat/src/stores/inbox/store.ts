import {
  Dm,
  Group,
  type Conversation,
  type Message as XmtpMessage,
  type Member,
  type GroupPermissions,
} from "@xmtp/browser-sdk";
import { createStore } from "zustand";

import {
  getLastCreatedAt,
  isLastSentAt,
  sortConversations,
  sortMessages,
} from "@/stores/inbox/utils";

export type ConversationMetadata = {
  name?: string;
  description?: string;
  imageUrl?: string;
};

// alias types for clarity
type InboxId = string;
type ConversationId = string;
type MessageId = string;

export type InboxState = {
  // all conversations
  conversations: Map<ConversationId, Conversation>;
  // the most recent conversation creation timestamp
  lastCreatedAt?: bigint;
  // the last message for each conversation
  lastMessages: Map<ConversationId, XmtpMessage | undefined>;
  // the last message sent timestamp for each conversation
  lastSentAt: Map<ConversationId, bigint | undefined>;
  // the members of each conversation
  members: Map<ConversationId, Map<InboxId, Member>>;
  // all conversation messages
  messages: Map<ConversationId, Map<MessageId, XmtpMessage>>;
  // the metadata for each conversation
  metadata: Map<ConversationId, ConversationMetadata>;
  // the permissions for each conversation
  permissions: Map<ConversationId, GroupPermissions>;
  // sorted conversations by most recent activity
  sortedConversations: Conversation[];
  // sorted messages by last sent timestamp
  sortedMessages: Map<ConversationId, XmtpMessage[]>;
  // the last attempted sync timestamp
  lastSyncedAt?: bigint;
};

export type InboxActions = {
  addConversation: (conversation: Conversation) => Promise<void>;
  addConversations: (conversations: Conversation[]) => Promise<void>;
  getConversation: (id: string) => Conversation | undefined;
  hasConversation: (id: string) => boolean;
  addMessage: (conversationId: string, message: XmtpMessage) => Promise<void>;
  addMessages: (
    conversationId: string,
    messages: XmtpMessage[],
  ) => Promise<void>;
  getMessage: (
    conversationId: string,
    messageId: string,
  ) => XmtpMessage | undefined;
  getMessages: (conversationId: string) => XmtpMessage[];
  hasMessage: (conversationId: string, messageId: string) => boolean;
  setLastSyncedAt: (timestamp: bigint) => void;
  syncPermissions: (conversationId: string) => Promise<void>;
  syncMembers: (conversationId: string) => Promise<void>;
  reset: () => void;
};

export const inboxStore = createStore<InboxState & InboxActions>()(
  (set, get, store) => ({
    conversations: new Map(),
    lastMessages: new Map(),
    lastSentAt: new Map(),
    members: new Map(),
    messages: new Map(),
    metadata: new Map(),
    permissions: new Map(),
    sortedConversations: [],
    sortedMessages: new Map(),
    addConversation: async (conversation: Conversation) => {
      const state = get();
      // update conversations state
      const newConversations = new Map(state.conversations);
      newConversations.set(conversation.id, conversation);
      // update members state
      const members = await conversation.members();
      const newMembers = new Map(state.members);
      newMembers.set(
        conversation.id,
        new Map(members.map((m) => [m.inboxId, m])),
      );
      const newPermissions = new Map(state.permissions);
      const newMetadata = new Map(state.metadata);
      if (conversation instanceof Group) {
        const snapshot = await conversation.state();
        newPermissions.set(conversation.id, snapshot.permissions);
        newMetadata.set(conversation.id, {
          name: snapshot.name,
          description: snapshot.description,
          imageUrl: snapshot.imageUrl,
        });
      } else if (conversation instanceof Dm) {
        const member = members.find(
          (m) => m.inboxId !== conversation.addedByInboxId,
        );
        if (member) {
          // update metadata state
          newMetadata.set(conversation.id, {
            name: member.inboxId,
          });
        }
      }
      // update last message state
      const lastMessage = await conversation.lastMessage();
      const newLastMessages = new Map(state.lastMessages);
      newLastMessages.set(conversation.id, lastMessage);
      set({
        conversations: newConversations,
        lastCreatedAt: getLastCreatedAt(conversation, state.lastCreatedAt),
        lastMessages: newLastMessages,
        members: newMembers,
        metadata: newMetadata,
        permissions: newPermissions,
        sortedConversations: sortConversations(
          newConversations,
          newLastMessages,
        ),
      });
    },
    addConversations: async (conversations: Conversation[]) => {
      if (conversations.length === 0) {
        return;
      }
      const state = get();
      // get conversation members in parallel
      const allMembers = new Map<string, Member[]>(
        await Promise.all(
          conversations.map(
            async (conversation): Promise<[string, Member[]]> => [
              conversation.id,
              await conversation.members(),
            ],
          ),
        ),
      );
      // get conversation last messages in parallel
      const allLastMessages = new Map<string, XmtpMessage | undefined>(
        await Promise.all(
          conversations.map(
            async (
              conversation,
            ): Promise<[string, XmtpMessage | undefined]> => [
              conversation.id,
              await conversation.lastMessage(),
            ],
          ),
        ),
      );

      // update conversations, members, and last message states
      const newConversations = new Map(state.conversations);
      const newMembers = new Map(state.members);
      const newPermissions = new Map(state.permissions);
      const newMetadata = new Map(state.metadata);
      const newLastMessages = new Map(state.lastMessages);
      let lastCreatedAt = state.lastCreatedAt;
      for (const conversation of conversations) {
        newConversations.set(conversation.id, conversation);
        lastCreatedAt = getLastCreatedAt(conversation, lastCreatedAt);
        const members = allMembers.get(conversation.id) ?? [];
        newMembers.set(
          conversation.id,
          new Map(members.map((m) => [m.inboxId, m])),
        );
        if (conversation instanceof Group) {
          const snapshot = await conversation.state();
          newPermissions.set(conversation.id, snapshot.permissions);
          newMetadata.set(conversation.id, {
            name: snapshot.name,
            description: snapshot.description,
            imageUrl: snapshot.imageUrl,
          });
        } else if (conversation instanceof Dm) {
          const member = members.find(
            (m) => m.inboxId !== conversation.addedByInboxId,
          );
          if (member) {
            // update metadata state
            newMetadata.set(conversation.id, {
              name: member.inboxId,
            });
          }
        }
        const lastMessage = allLastMessages.get(conversation.id);
        newLastMessages.set(conversation.id, lastMessage);
      }

      set({
        conversations: newConversations,
        lastCreatedAt,
        lastMessages: newLastMessages,
        members: newMembers,
        metadata: newMetadata,
        permissions: newPermissions,
        sortedConversations: sortConversations(
          newConversations,
          newLastMessages,
        ),
      });
    },
    getConversation: (id: string) => {
      return get().conversations.get(id);
    },
    hasConversation: (id: string) => {
      return get().conversations.has(id);
    },
    addMessage: async (conversationId: string, message: XmtpMessage) => {
      const state = get();
      const conversation = state.conversations.get(conversationId);
      // update messages state
      const newMessagesState = new Map(state.messages);
      const conversationMessages =
        newMessagesState.get(conversationId) || new Map<string, XmtpMessage>();
      const newMessages = new Map(conversationMessages);
      newMessages.set(message.id, message);
      newMessagesState.set(conversationId, newMessages);

      // update last sent at and last message states
      const newLastSentAt = new Map(state.lastSentAt);
      const newLastMessages = new Map(state.lastMessages);
      if (isLastSentAt(message, state.lastSentAt.get(conversationId))) {
        newLastSentAt.set(conversationId, message.sentAt.ns);
        newLastMessages.set(conversationId, message);
      }

      // update sorted messages state
      const newSortedMessages = new Map(state.sortedMessages);
      newSortedMessages.set(conversationId, sortMessages(newMessages));

      const newMembers = new Map(state.members);
      const newMetadata = new Map(state.metadata);

      // check for updated members and metadata
      if (message.content.kind === "groupUpdated") {
        const groupUpdated = message.content.value;

        // member updates
        if (conversation) {
          const isActive = await conversation
            .state()
            .then((state) =>
              "common" in state ? state.common.isActive : state.isActive,
            );
          // ensure group is active before syncing
          if (isActive) {
            await conversation.sync();
          }
          const members = await conversation.members();
          const updatedMembers = new Map(members.map((m) => [m.inboxId, m]));
          newMembers.set(message.conversationId, updatedMembers);
        }

        // update metadata state
        const metadataUpdates: ConversationMetadata = {};
        groupUpdated.metadataFieldChanges.forEach((change) => {
          switch (change.fieldName) {
            case "group_name":
              metadataUpdates.name = change.newValue;
              break;
            case "description":
              metadataUpdates.description = change.newValue;
              break;
            case "group_image_url_square":
              metadataUpdates.imageUrl = change.newValue;
              break;
          }
        });
        if (Object.keys(metadataUpdates).length > 0) {
          const existingMetadata = newMetadata.get(message.conversationId);
          newMetadata.set(message.conversationId, {
            ...existingMetadata,
            ...metadataUpdates,
          });
        }
      }

      set({
        lastMessages: newLastMessages,
        lastSentAt: newLastSentAt,
        members: newMembers,
        messages: newMessagesState,
        metadata: newMetadata,
        sortedConversations: sortConversations(
          state.conversations,
          newLastMessages,
        ),
        sortedMessages: newSortedMessages,
      });
    },
    addMessages: async (conversationId: string, messages: XmtpMessage[]) => {
      const state = get();
      const newMessagesByConversation = new Map(state.messages);
      const conversationMessages =
        newMessagesByConversation.get(conversationId) ||
        new Map<string, XmtpMessage>();
      const newMessages = new Map(conversationMessages);
      let lastSentAt = state.lastSentAt.get(conversationId);
      let lastMessage = state.lastMessages.get(conversationId);

      const newMetadata = new Map(state.metadata);
      const newMembers = new Map(state.members);

      for (const message of messages) {
        newMessages.set(message.id, message);
        if (isLastSentAt(message, lastSentAt)) {
          lastSentAt = message.sentAt.ns;
          lastMessage = message;
        }

        // check for updated members and metadata
        if (message.content.kind === "groupUpdated") {
          const groupUpdated = message.content.value;

          // member updates
          const conversation = state.conversations.get(message.conversationId);
          if (conversation) {
            const isActive = await conversation
              .state()
              .then((state) =>
                "common" in state ? state.common.isActive : state.isActive,
              );
            // ensure group is active before syncing
            if (isActive) {
              await conversation.sync();
            }
            const members = await conversation.members();
            const updatedMembers = new Map(members.map((m) => [m.inboxId, m]));
            newMembers.set(message.conversationId, updatedMembers);
          }

          // metadata updates
          const metadataUpdates: ConversationMetadata = {};
          groupUpdated.metadataFieldChanges.forEach((change) => {
            switch (change.fieldName) {
              case "group_name":
                metadataUpdates.name = change.newValue;
                break;
              case "description":
                metadataUpdates.description = change.newValue;
                break;
              case "group_image_url_square":
                metadataUpdates.imageUrl = change.newValue;
                break;
            }
          });
          if (Object.keys(metadataUpdates).length > 0) {
            const existingMetadata = newMetadata.get(message.conversationId);
            newMetadata.set(message.conversationId, {
              ...existingMetadata,
              ...metadataUpdates,
            });
          }
        }
      }

      // update messages state
      newMessagesByConversation.set(conversationId, newMessages);

      // update last sent at state
      const newLastSentAt = new Map(state.lastSentAt);
      newLastSentAt.set(conversationId, lastSentAt);

      // update last message state
      const newLastMessages = new Map(state.lastMessages);
      newLastMessages.set(conversationId, lastMessage);

      // update sorted messages state
      const newSortedMessages = new Map(state.sortedMessages);
      newSortedMessages.set(conversationId, sortMessages(newMessages));

      set({
        lastMessages: newLastMessages,
        lastSentAt: newLastSentAt,
        members: newMembers,
        messages: newMessagesByConversation,
        metadata: newMetadata,
        sortedConversations: sortConversations(
          state.conversations,
          newLastMessages,
        ),
        sortedMessages: newSortedMessages,
      });
    },
    getMessage: (conversationId: string, messageId: string) => {
      const messages = get().messages.get(conversationId);
      return messages?.get(messageId);
    },
    getMessages: (conversationId: string) => {
      const messages = get().messages.get(conversationId);
      return messages ? Array.from(messages.values()) : [];
    },
    hasMessage: (conversationId: string, messageId: string) => {
      const conversationMessages = get().messages.get(conversationId);
      return conversationMessages?.has(messageId) ?? false;
    },
    setLastSyncedAt: (timestamp: bigint) => {
      set({ lastSyncedAt: timestamp });
    },
    syncPermissions: async (conversationId: string) => {
      const state = get();
      const conversation = state.conversations.get(conversationId);
      if (conversation instanceof Group) {
        const newPermissions = new Map(state.permissions);
        newPermissions.set(
          conversationId,
          await conversation.state().then((state) => state.permissions),
        );
        set({
          permissions: newPermissions,
        });
      }
    },
    syncMembers: async (conversationId: string) => {
      const state = get();
      const conversation = state.conversations.get(conversationId);
      if (conversation instanceof Group) {
        const newMembers = new Map(state.members);
        const members = await conversation.members();
        newMembers.set(
          conversationId,
          new Map(members.map((m) => [m.inboxId, m])),
        );
        set({ members: newMembers });
      }
    },
    reset: () => {
      set(store.getInitialState());
    },
  }),
);
