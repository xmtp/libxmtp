package org.xmtp.android.example.shared.attachments

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.feature.attachments.AttachmentPhase

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AttachmentCard(
    state: AttachmentCardState,
    conversationId: String,
    action: (AttachmentAction) -> Unit,
    showFilename: Boolean = true,
) {
    Surface(
        Modifier.fillMaxWidth().testTag("attachment-card-${state.id}"),
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.primaryContainer) {
                    Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
                        Text(
                            "FILE",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onPrimaryContainer,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (showFilename) {
                        Text(
                            state.filename,
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                    val details =
                        buildList {
                            add(attachmentType(state.mimeType))
                            state.verifiedSizeBytes?.takeIf { it >= 0 }?.let { add(attachmentSize(it)) }
                        }
                    Text(
                        details.joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                state.status,
                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                style = MaterialTheme.typography.bodyMedium,
                color =
                    if (state.phase ==
                        AttachmentPhase.FAILED
                    ) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
            )
            state.error?.let {
                Text(
                    it,
                    Modifier.semantics {
                        liveRegion = LiveRegionMode.Polite
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            if (state.busy) {
                val transferred = state.transferredBytes
                val total = state.totalBytes
                if (transferred != null && total != null && total > 0 && transferred >= 0 && transferred <= total) {
                    LinearProgressIndicator(
                        progress = { transferred.toFloat() / total.toFloat() },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        "${attachmentSize(transferred)} of ${attachmentSize(total)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                }
            }
            if (state.unknownOutcome) {
                Text(
                    "The original message may already be in the chat. Review it before you send another file.",
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            if (state.unavailable) {
                Text(
                    "Draft expired or unavailable. Select a file again to make a new draft.",
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (state.canSend) AttachmentButton("Send file") { action(AttachmentAction.Send(state.id)) }
                if (state.acceptedMessageId != null) {
                    AttachmentButton("Retry sending", enabled = !state.busy) {
                        action(AttachmentAction.RetryPublication(state.id, conversationId))
                    }
                }
                if (state.canDownload) {
                    AttachmentButton(
                        if (state.phase ==
                            AttachmentPhase.FAILED
                        ) {
                            "Retry download"
                        } else {
                            "Download"
                        },
                    ) { action(AttachmentAction.Download(state.id)) }
                }
                if (state.canOpen) AttachmentButton("Open") { action(AttachmentAction.Open(state.id)) }
                if (state.canSave) AttachmentButton("Save") { action(AttachmentAction.Save(state.id)) }
                if (state.acceptedMessageId != null ||
                    state.unknownOutcome
                ) {
                    AttachmentButton("View chat") { action(AttachmentAction.ViewChat(conversationId)) }
                }
                if (state.canDiscard) AttachmentButton("Discard") { action(AttachmentAction.Discard(state.id)) }
            }
        }
    }
}

@Composable
private fun AttachmentButton(
    label: String,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    TextButton(onClick, Modifier.heightIn(min = 48.dp), enabled = enabled) { Text(label) }
}

internal fun attachmentType(mimeType: String?): String =
    when {
        mimeType?.startsWith("image/") == true -> "Image"
        mimeType?.startsWith("video/") == true -> "Video"
        mimeType?.startsWith("audio/") == true -> "Audio"
        mimeType == "application/pdf" -> "PDF document"
        mimeType?.startsWith("text/") == true -> "Text file"
        else -> "File"
    }

internal fun attachmentSize(bytes: Long): String =
    when {
        bytes < 1024 -> "$bytes B"
        bytes < 1024 * 1024 -> "${bytes / 1024} KB"
        bytes < 1024 * 1024 * 1024 -> "${bytes / (1024 * 1024)} MB"
        else -> "${bytes / (1024 * 1024 * 1024)} GB"
    }
