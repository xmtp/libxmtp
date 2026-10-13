package org.xmtp.android.example.messenger

import org.xmtp.android.example.messenger.app.AppGraph
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.Screen
import uniffi.xmtp_sdk.*

internal fun currentChatTargetReady(
    model: MessengerViewModel,
    graph: AppGraph,
    owner: ActiveSession,
    chatId: String,
): Boolean {
    val chat = model.chat.state.value
    val rendered = model.state.value
    return model.session.accepts(owner.key) &&
        graph.destinations.state.value.route == AppRoute.Chat(chatId) &&
        model.chat.currentConversation()?.id() == chatId &&
        chat.conversationId == chatId && chat.failure == null && !chat.busy &&
        rendered.screen == Screen.TIMELINE && rendered.conversationId == chatId &&
        rendered.chatState?.let { it.conversationId == chatId && it.failure == null && !it.busy } == true
}
