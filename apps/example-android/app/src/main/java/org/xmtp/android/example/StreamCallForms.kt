package org.xmtp.android.example

import uniffi.xmtp_sdk.*

// Compile the public stream calls from a separate app module.
@Suppress("UNUSED_VARIABLE")
internal suspend fun checkStreamCallForms(
    client: SDKClient,
    group: Group,
    dm: Dm,
    conversation: Conversation,
    cursor: String,
) {
    val notifications: kotlinx.coroutines.flow.Flow<Conversation> = client.conversations.stream()
    client.conversations.stream(
        ConversationStreamOptions(conversationKind = ConversationKind.DM, consentStates = emptyList()),
    )
    val messages: kotlinx.coroutines.flow.Flow<Message> = client.conversations.streamAllMessages()
    client.conversations.streamAllMessages(
        MessageStreamOptions(
            conversationKind = ConversationKind.GROUP,
            consentStates = listOf(ConsentState.ALLOWED),
            from = cursor,
        ),
    )
    group.streamMessages()
    dm.streamMessages()
    conversation.streamMessages(ConversationMessageStreamOptions(from = cursor))
    client.conversations.list()
    client.conversations.createGroup(emptyList())
    client.conversations.conversationReader(null)
}
