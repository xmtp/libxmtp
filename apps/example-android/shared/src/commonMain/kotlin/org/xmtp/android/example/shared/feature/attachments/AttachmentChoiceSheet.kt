package org.xmtp.android.example.shared.feature.attachments

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.AppIcons
import org.xmtp.android.example.shared.attachments.AttachmentAction
import org.xmtp.android.example.shared.attachments.AttachmentSelection
import org.xmtp.android.example.shared.designsystem.MessengerSheet

@Composable
fun AttachmentChoiceSheet(
    state: AttachmentChoiceState,
    visible: Boolean,
    onHidden: () -> Unit,
    action: (AttachmentAction) -> Unit,
) {
    val dismiss = { action(AttachmentAction.DismissChoice(state.instanceId)) }
    MessengerSheet(onDismiss = dismiss, visible = visible, onHidden = onHidden) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .testTag("attachment-choice-sheet")
                .padding(horizontal = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Add an attachment", Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                IconButton(dismiss, Modifier.size(48.dp), enabled = visible) {
                    Icon(AppIcons.Close, "Close attachment choices")
                }
            }
            Text(
                "Choose a photo, video, or file from this device.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyLarge,
            )
            FilledTonalButton(
                onClick = { action(AttachmentAction.Choose(state.instanceId, AttachmentSelection.PHOTO_VIDEO)) },
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag("attachment-choice-photo-video"),
                enabled = visible,
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 14.dp),
            ) { Text("Photo/video", style = MaterialTheme.typography.bodyLarge) }
            OutlinedButton(
                onClick = { action(AttachmentAction.Choose(state.instanceId, AttachmentSelection.FILE)) },
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag("attachment-choice-file"),
                enabled = visible,
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 14.dp),
            ) { Text("File", style = MaterialTheme.typography.bodyLarge) }
            Spacer(Modifier.height(24.dp))
        }
    }
}
