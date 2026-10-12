package org.xmtp.android.example.shared.feature.chat

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.xmtp.android.example.shared.*

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun MessageActions(
    row: MessageRow,
    maximumHeight: Dp,
    enabled: Boolean,
    outboundEnabled: Boolean,
    action: (ChatAction) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    val haptics = LocalHapticFeedback.current
    var confirmDelete by remember(row.id) { mutableStateOf(false) }
    Surface(
        Modifier.testTag("reaction-popover").widthIn(max = 304.dp).heightIn(max = maximumHeight.coerceAtLeast(48.dp)),
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(24.dp),
        shadowElevation = 8.dp,
    ) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(8.dp)) {
            Row {
                listOf("👍", "❤️", "😂", "😮", "😢").forEach { emoji ->
                    TextButton(
                        onClick = {
                            val mine = row.reactions.firstOrNull { it.emoji == emoji }?.mine == true
                            action(ChatAction.React(row.id, emoji, mine))
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            action(ChatAction.DismissOverlay)
                        },
                        modifier =
                            Modifier.size(
                                48.dp,
                            ),
                        enabled = enabled && outboundEnabled,
                        contentPadding = PaddingValues(0.dp),
                    ) {
                        Text(emoji, fontSize = with(LocalDensity.current) { (24 / fontScale).sp })
                    }
                }
                IconButton({
                    action(ChatAction.ShowEmoji(row.id))
                }, Modifier.size(48.dp), enabled = enabled && outboundEnabled) { Icon(AppIcons.More, "More reactions") }
            }
            HorizontalDivider(Modifier.padding(horizontal = 8.dp), color = MaterialTheme.colorScheme.outlineVariant)
            FlowRow(Modifier.fillMaxWidth()) {
                TextButton({
                    clipboard.setText(AnnotatedString(row.text))
                    action(ChatAction.DismissOverlay)
                }, Modifier.heightIn(min = 48.dp), enabled = enabled) { Text("Copy") }
                TextButton({
                    action(ChatAction.Reply(row.id))
                    action(ChatAction.DismissOverlay)
                }, Modifier.heightIn(min = 48.dp), enabled = enabled && outboundEnabled) { Text("Reply") }
                if (row.mine &&
                    row.sendState == MessageSendState.SENT
                ) {
                    TextButton(
                        { confirmDelete = true },
                        Modifier.heightIn(min = 48.dp),
                        enabled = enabled && outboundEnabled,
                    ) { Text("Delete") }
                }
                if (row.mine &&
                    (row.sendState == MessageSendState.QUEUED || row.sendState == MessageSendState.NOT_SENT)
                ) {
                    TextButton(
                        {
                            action(ChatAction.RetrySend(row.id))
                            action(ChatAction.DismissOverlay)
                        },
                        Modifier.heightIn(min = 48.dp).testTag("message-retry-${row.id}"),
                        enabled =
                            enabled && outboundEnabled,
                    ) {
                        Text("Retry")
                    }
                    TextButton({
                        action(ChatAction.EditFailedSend(row.id))
                        action(ChatAction.DismissOverlay)
                    }, Modifier.heightIn(min = 48.dp), enabled = enabled && outboundEnabled) { Text("Edit") }
                }
            }
        }
    }
    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete message?") },
            text = {
                Text(
                    "This sends a deletion request to this conversation. Other members may already have a copy.",
                )
            },
            confirmButton = {
                TextButton({
                    confirmDelete = false
                    action(ChatAction.DeleteMessage(row.id))
                    action(ChatAction.DismissOverlay)
                }, enabled = enabled && outboundEnabled) {
                    Text(
                        "Delete message",
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            },
            dismissButton = { TextButton({ confirmDelete = false }) { Text("Cancel") } },
        )
    }
}
