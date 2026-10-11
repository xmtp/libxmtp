package org.xmtp.android.example.shared.feature.settings

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable
fun SettingsScreen(
    state: SettingsState,
    action: (SettingsAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    var reset by remember { mutableStateOf(false) }
    var signOut by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    LazyColumn(
        Modifier.fillMaxSize().testTag("app-settings"),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            SettingsSection("Profile") {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    MessengerAvatar("You", size = 56.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Your profile", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                        Text(
                            "Your identity on this server",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                HorizontalDivider()
                Text(
                    "Inbox ID",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                SelectionContainer {
                    Text(
                        state.inbox.ifBlank { "No active identity" },
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                TextButton({
                    clipboard.setText(AnnotatedString(state.inbox))
                }, Modifier.heightIn(min = 48.dp), enabled = state.inbox.isNotBlank()) { Text("Copy inbox ID") }
            }
        }
        state.failure?.let { item { Notice(it.message) } }
        if (state.busy) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
        if (state.pendingReset) {
            item {
                SettingsSection("Local deletion is incomplete") {
                    Text(
                        "Continue to remove this server's identity, saved messages, and local files from this device.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Action("Continue deletion", !state.busy) { action(SettingsAction.ResumeReset) }
                }
            }
        }
        item {
            SettingsSection("Notifications") {
                Text(
                    "Get alerts when new messages arrive. You can use Chats with notifications off.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(state.notificationStatus, style = MaterialTheme.typography.bodyLarge)
                if (state.notificationPermission == NotificationPermission.DENIED) {
                    Text(
                        "Notification permission is off. You can enable it in Android settings.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                if (state.notifications.available) {
                    TextButton(
                        { action(SettingsAction.SetNotifications(!state.notificationsEnabled)) },
                        Modifier.heightIn(min = 48.dp),
                        enabled =
                            !state.busy && !state.pendingReset,
                    ) { Text(if (state.notificationsEnabled) "Turn notifications off" else "Turn notifications on") }
                } else {
                    Text(
                        state.notifications.reason ?: "Notifications are not configured for this app.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
        item {
            SettingsSection("Server") {
                Text(state.serverLabel.ifBlank { "Connected server" }, style = MaterialTheme.typography.titleMedium)
                SelectionContainer {
                    Text(
                        state.backend.ifBlank {
                            "No server selected"
                        },
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                TextButton({
                    clipboard.setText(AnnotatedString(state.backend))
                }, Modifier.heightIn(min = 48.dp), enabled = state.backend.isNotBlank()) { Text("Copy server address") }
            }
        }
        item {
            SettingsSection("Appearance") {
                Text(
                    "Choose how Chats looks on this device.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Appearance.entries.forEach { appearance ->
                    val label =
                        when (appearance) {
                            Appearance.SYSTEM -> "Use device setting"
                            Appearance.LIGHT -> "Light"
                            Appearance.DARK -> "Dark"
                        }
                    Surface(
                        onClick = { action(SettingsAction.SetAppearance(appearance)) },
                        enabled = !state.pendingReset,
                        modifier =
                            Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics {
                                role = Role.RadioButton
                                selected = state.appearance == appearance
                            },
                        shape = MaterialTheme.shapes.medium,
                        color =
                            if (state.appearance ==
                                appearance
                            ) {
                                MaterialTheme.colorScheme.primaryContainer
                            } else {
                                MaterialTheme.colorScheme.surface
                            },
                    ) {
                        Row(
                            Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            RadioButton(state.appearance == appearance, onClick = null)
                            Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
                        }
                    }
                }
            }
        }
        item {
            SettingsSection("Privacy") {
                Text("Message requests let you choose who can contact you.", style = MaterialTheme.typography.bodyLarge)
                Text(
                    "Files are verified before they can be opened or saved. " +
                        "Request files are not downloaded automatically.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton({
                    shellAction(ShellAction.Navigate(AppRoute.Requests))
                }, Modifier.heightIn(min = 48.dp)) { Text("Message requests") }
            }
        }
        item {
            SettingsSection("Developer tools") {
                Text("Local recovery and app diagnostics", style = MaterialTheme.typography.bodyLarge)
                Text(
                    "Unread counts use local insertion time. " +
                        "Equal timestamps and imported messages can change these counts.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.attachments.available) {
                    TextButton({
                        shellAction(ShellAction.Navigate(AppRoute.AttachmentRecovery))
                    }, Modifier.heightIn(min = 48.dp)) { Text("Draft recovery") }
                } else {
                    Text(
                        state.attachments.reason ?: "File attachments are not available on this server.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                Text(
                    "Open conversation Details for raw metadata fields.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        items(state.unknownSends, key = { it.draftId }) { draft ->
            SettingsSection("Send needs review") {
                Text(
                    "Send outcome unknown. The original message may already exist.",
                    style = MaterialTheme.typography.bodyLarge,
                )
                TextButton({
                    shellAction(ShellAction.Navigate(AppRoute.Chat(draft.conversationId)))
                }, Modifier.heightIn(min = 48.dp)) { Text("View chat") }
                Text(
                    "Clear the saved send status after you check the conversation. " +
                        "This does not delete or cancel the original message.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton({
                    action(SettingsAction.DiscardUnknownSend(draft.draftId))
                }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Clear saved send status") }
            }
        }
        item {
            SettingsSection("This device") {
                Text("Sign out", style = MaterialTheme.typography.titleMedium)
                Text(
                    "End this session and clear its access token. " +
                        "Your identity and saved messages stay on this device.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedButton(
                    { signOut = true },
                    Modifier.fillMaxWidth().heightIn(min = 48.dp),
                    enabled =
                        !state.busy && !state.pendingReset,
                ) { Text("Sign out") }
                HorizontalDivider(Modifier.padding(vertical = 4.dp))
                Text("Delete local data", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Remove this server's identity, saved messages, and local files from this device.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(
                    { reset = true },
                    Modifier.heightIn(min = 48.dp),
                    enabled =
                        !state.busy && !state.pendingReset,
                    colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                ) { Text("Delete local data") }
            }
        }
    }
    if (signOut) {
        AlertDialog(
            onDismissRequest = { signOut = false },
            title = { Text("Sign out?") },
            text = {
                Text(
                    "This ends the current session and clears its access token. " +
                        "Your identity and saved messages stay on this device.",
                )
            },
            confirmButton = {
                TextButton({
                    signOut = false
                    action(SettingsAction.SignOut)
                }) { Text("Sign out") }
            },
            dismissButton = { TextButton({ signOut = false }) { Text("Cancel") } },
        )
    }
    if (reset) {
        AlertDialog(
            onDismissRequest = { reset = false },
            title = { Text("Delete local data?") },
            text = {
                Text(
                    "This removes this server's identity, saved messages, and local files from this device. " +
                        "It does not delete a remote account or messages on other devices.",
                )
            },
            confirmButton = {
                TextButton(
                    {
                        reset = false
                        action(SettingsAction.DeleteLocalData)
                    },
                    colors =
                        ButtonDefaults.textButtonColors(
                            contentColor = MaterialTheme.colorScheme.error,
                        ),
                ) { Text("Delete") }
            },
            dismissButton = { TextButton({ reset = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun SettingsSection(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            content()
        }
    }
}
