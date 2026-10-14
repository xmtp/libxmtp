package org.xmtp.android.example.shared.feature.group

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
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable
fun ConversationDetailsScreen(
    state: GroupState,
    action: (GroupAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) {
    val settings = state.settings
    var blockConfirmation by remember(state.conversationId) { mutableStateOf(false) }
    LazyColumn(
        Modifier.fillMaxSize().testTag("conversation-settings"),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 32.dp, top = 12.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            Column(
                Modifier.fillMaxWidth().padding(vertical = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                MessengerAvatar(settings.title.ifBlank { "Chat" }, group = settings.group, size = 80.dp)
                Text(
                    settings.title.ifBlank {
                        "Conversation"
                    },
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    textAlign = TextAlign.Center,
                )
                Text(
                    if (settings.group) "${settings.members.size} members" else "Direct conversation",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (settings.description.isNotBlank()) {
                    Text(settings.description, style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center)
                }
            }
        }
        state.failure?.let { failure ->
            item {
                Notice(
                    failure.message,
                    if (failure.recovery ==
                        FailureRecovery.RETRY
                    ) {
                        "Retry"
                    } else {
                        null
                    },
                ) { action(GroupAction.Refresh) }
            }
        }
        if (state.busy) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
        if (state.pendingLeave) {
            item { Notice("Leaving group. Removal is pending until membership confirms the change.") }
        }
        item {
            DetailsSection("Name on this device") {
                Text(
                    "Use a local name to find this conversation. Other people do not see this name.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(state.aliasInput, {
                    action(GroupAction.ChangeAlias(it))
                }, label = {
                    Text(
                        "Local name",
                    )
                }, modifier = Modifier.fillMaxWidth().testTag("conversation-local-alias"))
                TextButton({
                    action(GroupAction.SetAlias(state.aliasInput))
                }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Save local name") }
            }
        }
        item {
            DetailsSection("Notifications") {
                Text(
                    "Get alerts for new messages in this conversation.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(if (settings.notifications) "On" else "Off", fontWeight = FontWeight.SemiBold)
                if (state.notifications.available) {
                    TextButton(
                        onClick = { action(GroupAction.SetNotifications(!settings.notifications)) },
                        enabled = !state.busy,
                        modifier = Modifier.heightIn(min = 48.dp),
                    ) { Text(if (settings.notifications) "Turn notifications off" else "Turn notifications on") }
                } else {
                    Text(
                        state.notifications.reason ?: "Notifications are not available for this conversation.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
        item { DetailsHeading("Members", "${settings.members.size}") }
        items(settings.members, key = { it.inboxId }) { member ->
            MemberCard(member, member.inboxId in state.pendingMemberIds, state.busy || state.pendingLeave, action)
        }
        if (settings.members.isEmpty()) {
            item {
                DetailsSection {
                    Text(
                        "No member details are available.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        if (settings.group) {
            item {
                DetailsSection {
                    if (state.editing == GroupEdit.MEMBERS) {
                        OutlinedTextField(state.memberInput, {
                            action(GroupAction.ChangeMemberInput(it))
                        }, label = {
                            Text(
                                "Handle, Ethereum address, or inbox ID",
                            )
                        }, modifier = Modifier.fillMaxWidth(), minLines = 2)
                        Action(
                            "Add member",
                            state.memberInput.isNotBlank() && !state.busy,
                        ) { action(GroupAction.AddMember(state.memberInput)) }
                        TextButton({ action(GroupAction.CancelEdit) }, Modifier.heightIn(min = 48.dp)) { Text("Done") }
                    } else {
                        TextButton({
                            action(GroupAction.Edit(GroupEdit.MEMBERS))
                        }, Modifier.heightIn(min = 48.dp), enabled = !state.pendingLeave) { Text("Add member") }
                    }
                }
            }
        }
        item {
            DetailsSection("Shared media and files") {
                Text(
                    "Files from the loaded chat. Download a file to verify it before you open or save it.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.sharedMedia.isEmpty()) {
                    Text("No shared files in the loaded messages.", style = MaterialTheme.typography.bodyLarge)
                } else {
                    state.sharedMedia.forEach { message ->
                        HorizontalDivider()
                        Text(
                            message.text.ifBlank { "File" },
                            style = MaterialTheme.typography.bodyLarge,
                            fontWeight = FontWeight.Medium,
                        )
                        Text(
                            "${message.sender} · ${message.day} ${message.time}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        messageExtra(message)
                        TextButton(
                            { state.conversationId?.let { shellAction(ShellAction.Navigate(AppRoute.Chat(it))) } },
                            Modifier.heightIn(min = 48.dp),
                        ) { Text("View in chat") }
                    }
                }
            }
        }
        if (settings.group) {
            item {
                DetailsSection("Conversation settings") {
                    if (state.editing == GroupEdit.DETAILS) {
                        OutlinedTextField(state.name, {
                            action(GroupAction.ChangeName(it))
                        }, label = { Text("Name") }, modifier = Modifier.fillMaxWidth())
                        OutlinedTextField(state.description, {
                            action(GroupAction.ChangeDescription(it))
                        }, label = { Text("Description") }, modifier = Modifier.fillMaxWidth(), minLines = 2)
                        Action("Save", !state.busy) { action(GroupAction.UpdateGroup(state.name, state.description)) }
                        TextButton(
                            { action(GroupAction.CancelEdit) },
                            Modifier.heightIn(min = 48.dp),
                        ) { Text("Cancel") }
                    } else {
                        TextButton(
                            {
                                action(GroupAction.Edit(GroupEdit.DETAILS))
                            },
                            Modifier.heightIn(
                                min = 48.dp,
                            ),
                            enabled = !state.pendingLeave,
                        ) { Text("Edit name and description") }
                    }
                    HorizontalDivider()
                    Text("Who can change group settings", fontWeight = FontWeight.SemiBold)
                    Text(
                        settings.preset.ifBlank { "Permissions are not available." },
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    TextButton(
                        { action(GroupAction.Edit(GroupEdit.PERMISSIONS)) },
                        Modifier.heightIn(min = 48.dp),
                        enabled =
                            !state.busy && !state.pendingLeave,
                    ) { Text("Change permissions") }
                    if (state.editing == GroupEdit.PERMISSIONS) {
                        Action("All members", !state.busy) { action(GroupAction.SetPreset(false)) }
                        Action("Admins only", !state.busy) { action(GroupAction.SetPreset(true)) }
                        TextButton({ action(GroupAction.CancelEdit) }, Modifier.heightIn(min = 48.dp)) { Text("Done") }
                    }
                }
            }
        }
        item {
            DetailsSection("Disappearing messages") {
                Text(
                    if (settings.disappearingSeconds ==
                        "0"
                    ) {
                        "Off"
                    } else {
                        "After ${settings.disappearingSeconds} seconds"
                    },
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    "Set how long messages remain in this conversation.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.editing == GroupEdit.DISAPPEARING) {
                    OutlinedTextField(state.disappearingInput, {
                        action(GroupAction.ChangeDisappearingInput(it))
                    }, label = {
                        Text("Duration in seconds")
                    }, supportingText = { Text("Use 0 to turn this off.") }, modifier = Modifier.fillMaxWidth())
                    Action(
                        "Save duration",
                        !state.busy && state.disappearingInput.toLongOrNull()?.let { it >= 0 } == true,
                    ) {
                        state.disappearingInput.toLongOrNull()?.let { action(GroupAction.SetDisappearing(it)) }
                    }
                    TextButton({ action(GroupAction.CancelEdit) }, Modifier.heightIn(min = 48.dp)) { Text("Cancel") }
                } else {
                    TextButton({
                        action(GroupAction.Edit(GroupEdit.DISAPPEARING))
                    }, Modifier.heightIn(min = 48.dp), enabled = !state.pendingLeave) { Text("Change duration") }
                }
            }
        }
        item {
            DetailsSection("Developer tools") {
                Text(
                    "Raw metadata fields for this conversation.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.metadata.available) {
                    if (settings.group) {
                        TextButton({
                            state.conversationId?.let {
                                shellAction(ShellAction.Navigate(AppRoute.GroupFields(it)))
                            }
                        }, Modifier.heightIn(min = 48.dp)) { Text("Group fields") }
                    }
                    TextButton({
                        state.conversationId?.let {
                            shellAction(ShellAction.Navigate(AppRoute.OwnFields(it)))
                        }
                    }, Modifier.heightIn(min = 48.dp)) { Text("My fields") }
                } else {
                    Text(
                        state.metadata.reason ?: "This server does not support metadata fields.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                if (settings.membership.isNotBlank()) {
                    Text(
                        "Membership: ${settings.membership}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        item {
            DetailsSection("Privacy") {
                Text(
                    "Block this conversation to stop it from appearing in Chats.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(
                    {
                        blockConfirmation = true
                    },
                    Modifier.heightIn(
                        min = 48.dp,
                    ),
                    enabled = !state.busy,
                    colors =
                        ButtonDefaults.textButtonColors(
                            contentColor = MaterialTheme.colorScheme.error,
                        ),
                ) {
                    Text("Block conversation")
                }
                if (settings.group) {
                    HorizontalDivider()
                    Text("Leave group", fontWeight = FontWeight.SemiBold)
                    Text(
                        "Request removal from this group. Your membership can remain pending until it is confirmed.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    TextButton(
                        { action(GroupAction.Edit(GroupEdit.LEAVE)) },
                        Modifier.heightIn(min = 48.dp),
                        enabled =
                            settings.canRequestRemoval && state.leave.available && !state.pendingLeave && !state.busy,
                        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                    ) { Text(if (state.pendingLeave) "Leaving group…" else "Leave group") }
                    if (!settings.canRequestRemoval ||
                        !state.leave.available
                    ) {
                        Text(
                            state.leave.reason ?: "Removal is not available for your current membership.",
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
            }
        }
    }
    if (state.editing == GroupEdit.LEAVE && !state.pendingLeave) {
        AlertDialog(
            onDismissRequest = { action(GroupAction.CancelEdit) },
            title = { Text("Leave group?") },
            text = {
                Text(
                    "This requests your removal from ${settings.title.ifBlank {
                        "this group"
                    }}. You leave when the membership change is confirmed.",
                )
            },
            confirmButton = { TextButton({ action(GroupAction.RequestRemoval) }) { Text("Leave group") } },
            dismissButton = { TextButton({ action(GroupAction.CancelEdit) }) { Text("Cancel") } },
        )
    }
    if (blockConfirmation) {
        AlertDialog(
            onDismissRequest = { blockConfirmation = false },
            title = { Text("Block conversation?") },
            text = {
                Text(
                    "This changes consent to Blocked and removes this conversation from Chats. " +
                        "It does not delete its messages.",
                )
            },
            confirmButton = {
                TextButton({
                    blockConfirmation = false
                    action(GroupAction.Block)
                }) { Text("Block") }
            },
            dismissButton = { TextButton({ blockConfirmation = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun MemberCard(
    member: MemberRow,
    pending: Boolean,
    busy: Boolean,
    action: (GroupAction) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    DetailsSection(modifier = Modifier.testTag("settings-member-${member.inboxId}")) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            MessengerAvatar(member.displayName ?: member.inboxId)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(member.displayName ?: "Chat member", style = MaterialTheme.typography.titleMedium)
                Text(
                    member.role,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        SelectionContainer {
            Text(
                member.inboxId,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TextButton({
            clipboard.setText(AnnotatedString(member.inboxId))
        }, Modifier.heightIn(min = 48.dp)) { Text("Copy inbox ID") }
        if (pending) {
            Text(
                "Membership change pending",
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.bodyMedium,
            )
            LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        if (member.canManage) {
            if (!member.role.equals("Super admin", ignoreCase = true)) {
                val admin = member.role.equals("Admin", ignoreCase = true)
                TextButton(
                    { action(GroupAction.SetAdmin(member.inboxId, !admin)) },
                    Modifier.heightIn(min = 48.dp),
                    enabled =
                        !pending && !busy,
                ) { Text(if (admin) "Remove admin" else "Make admin") }
            }
            TextButton(
                { action(GroupAction.RemoveMember(member.inboxId)) },
                Modifier.heightIn(min = 48.dp),
                enabled =
                    !pending && !busy,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) { Text("Remove") }
        }
    }
}

@Composable
private fun DetailsHeading(
    title: String,
    detail: String? = null,
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        detail?.let {
            Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun DetailsSection(
    title: String? = null,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            title?.let { Text(it, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold) }
            content()
        }
    }
}
