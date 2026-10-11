package org.xmtp.android.example.shared.feature.conversations

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable
fun RequestsScreen(
    state: RequestState,
    action: (RequestAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val clipboard = LocalClipboardManager.current
    var showSender by remember(state.conversationId) { mutableStateOf(false) }
    Surface(
        color = MaterialTheme.colorScheme.primaryContainer,
        shape = RoundedCornerShape(20.dp),
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                "Message request",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
            )
            Text(
                "Accept to reply. Request media stays closed until you choose to download it.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
            )
            state.failure?.let {
                Text(
                    it.message,
                    color = MaterialTheme.colorScheme.error,
                    modifier =
                        Modifier.semantics {
                            liveRegion =
                                LiveRegionMode.Polite
                        },
                )
            }
            Button(onClick = {
                action(RequestAction.Accept)
            }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Accept") }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = {
                    action(RequestAction.DeleteLocal)
                }, enabled = state.deleteCapability.available, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) {
                    Text("Delete request")
                }
                TextButton(
                    onClick = {
                        action(RequestAction.Block)
                    },
                    modifier =
                        Modifier
                            .weight(
                                1f,
                            ).heightIn(
                                min = 48.dp,
                            ),
                    colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                ) {
                    Text("Block")
                }
            }
            if (!state.deleteCapability.available) {
                Text(
                    state.deleteCapability.reason ?: "Local request removal is unavailable.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                )
            }
            if (state.senderInboxId.isNotBlank()) {
                TextButton(
                    onClick = {
                        showSender = !showSender
                    },
                    modifier =
                        Modifier.heightIn(
                            min = 48.dp,
                        ),
                ) { Text(if (showSender) "Hide sender details" else "Sender details") }
                if (showSender) {
                    Text(
                        "Sender inbox ID",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onPrimaryContainer,
                    )
                    Text(
                        state.senderInboxId,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onPrimaryContainer,
                    )
                    TextButton(onClick = {
                        clipboard.setText(AnnotatedString(state.senderInboxId))
                    }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Copy inbox ID") }
                }
            }
        }
    }
    state.pendingDecision?.let { decision ->
        val title =
            when (decision) {
                RequestDecision.ACCEPT -> "Accept this request?"
                RequestDecision.DELETE_LOCAL -> "Delete this request?"
                RequestDecision.BLOCK -> "Block this conversation?"
            }
        val detail =
            when (decision) {
                RequestDecision.ACCEPT -> {
                    "Move this conversation to Chats so you can reply."
                }

                RequestDecision.DELETE_LOCAL -> {
                    "Remove this request from this device. " +
                        "This action does not remove the conversation for other people."
                }

                RequestDecision.BLOCK -> {
                    "Mark this conversation as blocked. It will leave Chats and message requests."
                }
            }
        AlertDialog(
            onDismissRequest = { action(RequestAction.Cancel) },
            title = { Text(title) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(detail)
                    state.failure?.let { Text(it.message, color = MaterialTheme.colorScheme.error) }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    action(RequestAction.Confirm)
                }, modifier = Modifier.heightIn(min = 48.dp)) {
                    Text(
                        when (decision) {
                            RequestDecision.ACCEPT -> "Accept"
                            RequestDecision.DELETE_LOCAL -> "Delete request"
                            RequestDecision.BLOCK -> "Block"
                        },
                    )
                }
            },
            dismissButton = {
                TextButton(
                    onClick = { action(RequestAction.Cancel) },
                    modifier = Modifier.heightIn(min = 48.dp),
                ) { Text("Cancel") }
            },
        )
    }
}
