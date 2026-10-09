package org.xmtp.android.example.shared.attachments

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.ConversationRow

@Composable
fun AttachmentComposer(
    cards: List<AttachmentCardState>,
    conversationId: String?,
    error: String?,
    action: (AttachmentAction) -> Unit,
) {
    Column {
        error?.let { Text(it) }
        cards.filter { it.conversationId == conversationId }.forEach { card ->
            AttachmentCard(card, card.conversationId, action)
        }
    }
}

@Composable
fun AttachmentMessage(
    card: AttachmentCardState,
    conversationId: String,
    preview: ImageBitmap?,
    action: (AttachmentAction) -> Unit,
    showFilename: Boolean = true,
) {
    Column {
        if (preview != null) Image(preview, card.filename, Modifier.fillMaxWidth().heightIn(max = 240.dp))
        AttachmentCard(card, conversationId, action, showFilename)
    }
}

@Composable
fun AttachmentRecovery(
    cards: List<AttachmentCardState>,
    conversations: List<ConversationRow>,
    error: String?,
    action: (AttachmentAction) -> Unit,
) {
    LazyColumn {
        item {
            Text("Upload drafts are retained for 24 hours by default. An interrupted send needs review.")
            error?.let { Text(it) }
        }
        items(cards, key = { it.id }) { card ->
            AttachmentCard(card.copy(canSend = false), card.conversationId, action)
            if (card.conversationId.isEmpty()) {
                conversations.forEach { chat ->
                    TextButton({
                        action(AttachmentAction.Assign(card.id, chat.id))
                    }, Modifier.heightIn(min = 48.dp)) { Text("Assign to ${chat.title}") }
                }
            } else if (!card.unknownOutcome &&
                card.acceptedMessageId == null
            ) {
                TextButton({
                    action(AttachmentAction.ViewChat(card.conversationId))
                }, Modifier.heightIn(min = 48.dp)) { Text("View chat") }
            }
        }
    }
}
