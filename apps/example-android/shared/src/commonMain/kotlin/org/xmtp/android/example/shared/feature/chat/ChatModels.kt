package org.xmtp.android.example.shared.feature.chat

import org.xmtp.android.example.shared.*

data class ReplyTarget(
    val messageId: String,
    val sender: String,
    val firstLine: String,
)

data class ComposerState(
    val text: String = "",
    val selectionStart: Int = 0,
    val selectionEnd: Int = 0,
    val revision: Long = 0,
    val reply: ReplyTarget? = null,
    val pendingRequestId: Long? = null,
    val submittedRevision: Long? = null,
    val result: TextSendResult? = null,
    val enabled: Boolean = true,
    val compositionStart: Int? = null,
    val compositionEnd: Int? = null,
)

enum class MessageSurface { POPOVER, EMOJI, REACTORS }

data class ReactionIdentity(
    val inboxId: String,
    val displayName: String,
)

data class MessageOverlayState(
    val messageId: String,
    val surface: MessageSurface = MessageSurface.POPOVER,
    val emojiQuery: String = "",
    val emoji: String? = null,
    val instanceId: Long = 0,
)

data class ChatState(
    val conversationId: String? = null,
    val title: String = "",
    val group: Boolean = false,
    val memberCount: Int? = null,
    val unknown: Boolean = false,
    val messages: List<MessageRow> = emptyList(),
    val composer: ComposerState = ComposerState(),
    val overlay: MessageOverlayState? = null,
    val historyNotice: String? = null,
    val hasOlder: Boolean = true,
    val hasOlderRecovery: Boolean = false,
    val recoveryAtNewest: Boolean = true,
    val recoveryNotice: String? = null,
    val anchor: ScrollAnchor? = null,
    val atNewest: Boolean = true,
    val newMessageCount: Int? = null,
    val busy: Boolean = false,
    val failure: AppFailure? = null,
    val readerFailure: AppFailure? = null,
    val features: FeatureAvailability = FeatureAvailability(),
)

sealed interface ChatAction {
    data class ChangeDraft(
        val text: String,
        val selectionStart: Int,
        val selectionEnd: Int,
        val compositionStart: Int? = null,
        val compositionEnd: Int? = null,
    ) : ChatAction

    data class SendText(
        val text: String,
        val requestId: Long,
        val submittedRevision: Long,
    ) : ChatAction

    data class Reply(
        val messageId: String?,
    ) : ChatAction

    data class React(
        val messageId: String,
        val emoji: String,
        val remove: Boolean,
    ) : ChatAction

    data class RetrySend(
        val messageId: String,
    ) : ChatAction

    data class EditFailedSend(
        val messageId: String,
    ) : ChatAction

    data class DeleteMessage(
        val messageId: String,
    ) : ChatAction

    data class Consent(
        val allowed: Boolean,
    ) : ChatAction

    data class Viewport(
        val anchor: ScrollAnchor,
        val atNewest: Boolean,
    ) : ChatAction

    data class SelectMessage(
        val messageId: String,
    ) : ChatAction

    data class ShowEmoji(
        val messageId: String,
    ) : ChatAction

    data class ShowReactors(
        val messageId: String,
        val emoji: String,
    ) : ChatAction

    data class ChangeEmojiQuery(
        val value: String,
    ) : ChatAction

    data class CloseSheet(
        val messageId: String,
        val surface: MessageSurface,
        val instanceId: Long,
    ) : ChatAction

    data object DismissOverlay : ChatAction

    data object LoadOlder : ChatAction

    data object JumpToLatest : ChatAction

    data object LoadOlderRecovery : ChatAction

    data object LatestRecovery : ChatAction

    data object Refresh : ChatAction

    data object SelectAttachment : ChatAction

    data class OpenAttachment(
        val messageId: String,
    ) : ChatAction
}
