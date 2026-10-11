package org.xmtp.android.example.shared.feature.attachments

import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.CapabilityUi
import org.xmtp.android.example.shared.attachments.AttachmentCardState

/** Display state contains no remote URL, file path, or key material. */
data class AttachmentState(
    val drafts: List<AttachmentCardState> = emptyList(),
    val messages: Map<String, AttachmentCardState> = emptyMap(),
    val verifiedMessageIds: Set<String> = emptySet(),
    val capability: CapabilityUi = CapabilityUi(),
    val failure: AppFailure? = null,
    val choice: AttachmentChoiceState? = null,
)

data class AttachmentChoiceState(
    val instanceId: Long,
    val conversationId: String,
)

enum class AttachmentPhase { AVAILABLE, WAITING, UPLOADING, COMPLETE, DOWNLOADING, VERIFIED, FAILED, UNKNOWN, EXPIRED }

fun attachmentPhase(
    status: String,
    unknown: Boolean,
    unavailable: Boolean,
): AttachmentPhase =
    when {
        unknown -> AttachmentPhase.UNKNOWN
        unavailable -> AttachmentPhase.EXPIRED
        status == "Waiting" -> AttachmentPhase.WAITING
        status == "Uploading" -> AttachmentPhase.UPLOADING
        status == "Complete" -> AttachmentPhase.COMPLETE
        status == "Downloading" -> AttachmentPhase.DOWNLOADING
        status == "Verified" -> AttachmentPhase.VERIFIED
        status.startsWith("Failed") -> AttachmentPhase.FAILED
        else -> AttachmentPhase.AVAILABLE
    }
