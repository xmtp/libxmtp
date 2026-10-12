package org.xmtp.android.example.shared.attachments

import org.xmtp.android.example.shared.feature.attachments.AttachmentPhase
import org.xmtp.android.example.shared.feature.attachments.attachmentPhase

/** Contains no remote URL or key material. */
data class AttachmentCardState(
    val id: String,
    val filename: String,
    val status: String,
    val busy: Boolean = false,
    val canSend: Boolean = false,
    val canDownload: Boolean = false,
    val canOpen: Boolean = false,
    val canDiscard: Boolean = false,
    val unknownOutcome: Boolean = false,
    val unavailable: Boolean = false,
    val error: String? = null,
    val conversationId: String = "",
    val acceptedMessageId: String? = null,
    val canSave: Boolean = canOpen,
    val mimeType: String? = null,
    val verifiedSizeBytes: Long? = null,
    val transferredBytes: Long? = null,
    val totalBytes: Long? = null,
    val phase: AttachmentPhase = attachmentPhase(status, unknownOutcome, unavailable),
)

enum class AttachmentSelection { FILE, PHOTO_VIDEO }

sealed interface AttachmentAction {
    data object Select : AttachmentAction

    data class Choose(
        val instanceId: Long,
        val selection: AttachmentSelection,
    ) : AttachmentAction

    data class DismissChoice(
        val instanceId: Long,
    ) : AttachmentAction

    data class Send(
        val draftId: String,
    ) : AttachmentAction

    data class RetryPublication(
        val draftId: String,
        val conversationId: String,
    ) : AttachmentAction

    data class Discard(
        val draftId: String,
    ) : AttachmentAction

    data class Download(
        val messageId: String,
    ) : AttachmentAction

    data class Open(
        val messageId: String,
    ) : AttachmentAction

    data class Save(
        val messageId: String,
    ) : AttachmentAction

    data class ViewChat(
        val conversationId: String,
    ) : AttachmentAction

    data class Assign(
        val draftId: String,
        val conversationId: String,
    ) : AttachmentAction
}
