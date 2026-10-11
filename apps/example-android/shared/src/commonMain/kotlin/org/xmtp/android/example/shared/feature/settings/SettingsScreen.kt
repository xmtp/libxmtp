package org.xmtp.android.example.shared.feature.settings
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable fun SettingsScreen(
    state: SettingsState,
    action: (SettingsAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    var reset by remember {
        mutableStateOf(false)
    }
    LazyColumn(
        contentPadding =
            PaddingValues(
                20.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    16.dp,
                ),
    ) {
        item {
            Text("Backend", fontWeight = FontWeight.Bold)
            Text(
                state.backend,
            )
            Text("Inbox ID", fontWeight = FontWeight.Bold)
            Text(
                state.inbox,
            )
        }
        item {
            Text("Unread counts use local insertion time. Equal timestamps and imports can change these counts.")
        }
        item { Text("Notifications: ${state.notificationStatus}") }
        if (state.notifications.available) {
            item {
                Action(if (state.notificationsEnabled) "Turn notifications off" else "Turn notifications on") {
                    action(
                        SettingsAction.SetNotifications(!state.notificationsEnabled),
                    )
                }
            }
        }
        if (state.attachments.available) {
            item {
                Action("Draft recovery") {
                    shellAction(ShellAction.Navigate(AppRoute.AttachmentRecovery))
                }
            }
        }
        items(
            state.unknownSends,
            key = {
                it.draftId
            },
        ) { draft ->
            Column {
                Notice("Send outcome unknown. The original message may already exist.")
                Action("View chat") {
                    shellAction(ShellAction.Navigate(AppRoute.Chat(draft.conversationId)))
                }
                TextButton(
                    {
                        action(
                            SettingsAction
                                .DiscardUnknownSend(
                                    draft.draftId,
                                ),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Discard record")
                }
            }
        }
        item {
            Action("Sign out") {
                action(
                    SettingsAction.SignOut,
                )
            }
        }
        item {
            Action("Delete my account") {
                reset = true
            }
        }
        item {
            Text("This app is for testing and debugging purposes only.")
        }
    }
    if (reset) {
        AlertDialog(
            onDismissRequest = {
                reset = false
            },
            title = {
                Text("Delete local account?")
            },
            text = {
                Text("This removes this backend's identity, SDK database and local files from this device.")
            },
            confirmButton = {
                TextButton({
                    reset = false
                    action(
                        SettingsAction.DeleteLocalData,
                    )
                }) {
                    Text("Delete")
                }
            },
            dismissButton = {
                TextButton({
                    reset = false
                }) {
                    Text("Cancel")
                }
            },
        )
    }
}
