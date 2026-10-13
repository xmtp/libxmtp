package org.xmtp.android.example.shared.feature.conversations

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.MessengerAvatar

@Composable
fun CreateConversationScreen(
    state: CreateConversationState,
    action: (CreateConversationAction) -> Unit,
) {
    val review = state.step == CreateStep.REVIEW
    val availableRecipients =
        state.knownRecipients.filter { person ->
            state.selectedRecipients.none {
                it.inboxId ==
                    person.inboxId
            }
        }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                if (review) "2 of 2 · Review" else "1 of 2 · Choose people",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                if (review) {
                    if (state.group) "Make it your group" else "Ready to say hello?"
                } else {
                    "Who is this for?"
                },
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        state.failure?.let {
            ConversationFailure(it, state.busy) {
                action(if (review) CreateConversationAction.Create else CreateConversationAction.ResolveRecipients)
            }
        }
        if (!review) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ModeChoice("Direct message", "A conversation with one person", !state.group, !state.busy) {
                    action(CreateConversationAction.ChangeMode(false))
                }
                ModeChoice("Group", "Bring people together", state.group, !state.busy) {
                    action(CreateConversationAction.ChangeMode(true))
                }
            }
            Surface(color = MaterialTheme.colorScheme.surface, shape = RoundedCornerShape(20.dp)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Add a person", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    OutlinedTextField(
                        value = state.recipients,
                        onValueChange = { action(CreateConversationAction.ChangeRecipients(it)) },
                        label = { Text("Handle, Ethereum address, or inbox ID") },
                        supportingText = {
                            Text(
                                if (state.group) {
                                    "Separate multiple people with commas."
                                } else {
                                    "Enter one handle, Ethereum address, or inbox ID."
                                },
                            )
                        },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = !state.busy,
                        minLines = 2,
                        shape = RoundedCornerShape(16.dp),
                    )
                    OutlinedButton(
                        onClick = { action(CreateConversationAction.ResolveRecipients) },
                        enabled =
                            !state.busy && state.recipients.isNotBlank(),
                        modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
                    ) { Text(if (state.busy) "Checking people…" else "Check and add") }
                }
            }
            if (availableRecipients.isNotEmpty()) {
                Text(
                    "People from your chats",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    "Saved on this device",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                availableRecipients.forEach { person ->
                    val selected = state.selectedRecipients.any { it.inboxId == person.inboxId } || person.selected
                    RecipientItem(person, selected, !state.busy, onClick = {
                        action(CreateConversationAction.ToggleRecipient(person.inboxId))
                    })
                }
            }
            if (state.selectedRecipients.isNotEmpty()) {
                Text(
                    "Selected · ${state.selectedRecipients.size}",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                state.selectedRecipients.forEach { person ->
                    RecipientItem(
                        person,
                        true,
                        !state.busy,
                        onClick = { action(CreateConversationAction.ToggleRecipient(person.inboxId)) },
                    )
                }
            }
            state.maxMembers
                ?.takeIf {
                    state.group
                }?.let {
                    Text(
                        "Up to $it members, including you.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            Button(
                onClick = { action(CreateConversationAction.Next) },
                enabled =
                    !state.busy && state.selectedRecipients.isNotEmpty() &&
                        state.selectedRecipients.all { it.failure == null } &&
                        (state.group || state.selectedRecipients.size == 1) &&
                        (
                            state.maxMembers == null || !state.group ||
                                state.selectedRecipients.size + 1 <= state.maxMembers
                        ),
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp),
                shape = RoundedCornerShape(18.dp),
            ) { Text("Continue") }
        } else {
            if (state.group) {
                OutlinedTextField(state.name, {
                    action(CreateConversationAction.ChangeName(it))
                }, label = {
                    Text("Group name")
                }, modifier = Modifier.fillMaxWidth(), enabled = !state.busy, shape = RoundedCornerShape(16.dp))
                OutlinedTextField(
                    state.description,
                    {
                        action(CreateConversationAction.ChangeDescription(it))
                    },
                    label = {
                        Text("Description (optional)")
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !state.busy,
                    minLines = 2,
                    shape =
                        RoundedCornerShape(
                            16.dp,
                        ),
                )
            }
            Surface(color = MaterialTheme.colorScheme.surface, shape = RoundedCornerShape(20.dp)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(
                        if (state.group) "Members · ${state.selectedRecipients.size + 1}" else "Send a message to",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (state.group) Text("You", style = MaterialTheme.typography.bodyLarge)
                    state.selectedRecipients.forEach { person ->
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            MessengerAvatar(person.title.ifBlank { "Person" }, size = 40.dp)
                            Text(
                                person.title.ifBlank { person.inboxId.take(12) },
                                Modifier.weight(1f),
                                style = MaterialTheme.typography.bodyLarge,
                            )
                        }
                    }
                    TextButton(onClick = {
                        action(CreateConversationAction.Back)
                    }, enabled = !state.busy, modifier = Modifier.heightIn(min = 48.dp)) { Text("Change people") }
                }
            }
            if (state.group) {
                Text(
                    "Group permissions",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                ModeChoice(
                    "All members",
                    "Members can update group details and add people.",
                    !state.adminOnly,
                    !state.busy,
                ) {
                    action(CreateConversationAction.ChangePreset(false))
                }
                ModeChoice(
                    "Admins only",
                    "Only admins can update group details and add people.",
                    state.adminOnly,
                    !state.busy,
                ) {
                    action(CreateConversationAction.ChangePreset(true))
                }
            }
            Button(
                onClick = { action(CreateConversationAction.Create) },
                enabled =
                    !state.busy && state.selectedRecipients.isNotEmpty(),
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp),
                shape = RoundedCornerShape(18.dp),
            ) {
                if (state.busy) {
                    CircularProgressIndicator(
                        Modifier.size(20.dp),
                        color = MaterialTheme.colorScheme.onPrimary,
                        strokeWidth = 2.dp,
                    )
                    Spacer(Modifier.width(12.dp))
                }
                Text(
                    if (state.busy) {
                        "Creating conversation…"
                    } else if (state.group) {
                        "Create group"
                    } else {
                        "Start conversation"
                    },
                )
            }
            TextButton(onClick = {
                action(CreateConversationAction.Back)
            }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Back") }
        }
    }
}

@Composable
private fun ModeChoice(
    title: String,
    detail: String,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Surface(
        color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(18.dp),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .selectable(
                    selected = selected,
                    enabled = enabled,
                    role = Role.RadioButton,
                    onClick = onClick,
                ).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            RadioButton(selected = selected, onClick = null, enabled = enabled)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(
                    detail,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun RecipientItem(
    person: RecipientUi,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Surface(
        color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .toggleable(value = selected, enabled = enabled, role = Role.Checkbox, onValueChange = {
                    onClick()
                })
                .padding(
                    12.dp,
                ),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MessengerAvatar(person.title.ifBlank { "Person" }, size = 40.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(person.title.ifBlank { person.inboxId.take(12) }, style = MaterialTheme.typography.titleSmall)
                person.failure?.let {
                    Text(
                        it.message,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }
            Checkbox(checked = selected, onCheckedChange = null, enabled = enabled)
        }
    }
}
