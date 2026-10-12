package org.xmtp.android.example.shared

sealed interface AppRoute {
    data object Connect : AppRoute

    data object Chats : AppRoute

    data object Requests : AppRoute

    data object CreateConversation : AppRoute

    data class Chat(
        val conversationId: String,
    ) : AppRoute

    data class ConversationDetails(
        val conversationId: String,
    ) : AppRoute

    data class GroupFields(
        val conversationId: String,
    ) : AppRoute

    data class OwnFields(
        val conversationId: String,
    ) : AppRoute

    data object Settings : AppRoute

    data object AttachmentRecovery : AppRoute
}

enum class Appearance { SYSTEM, LIGHT, DARK }

enum class FailureRecovery { NONE, RETRY, RETRY_READER, CONNECT, OPEN_SETTINGS }

data class AppFailure(
    val message: String,
    val code: String,
    val recovery: FailureRecovery = FailureRecovery.NONE,
)

data class CapabilityUi(
    val available: Boolean = false,
    val reason: String? = null,
)

data class ShellState(
    val route: AppRoute = AppRoute.Connect,
    val inbox: String = "",
    val connection: String = "",
    val readerFailure: AppFailure? = null,
    val appearance: Appearance = Appearance.SYSTEM,
    val reducedMotion: Boolean = false,
)

sealed interface ShellAction {
    data class Navigate(
        val route: AppRoute,
    ) : ShellAction

    data object Back : ShellAction

    data object RetryReader : ShellAction

    data object Refresh : ShellAction
}

enum class MessageSendState { SENDING, QUEUED, SENT, NOT_SENT, UNKNOWN }

sealed interface TextSendOutcome {
    data class Accepted(
        val messageId: String,
    ) : TextSendOutcome

    data class AcceptedPendingCommit(
        val messageId: String,
        val failure: AppFailure,
    ) : TextSendOutcome

    data class Rejected(
        val failure: AppFailure,
    ) : TextSendOutcome

    data class Unknown(
        val draftId: String,
    ) : TextSendOutcome
}

val MessageSendState.label: String
    get() =
        when (this) {
            MessageSendState.SENDING -> "Sending"
            MessageSendState.QUEUED -> "Queued"
            MessageSendState.SENT -> "Sent"
            MessageSendState.NOT_SENT -> "Not sent"
            MessageSendState.UNKNOWN -> "Unknown outcome"
        }
