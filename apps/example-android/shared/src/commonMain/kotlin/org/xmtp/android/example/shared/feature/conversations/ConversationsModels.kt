package org.xmtp.android.example.shared.feature.conversations

import org.xmtp.android.example.shared.*

data class ListAnchor(
    val firstIndex: Int = 0,
    val offsetPx: Int = 0,
)

data class ConversationsState(
    val rows: List<ConversationRow> = emptyList(),
    val requests: List<ConversationRow> = emptyList(),
    val requestCount: Int? = null,
    val requestCountValid: Boolean = false,
    val unknownTab: Boolean = false,
    val query: String = "",
    val selectedConversationId: String? = null,
    val anchor: ListAnchor = ListAnchor(),
    val listLimit: Int = 50,
    val busy: Boolean = false,
    val pageLoading: Boolean = false,
    val hasMore: Boolean = true,
    val connection: String = "",
    val failure: AppFailure? = null,
)

sealed interface ConversationsAction {
    data class ChangeQuery(
        val value: String,
    ) : ConversationsAction

    data class SetAlias(
        val conversationId: String,
        val alias: String,
    ) : ConversationsAction

    data class OpenConversation(
        val id: String,
    ) : ConversationsAction

    data class SelectTab(
        val unknown: Boolean,
    ) : ConversationsAction

    data class Viewport(
        val firstIndex: Int,
        val visibleCount: Int,
        val offsetPx: Int = 0,
        val firstConversationIndex: Int = firstIndex,
    ) : ConversationsAction

    data object LoadMore : ConversationsAction

    data object Refresh : ConversationsAction

    data object OpenRequests : ConversationsAction

    data class Create(
        val group: Boolean,
    ) : ConversationsAction
}

enum class RequestDecision { ACCEPT, DELETE_LOCAL, BLOCK }

data class RequestState(
    val conversationId: String = "",
    val title: String = "",
    val senderInboxId: String = "",
    val pendingDecision: RequestDecision? = null,
    val deleteCapability: CapabilityUi = CapabilityUi(),
    val failure: AppFailure? = null,
    val activeDecision: RequestDecision? = null,
)

sealed interface RequestAction {
    data object Accept : RequestAction

    data object DeleteLocal : RequestAction

    data object Block : RequestAction

    data object Confirm : RequestAction

    data object Cancel : RequestAction
}

enum class CreateStep { RECIPIENTS, REVIEW }

data class RecipientUi(
    val inboxId: String,
    val title: String,
    val selected: Boolean = false,
    val failure: AppFailure? = null,
)

data class CreateConversationState(
    val group: Boolean = false,
    val step: CreateStep = CreateStep.RECIPIENTS,
    val recipients: String = "",
    val knownRecipients: List<RecipientUi> = emptyList(),
    val selectedRecipients: List<RecipientUi> = emptyList(),
    val name: String = "",
    val description: String = "",
    val adminOnly: Boolean = false,
    val maxMembers: Int? = null,
    val busy: Boolean = false,
    val failure: AppFailure? = null,
)

sealed interface CreateConversationAction {
    data class ChangeMode(
        val group: Boolean,
    ) : CreateConversationAction

    data class ChangeRecipients(
        val value: String,
    ) : CreateConversationAction

    data class ToggleRecipient(
        val inboxId: String,
    ) : CreateConversationAction

    data object ResolveRecipients : CreateConversationAction

    data object Next : CreateConversationAction

    data object Back : CreateConversationAction

    data class ChangeName(
        val value: String,
    ) : CreateConversationAction

    data class ChangeDescription(
        val value: String,
    ) : CreateConversationAction

    data class ChangePreset(
        val adminOnly: Boolean,
    ) : CreateConversationAction

    data object Create : CreateConversationAction
}
