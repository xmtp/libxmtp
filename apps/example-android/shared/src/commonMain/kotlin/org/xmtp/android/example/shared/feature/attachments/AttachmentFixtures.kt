package org.xmtp.android.example.shared.feature.attachments

import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.CapabilityUi
import org.xmtp.android.example.shared.attachments.AttachmentCardState

object AttachmentFixtures {
    val empty = AttachmentState(capability = CapabilityUi(true))
    val loading = empty.copy(drafts = listOf(AttachmentCardState("upload-1", "Trail.jpg", "Uploading", busy = true)))
    val populated =
        empty.copy(
            messages =
                mapOf(
                    "file-1" to
                        AttachmentCardState(
                            "file-1",
                            "Trail.jpg",
                            "Verified",
                            canOpen = true,
                            mimeType = "image/jpeg",
                            verifiedSizeBytes = 120000,
                        ),
                ),
            verifiedMessageIds = setOf("file-1"),
        )
    val failure = empty.copy(failure = AppFailure("Cannot download this file. Try again.", "attachment_network"))
    val unknown =
        empty.copy(
            drafts = listOf(AttachmentCardState("draft-1", "Notes.txt", "Review send outcome", unknownOutcome = true)),
        )
    val expired =
        empty.copy(
            drafts = listOf(AttachmentCardState("draft-2", "Notes.txt", "Unavailable", unavailable = true)),
        )
}
