package org.xmtp.android.example.shared.attachments

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.ConversationRow
import org.xmtp.android.example.shared.feature.attachments.AttachmentPhase

@Composable
fun AttachmentComposer(
    cards: List<AttachmentCardState>,
    conversationId: String?,
    error: String?,
    action: (AttachmentAction) -> Unit,
) {
    val activeCards = cards.filter { it.conversationId == conversationId }
    if (activeCards.isEmpty() && error == null) return
    LazyColumn(
        Modifier.fillMaxWidth().heightIn(max = 112.dp).testTag("attachment-composer-drafts"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        error?.let {
            item {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
        items(activeCards, key = { it.id }) { card -> AttachmentCard(card, card.conversationId, action) }
    }
}

@Composable
fun AttachmentRecovery(
    cards: List<AttachmentCardState>,
    conversations: List<ConversationRow>,
    error: String?,
    action: (AttachmentAction) -> Unit,
) {
    LazyColumn(
        Modifier.fillMaxSize().testTag("attachment-recovery"),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            Text("Files that need review", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(8.dp))
            Text(
                "Upload drafts are retained for 24 hours by default. " +
                    "Review an interrupted send before you send another file.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            error?.let {
                Spacer(Modifier.height(8.dp))
                Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
            }
        }
        if (cards.isEmpty()) {
            item {
                Surface(
                    Modifier.fillMaxWidth(),
                    shape = MaterialTheme.shapes.large,
                    color = MaterialTheme.colorScheme.surface,
                ) {
                    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("No retained drafts", style = MaterialTheme.typography.titleLarge)
                        Text(
                            "There are no retained file drafts to review.",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        items(cards, key = { it.id }) { card ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                AttachmentCard(card.copy(canSend = false), card.conversationId, action)
                if (card.conversationId.isEmpty()) {
                    Surface(
                        Modifier.fillMaxWidth(),
                        shape = MaterialTheme.shapes.large,
                        color = MaterialTheme.colorScheme.surface,
                    ) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("Choose a chat for this draft", style = MaterialTheme.typography.titleMedium)
                            if (conversations.isEmpty()) {
                                Text(
                                    "Start a conversation to assign this draft.",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            conversations.forEach { chat ->
                                TextButton(
                                    {
                                        action(AttachmentAction.Assign(card.id, chat.id))
                                    },
                                    Modifier.fillMaxWidth().heightIn(
                                        min = 48.dp,
                                    ),
                                    enabled = !card.busy,
                                ) { Text("Assign to ${chat.title}") }
                            }
                        }
                    }
                } else if (!card.unknownOutcome && card.acceptedMessageId == null) {
                    TextButton({
                        action(AttachmentAction.ViewChat(card.conversationId))
                    }, Modifier.heightIn(min = 48.dp)) { Text("View chat") }
                }
            }
        }
    }
}
