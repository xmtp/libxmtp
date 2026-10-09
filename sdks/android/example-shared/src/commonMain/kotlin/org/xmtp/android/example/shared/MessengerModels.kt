package org.xmtp.android.example.shared

enum class Screen {
    START,
    CONVERSATIONS,
    CREATE,
    TIMELINE,
    CONVERSATION_SETTINGS,
    APP_SETTINGS,
    GROUP_FIELDS,
    MY_FIELDS,
    DRAFTS,
}

data class ConversationRow(
    val id: String,
    val title: String,
    val preview: String,
    val time: String,
    val unread: String,
    val unknown: Boolean,
    val pattern: Int,
)

data class ReactionUi(
    val emoji: String,
    val count: Int,
    val mine: Boolean,
)

data class MessageRow(
    val id: String,
    val sender: String,
    val text: String,
    val time: String,
    val day: String,
    val sentAtNs: Long,
    val mine: Boolean,
    val status: String,
    val reply: String? = null,
    val reactions: List<ReactionUi> = emptyList(),
    val deleted: Boolean = false,
    val attachment: Boolean = false,
)

data class MemberRow(
    val inboxId: String,
    val role: String,
    val canManage: Boolean,
)

data class ScrollAnchor(
    val messageId: String,
    val sentAtNs: Long,
    val offsetPx: Int,
    val wasAtNewest: Boolean,
)

data class ConversationSettings(
    val title: String = "",
    val description: String = "",
    val group: Boolean = false,
    val members: List<MemberRow> = emptyList(),
    val preset: String = "",
    val membership: String = "",
    val canRequestRemoval: Boolean = false,
    val disappearingSeconds: String = "0",
    val notifications: Boolean = false,
)

data class FeatureAvailability(
    val attachments: Boolean = false,
    val metadata: Boolean = false,
    val notifications: Boolean = false,
)

data class UnknownSendRow(
    val draftId: String,
    val conversationId: String,
)

data class MessengerState(
    val screen: Screen =
        Screen.START,
    val backend: String = "",
    val credentialsRequiredFor: String? = null,
    val inbox: String = "",
    val busy: Boolean = false,
    val error: String? = null,
    val pendingReset: Boolean = false,
    val readerError: String? = null,
    val connection: String = "",
    val migrationRequired: Boolean = false,
    val migrationAccounts: List<String> = emptyList(),
    val conversations: List<ConversationRow> = emptyList(),
    val unknownTab: Boolean = false,
    val messages: List<MessageRow> = emptyList(),
    val conversationId: String? = null,
    val conversationTitle: String = "",
    val conversationUnknown: Boolean = false,
    val replyTo: String? = null,
    val replyPreview: String? = null,
    val historyNotice: String? = null,
    val hasOlder: Boolean = true,
    val anchor: ScrollAnchor? = null,
    val settings: ConversationSettings = ConversationSettings(),
    val unknownSends: List<UnknownSendRow> = emptyList(),
    val features: FeatureAvailability = FeatureAvailability(),
)

sealed interface MessengerAction {
    data class Connect(
        val backend: String,
        val credential: String,
    ) : MessengerAction

    data class InspectBackend(
        val backend: String,
    ) : MessengerAction

    data class Navigate(
        val screen: Screen,
    ) : MessengerAction

    data class OpenConversation(
        val id: String,
    ) : MessengerAction

    data class SelectTab(
        val unknown: Boolean,
    ) : MessengerAction

    data class ListViewport(
        val firstIndex: Int,
        val visibleCount: Int,
    ) : MessengerAction

    data object LoadMoreConversations : MessengerAction

    data object Refresh : MessengerAction

    data object RetryReader : MessengerAction

    data object LoadOlder : MessengerAction

    data object JumpToLatest : MessengerAction

    data class Viewport(
        val anchor: ScrollAnchor,
        val atNewest: Boolean,
    ) : MessengerAction

    data class SendText(
        val text: String,
    ) : MessengerAction

    data class Reply(
        val messageId: String?,
    ) : MessengerAction

    data class React(
        val messageId: String,
        val emoji: String,
        val remove: Boolean,
    ) : MessengerAction

    data class RetrySend(
        val messageId: String,
    ) : MessengerAction

    data class DeleteMessage(
        val messageId: String,
    ) : MessengerAction

    data class Consent(
        val allowed: Boolean,
    ) : MessengerAction

    data class Create(
        val group: Boolean,
        val recipients: String,
        val name: String,
        val description: String,
        val adminOnly: Boolean,
    ) : MessengerAction

    data class UpdateGroup(
        val name: String,
        val description: String,
    ) : MessengerAction

    data class SetPreset(
        val adminOnly: Boolean,
    ) : MessengerAction

    data class AddMember(
        val inboxId: String,
    ) : MessengerAction

    data class RemoveMember(
        val inboxId: String,
    ) : MessengerAction

    data class SetAdmin(
        val inboxId: String,
        val admin: Boolean,
    ) : MessengerAction

    data class SetDisappearing(
        val seconds: Long,
    ) : MessengerAction

    data object RequestRemoval : MessengerAction

    data object SignOut : MessengerAction

    data object DeleteAccount : MessengerAction

    data object ResumeReset : MessengerAction

    data class ResetLegacyAccount(
        val inboxId: String,
    ) : MessengerAction

    data class DiscardUnknownSend(
        val draftId: String,
    ) : MessengerAction

    data class Feature(
        val name: String,
        val value: String = "",
    ) : MessengerAction
}
