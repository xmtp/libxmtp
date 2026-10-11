package org.xmtp.android.example.messenger.data.xmtp

import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*

interface ConversationQueries {
    suspend fun list(
        owner: ActiveSession,
        options: ListConversationsOptions,
    ): List<Conversation>

    suspend fun find(
        owner: ActiveSession,
        id: String,
    ): Conversation?

    suspend fun state(chat: Conversation): ConversationState

    suspend fun groupState(group: Group): GroupState

    suspend fun count(
        chat: Conversation,
        options: ListMessagesOptions,
    ): ULong

    suspend fun resolveInbox(
        owner: ActiveSession,
        address: String,
    ): String?

    suspend fun create(
        owner: ActiveSession,
        group: Boolean,
        recipients: List<String>,
        name: String,
        description: String,
        adminOnly: Boolean,
    ): Conversation
}

interface ChatQueries {
    suspend fun history(
        chat: Conversation,
        options: ListMessagesOptions,
        before: MessageHistoryPosition?,
        after: MessageHistoryPosition?,
    ): MessageHistoryPage

    suspend fun recovery(
        chat: Conversation,
        options: ListMessagesOptions,
        before: MessageRecoveryPosition?,
        after: MessageRecoveryPosition?,
    ): MessageRecoveryPage

    suspend fun message(
        owner: ActiveSession,
        id: String,
    ): Message?

    suspend fun messages(
        chat: Conversation,
        options: ListMessagesOptions,
    ): List<Message>
}

class XmtpQueries :
    ConversationQueries,
    ChatQueries {
    override suspend fun list(
        owner: ActiveSession,
        options: ListConversationsOptions,
    ) = owner.client.conversations.list(options)

    override suspend fun find(
        owner: ActiveSession,
        id: String,
    ) = owner.client.conversations.getById(id)

    override suspend fun state(chat: Conversation) = conversationState(chat)

    override suspend fun groupState(group: Group) = group.state()

    override suspend fun count(
        chat: Conversation,
        options: ListMessagesOptions,
    ) = chat.countMessages(options)

    override suspend fun resolveInbox(
        owner: ActiveSession,
        address: String,
    ) = owner.client.inboxIdFor(PublicIdentity(address, PublicIdentityKind.ETHEREUM))

    override suspend fun create(
        owner: ActiveSession,
        group: Boolean,
        recipients: List<String>,
        name: String,
        description: String,
        adminOnly: Boolean,
    ): Conversation =
        if (group) {
            Conversation.Group(
                owner.client.conversations.createGroup(
                    recipients,
                    CreateGroupOptions(
                        permissions = if (adminOnly) GroupPermissionMode.AdminOnly else GroupPermissionMode.AllMembers,
                        name = name,
                        description = description,
                    ),
                ),
            )
        } else {
            Conversation.Dm(owner.client.conversations.createDm(recipients.single()))
        }

    override suspend fun history(
        chat: Conversation,
        options: ListMessagesOptions,
        before: MessageHistoryPosition?,
        after: MessageHistoryPosition?,
    ) = chat.historyPage(options, before, after)

    override suspend fun recovery(
        chat: Conversation,
        options: ListMessagesOptions,
        before: MessageRecoveryPosition?,
        after: MessageRecoveryPosition?,
    ) = chat.recoveryPage(options, before, after)

    override suspend fun message(
        owner: ActiveSession,
        id: String,
    ) = owner.client.conversations.getMessageById(id)

    override suspend fun messages(
        chat: Conversation,
        options: ListMessagesOptions,
    ) = chat.messages(options)
}

fun interface BackendProbe {
    suspend fun inspect(backend: String): ServerConfiguration
}

class XmtpBackendProbe : BackendProbe {
    override suspend fun inspect(backend: String) =
        SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = backend)))
}
