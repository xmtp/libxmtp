package org.xmtp.android.example.shared

import org.xmtp.android.example.shared.feature.chat.*
import org.xmtp.android.example.shared.feature.connect.*
import org.xmtp.android.example.shared.feature.conversations.*
import org.xmtp.android.example.shared.feature.group.*
import org.xmtp.android.example.shared.feature.settings.*

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
    val muted: Boolean? = null,
    val group: Boolean = false,
    val alias: String? = null,
    val senderPreview: String? = null,
)

data class ReactionUi(
    val emoji: String,
    val count: Int,
    val mine: Boolean,
    val reactors: List<ReactionIdentity> = emptyList(),
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
) {
    val sendState: MessageSendState
        get() =
            when (status) {
                "Sending" -> MessageSendState.SENDING
                "Queued" -> MessageSendState.QUEUED
                "Sent", "Delivered", "Published" -> MessageSendState.SENT
                "Failed", "Not sent" -> MessageSendState.NOT_SENT
                "Unknown outcome", "Unknown" -> MessageSendState.UNKNOWN
                else -> if (mine) MessageSendState.UNKNOWN else MessageSendState.SENT
            }
}

data class MemberRow(
    val inboxId: String,
    val role: String,
    val canManage: Boolean,
    val displayName: String? = null,
)

data class ScrollAnchor(
    val messageId: String,
    val sentAtNs: Long,
    val offsetPx: Int,
    val wasAtNewest: Boolean,
    val deliveryCursor: String? = null,
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

data class TextSendResult(
    val requestId: Long,
    val conversationId: String,
    val submittedRevision: Long,
    val outcome: TextSendOutcome,
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
    val textSendResult: TextSendResult? = null,
    val replyTo: String? = null,
    val replyPreview: String? = null,
    val historyNotice: String? = null,
    val hasOlder: Boolean = true,
    val hasOlderRecovery: Boolean = false,
    val recoveryAtNewest: Boolean = true,
    val recoveryNotice: String? = null,
    val anchor: ScrollAnchor? = null,
    val settings: ConversationSettings = ConversationSettings(),
    val unknownSends: List<UnknownSendRow> = emptyList(),
    val features: FeatureAvailability = FeatureAvailability(),
    val notificationStatus: String = "Off: Firebase is not configured",
    val notificationsEnabled: Boolean = false,
    val shellState: ShellState? = null,
    val connectState: ConnectState? = null,
    val conversationsState: ConversationsState? = null,
    val chatState: ChatState? = null,
    val createState: CreateConversationState? = null,
    val groupState: GroupState? = null,
    val settingsState: SettingsState? = null,
    val requestState: RequestState? = null,
)

sealed interface MessengerAction {
    data class Connect(
        val backend: String,
        val credential: String?,
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

    data object LoadOlderRecovery : MessengerAction

    data object LatestRecovery : MessengerAction

    data object LoadOlder : MessengerAction

    data object JumpToLatest : MessengerAction

    data class Viewport(
        val anchor: ScrollAnchor,
        val atNewest: Boolean,
    ) : MessengerAction

    data class SendText(
        val text: String,
        val requestId: Long = 0,
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

    data class PickReaction(
        val messageId: String,
    ) : MessengerAction

    data object SelectAttachment : MessengerAction

    data class OpenAttachment(
        val messageId: String,
    ) : MessengerAction

    data class SetAppNotifications(
        val enabled: Boolean,
    ) : MessengerAction

    data class SetConversationNotifications(
        val enabled: Boolean,
    ) : MessengerAction

    data class ConnectFeature(
        val action: ConnectAction,
    ) : MessengerAction

    data class ConversationsFeature(
        val action: ConversationsAction,
    ) : MessengerAction

    data class ChatFeature(
        val action: ChatAction,
    ) : MessengerAction

    data class CreateFeature(
        val action: CreateConversationAction,
    ) : MessengerAction

    data class GroupFeature(
        val action: GroupAction,
    ) : MessengerAction

    data class SettingsFeature(
        val action: SettingsAction,
    ) : MessengerAction

    data class RequestFeature(
        val action: RequestAction,
    ) : MessengerAction

    data class ShellFeature(
        val action: ShellAction,
    ) : MessengerAction
}
