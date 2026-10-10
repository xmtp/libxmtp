package org.xmtp.android.example.shared.attachments

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AttachmentCard(
    state: AttachmentCardState,
    conversationId: String,
    action: (AttachmentAction) -> Unit,
    showFilename: Boolean = true,
) {
    Column {
        if (showFilename) Text(state.filename)
        Text(state.status)
        state.error?.let { Text(it) }
        if (state.busy) CircularProgressIndicator()
        if (state.unknownOutcome) Text("The original message may already be in the chat.")
        if (state.unavailable) Text("Draft expired or unavailable. Select a file again to make a new draft.")
        FlowRow(Modifier.fillMaxWidth()) {
            if (state.canSend) {
                TextButton(onClick = {
                    action(AttachmentAction.Send(state.id))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Send file") }
            }
            if (state.acceptedMessageId !=
                null
            ) {
                TextButton(onClick = {
                    action(AttachmentAction.RetryPublication(state.id, conversationId))
                }, enabled = !state.busy, modifier = Modifier.heightIn(min = 48.dp)) { Text("Retry publication") }
                TextButton(onClick = {
                    action(AttachmentAction.ViewChat(conversationId))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("View chat") }
            }
            if (state.canDownload) {
                TextButton(onClick = {
                    action(AttachmentAction.Download(state.id))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Download") }
            }
            if (state.canOpen) {
                TextButton(onClick = {
                    action(AttachmentAction.Open(state.id))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Open") }
                TextButton(onClick = {
                    action(AttachmentAction.Save(state.id))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Save") }
            }
            if (state.unknownOutcome) {
                TextButton(onClick = {
                    action(AttachmentAction.ViewChat(conversationId))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("View chat") }
            }
            if (state.canDiscard) {
                TextButton(onClick = {
                    action(AttachmentAction.Discard(state.id))
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Discard") }
            }
        }
    }
}
