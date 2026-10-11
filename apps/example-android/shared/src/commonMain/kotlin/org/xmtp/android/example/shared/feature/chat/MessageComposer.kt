package org.xmtp.android.example.shared.feature.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.designsystem.*

@Composable
internal fun MessageComposer(
    state: ChatState,
    action: (ChatAction) -> Unit,
    extra: @Composable () -> Unit,
) {
    val composer = state.composer
    val duration = if (LocalReducedMotion.current) 0 else MessengerMotion.REPLY
    Surface(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp).testTag("message-composer"),
        shape = RoundedCornerShape(24.dp),
        color = MaterialTheme.colorScheme.surface,
        shadowElevation = 2.dp,
    ) {
        Column {
            AnimatedVisibility(
                composer.reply != null,
                enter = expandVertically(tween(duration)),
                exit = shrinkVertically(tween(duration)),
            ) {
                Row(
                    Modifier.fillMaxWidth().padding(start = 16.dp, top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    VerticalDivider(Modifier.height(36.dp), thickness = 3.dp, color = MaterialTheme.colorScheme.primary)
                    Column(Modifier.weight(1f).padding(start = 10.dp)) {
                        Text(
                            "Reply to ${composer.reply?.sender.orEmpty()}",
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            color = MaterialTheme.colorScheme.primary,
                            style = MaterialTheme.typography.labelLarge,
                        )
                        Text(
                            composer.reply?.firstLine.orEmpty(),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    IconButton(
                        { action(ChatAction.Reply(null)) },
                        Modifier.size(48.dp),
                    ) { Icon(AppIcons.Close, "Cancel reply") }
                }
            }
            Box(Modifier.weight(1f, fill = false).fillMaxWidth().heightIn(max = 112.dp)) { extra() }
            Row(Modifier.padding(horizontal = 4.dp, vertical = 4.dp), verticalAlignment = Alignment.Bottom) {
                if (state.features.attachments &&
                    !state.unknown
                ) {
                    IconButton(
                        { action(ChatAction.SelectAttachment) },
                        Modifier.size(48.dp),
                    ) { Icon(AppIcons.Add, "Add attachment") }
                }
                TextField(
                    value =
                        TextFieldValue(
                            composer.text,
                            TextRange(
                                composer.selectionStart.coerceIn(0, composer.text.length),
                                composer.selectionEnd.coerceIn(0, composer.text.length),
                            ),
                            composition =
                                composer.compositionStart?.let { start ->
                                    composer.compositionEnd?.let { end ->
                                        TextRange(
                                            start.coerceIn(0, composer.text.length),
                                            end.coerceIn(0, composer.text.length),
                                        )
                                    }
                                },
                        ),
                    onValueChange = {
                        action(
                            ChatAction.ChangeDraft(
                                it.text,
                                it.selection.start,
                                it.selection.end,
                                it.composition?.start,
                                it.composition?.end,
                            ),
                        )
                    },
                    modifier = Modifier.weight(1f).heightIn(max = 180.dp),
                    placeholder = { Text("Message", style = MaterialTheme.typography.bodyLarge) },
                    textStyle = MaterialTheme.typography.bodyLarge,
                    maxLines = 5,
                    enabled = composer.enabled,
                    colors =
                        TextFieldDefaults.colors(
                            focusedContainerColor = MaterialTheme.colorScheme.surface,
                            unfocusedContainerColor = MaterialTheme.colorScheme.surface,
                            disabledContainerColor = MaterialTheme.colorScheme.surface,
                            focusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                            unfocusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                        ),
                )
                FilledIconButton(
                    onClick = { action(ChatAction.SendText(composer.text, 0, composer.revision)) },
                    modifier = Modifier.padding(bottom = 4.dp, end = 4.dp).size(48.dp),
                    enabled =
                        composer.text.isNotBlank() && composer.enabled && composer.pendingRequestId == null &&
                            !state.busy,
                ) { Icon(AppIcons.Send, "Send message") }
            }
        }
    }
}
