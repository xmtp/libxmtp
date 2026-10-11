package org.xmtp.android.example.shared.feature.conversations

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.ConversationRow
import org.xmtp.android.example.shared.FailureRecovery
import org.xmtp.android.example.shared.MessengerAvatar

internal val ConversationRow.displayTitle: String
    get() = alias?.takeIf { it.isNotBlank() } ?: title.ifBlank { if (group) "Group conversation" else "Conversation" }

@Composable
internal fun ConversationItem(row: ConversationRow, selected: Boolean, onClick: () -> Unit) {
    Surface(color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface) {
        Row(
            Modifier.fillMaxWidth().clickable(onClick = onClick).semantics(mergeDescendants = true) { role = Role.Button; this.selected = selected }.padding(horizontal = 20.dp, vertical = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            verticalAlignment = Alignment.Top,
        ) {
            MessengerAvatar(row.displayTitle, group = row.group)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                Text(row.displayTitle, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                val preview = row.preview.ifBlank { "Start a conversation" }
                Text(row.senderPreview?.takeIf { it.isNotBlank() }?.let { "$it: $preview" } ?: preview, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (row.time.isNotBlank()) Text(row.time, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
                    if (row.muted == true) Text("Muted", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (row.unread.isNotBlank() && row.unread != "0") {
                    Surface(color = MaterialTheme.colorScheme.primaryContainer, shape = RoundedCornerShape(8.dp)) {
                        Text("${row.unread} unread", Modifier.padding(horizontal = 8.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onPrimaryContainer)
                    }
                }
            }
        }
    }
}

@Composable
internal fun ConversationFailure(failure: AppFailure, busy: Boolean = false, retry: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.fillMaxWidth().padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite }) {
            Text(failure.message, color = MaterialTheme.colorScheme.onErrorContainer)
            if (failure.recovery != FailureRecovery.NONE) {
                TextButton(onClick = retry, enabled = !busy, modifier = Modifier.heightIn(min = 48.dp)) { Text("Retry") }
            }
        }
    }
}

@Composable
internal fun ConversationEmpty(title: String, detail: String, button: String? = null, onClick: () -> Unit = {}) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 28.dp, vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(title, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
        Text(detail, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (button != null) Button(onClick = onClick, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(button) }
    }
}
