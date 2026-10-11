package org.xmtp.android.example.shared

import android.content.res.Configuration
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.attachments.AttachmentCard
import org.xmtp.android.example.shared.attachments.AttachmentCardState
import org.xmtp.android.example.shared.designsystem.*
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceSheet
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceState
import org.xmtp.android.example.shared.feature.attachments.AttachmentPhase

class MessengerPreviewProvider : PreviewParameterProvider<MessengerPreviewCase> {
    override val values = messengerPreviewCases.asSequence()
}

@Preview(name = "Phone", widthDp = 360, heightDp = 780, showBackground = true)
@Preview(
    name = "Dark small phone",
    widthDp = 320,
    heightDp = 680,
    uiMode = Configuration.UI_MODE_NIGHT_YES,
    showBackground = true,
)
@Preview(name = "Large text", widthDp = 320, heightDp = 680, fontScale = 2f, showBackground = true)
@Preview(name = "Landscape", widthDp = 760, heightDp = 400, showBackground = true)
@Preview(name = "Wide window", widthDp = 1100, heightDp = 760, showBackground = true)
@Composable
fun MessengerScreenPreview(
    @PreviewParameter(MessengerPreviewProvider::class) case: MessengerPreviewCase,
) {
    MessengerPreview(case)
}

@Preview(
    name = "RTL dark large text",
    widthDp = 320,
    heightDp = 680,
    fontScale = 2f,
    locale = "ar",
    uiMode = Configuration.UI_MODE_NIGHT_YES,
    showBackground = true,
)
@Composable
fun MessengerRtlPreview(
    @PreviewParameter(MessengerPreviewProvider::class) case: MessengerPreviewCase,
) {
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) { MessengerPreview(case) }
}

class AttachmentPreviewProvider : PreviewParameterProvider<AttachmentCardState> {
    override val values =
        sequenceOf(
            AttachmentCardState(
                "file-waiting",
                "Meeting notes.pdf",
                "Waiting",
                canSend = true,
                canDiscard = true,
                mimeType = "application/pdf",
            ),
            AttachmentCardState(
                "file-upload",
                "Trail photo.jpg",
                "Uploading",
                busy = true,
                mimeType = "image/jpeg",
                transferredBytes = 24000,
                totalBytes = 120000,
                phase = AttachmentPhase.UPLOADING,
            ),
            AttachmentCardState(
                "file-download",
                "Trip video.mp4",
                "Downloading",
                busy = true,
                mimeType = "video/mp4",
                phase = AttachmentPhase.DOWNLOADING,
            ),
            AttachmentCardState(
                "file-ready",
                "Meeting notes.pdf",
                "Verified",
                canOpen = true,
                canSave = true,
                mimeType = "application/pdf",
                verifiedSizeBytes = 120000,
                phase = AttachmentPhase.VERIFIED,
            ),
            AttachmentCardState(
                "file-failure",
                "Meeting notes.pdf",
                "Failed",
                canDownload = true,
                error = "Cannot download this file. Try again.",
                phase = AttachmentPhase.FAILED,
            ),
            AttachmentCardState(
                "file-unknown",
                "Meeting notes.pdf",
                "Review send outcome",
                unknownOutcome = true,
                phase = AttachmentPhase.UNKNOWN,
            ),
            AttachmentCardState(
                "file-expired",
                "Meeting notes.pdf",
                "Unavailable",
                unavailable = true,
                phase = AttachmentPhase.EXPIRED,
            ),
        )
}

@Preview(name = "Attachment choices", widthDp = 360, heightDp = 680, showBackground = true)
@Preview(
    name = "Attachment choices dark large text",
    widthDp = 320,
    heightDp = 680,
    fontScale = 2f,
    uiMode = Configuration.UI_MODE_NIGHT_YES,
    showBackground = true,
)
@Composable
fun MessengerAttachmentChoicePreview() {
    MessengerTheme(Appearance.SYSTEM, reducedMotion = true) {
        AttachmentChoiceSheet(AttachmentChoiceState(1, "sample-conversation"), true, {}, {})
    }
}

@Preview(name = "File", widthDp = 360, showBackground = true)
@Preview(name = "File dark", widthDp = 320, uiMode = Configuration.UI_MODE_NIGHT_YES, showBackground = true)
@Preview(name = "File large text", widthDp = 320, fontScale = 2f, showBackground = true)
@Composable
fun MessengerAttachmentPreview(
    @PreviewParameter(AttachmentPreviewProvider::class) state: AttachmentCardState,
) {
    MessengerTheme(Appearance.SYSTEM, reducedMotion = true) {
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Shared file")
            AttachmentCard(state, "sample-conversation", {})
        }
    }
}
