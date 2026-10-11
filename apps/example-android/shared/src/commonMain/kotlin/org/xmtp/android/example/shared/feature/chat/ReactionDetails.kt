package org.xmtp.android.example.shared.feature.chat

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.designsystem.MessengerSheet

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReactionDetails(row: MessageRow, emoji: String?, instanceId: Long, visible: Boolean, onHidden: () -> Unit, action: (ChatAction) -> Unit) {
    val close = { action(ChatAction.CloseSheet(row.id, MessageSurface.REACTORS, instanceId)) }
    val haptics = LocalHapticFeedback.current
    val reaction = row.reactions.firstOrNull { it.emoji == emoji }
    MessengerSheet(onDismiss = close, visible = visible, onHidden = onHidden) {
        Column(Modifier.fillMaxWidth().testTag("reaction-details").padding(horizontal = 24.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("${reaction?.emoji.orEmpty()} Reactions", Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                IconButton(close, Modifier.size(48.dp), enabled = visible) { Icon(AppIcons.Close, "Close reactions") }
            }
            if (reaction?.reactors.isNullOrEmpty()) {
                Text("The names of people who reacted are not available.", Modifier.padding(vertical = 24.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                LazyColumn(Modifier.heightIn(max = 360.dp), contentPadding = PaddingValues(vertical = 16.dp)) {
                    items(reaction!!.reactors, key = { it.inboxId }) { person ->
                        Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                            MessengerAvatar(person.displayName)
                            Text(person.displayName, Modifier.weight(1f).padding(start = 12.dp), style = MaterialTheme.typography.bodyLarge)
                        }
                    }
                }
            }
            if (reaction != null) TextButton({ action(ChatAction.React(row.id, reaction.emoji, reaction.mine)); haptics.performHapticFeedback(HapticFeedbackType.LongPress); action(ChatAction.DismissOverlay) }, Modifier.heightIn(min = 48.dp).fillMaxWidth(), enabled = visible) { Text(if (reaction.mine) "Remove my reaction" else "Add this reaction") }
            Spacer(Modifier.height(24.dp))
        }
    }
}
