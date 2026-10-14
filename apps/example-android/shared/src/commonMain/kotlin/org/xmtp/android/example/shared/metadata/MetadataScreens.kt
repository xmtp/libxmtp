package org.xmtp.android.example.shared.metadata

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MetadataScreen(
    state: MetadataEditorState,
    own: Boolean,
    action: (MetadataEdit) -> Unit,
) {
    val fields = state.fields.filter { it.userField == own }
    var draft by remember(own) { mutableStateOf(OwnFieldDraft()) }
    val currentDraft = draft.merge(fields)
    SideEffect { draft = currentDraft }
    val ownValues = currentDraft.values
    LazyColumn(
        Modifier.fillMaxSize().padding(horizontal = 20.dp).testTag("metadata-fields"),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            Text(
                if (own) {
                    "These values apply to this conversation. Only your entry can change."
                } else {
                    "Fields come from this conversation's committed registry."
                },
            )
            if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            TextButton({
                action(MetadataEdit.Refresh)
            }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Reload fields") }
        }
        if (fields.isEmpty()) item { Text("No registered fields.") }
        items(fields, key = { it.id.componentId.toInt() }) { field ->
            Column(
                Modifier.testTag("metadata-field-${field.id.componentId}"),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(field.label, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Text("Component ${field.id.componentId} · ${field.shape}", style = MaterialTheme.typography.bodySmall)
                if (field.shape == FieldShape.BYTES ||
                    field.shape == FieldShape.USER_BYTES
                ) {
                    Text("${field.scalar.length / 2} bytes")
                }
                Text(field.policy, style = MaterialTheme.typography.bodySmall)
                if (!field.editable) {
                    Text("Unsupported${field.unsupportedTag?.let { " type $it" } ?: " field or policy"}")
                } else if (!field.canWrite) {
                    Text("Immutable field: this component is already set.")
                    Text(if (field.present) field.scalar else "Your value is absent.")
                    field.entries.forEach { Text("${it.key}: ${it.value}") }
                } else if (own) {
                    OutlinedTextField(
                        ownValues[field.id] ?: "",
                        { draft = currentDraft.change(field.id, it) },
                        label = {
                            Text(
                                if (field.shape ==
                                    FieldShape.USER_BYTES
                                ) {
                                    "Value (hex)"
                                } else {
                                    "Value"
                                },
                            )
                        },
                        modifier =
                            Modifier.fillMaxWidth().testTag(
                                "metadata-value-${field.id.componentId}",
                            ),
                        enabled = !state.busy,
                    )
                    Text(if (ownValues[field.id] == null) "Absent" else "Set", Modifier.fillMaxWidth())
                    FlowRow(Modifier.fillMaxWidth()) {
                        TextButton(
                            { draft = currentDraft.change(field.id, "") },
                            Modifier.heightIn(min = 48.dp),
                            enabled = !state.busy,
                        ) { Text("Set empty") }
                        TextButton(
                            { draft = currentDraft.change(field.id, null) },
                            Modifier.heightIn(min = 48.dp),
                            enabled = !state.busy && !field.immutable,
                        ) { Text("Clear") }
                    }
                } else {
                    GroupField(field, state.busy, action)
                }
                HorizontalDivider()
            }
        }
        if (own) {
            item {
                Button(
                    { action(currentDraft.edit()) },
                    Modifier.fillMaxWidth().heightIn(min = 48.dp),
                    enabled =
                        !state.busy && fields.any { it.canWrite },
                ) { Text("Save changed fields") }
            }
            items(state.members, key = { it.inboxId }) { member ->
                Column {
                    Text("Member ${member.inboxId}", fontWeight = FontWeight.Bold)
                    member.values.forEach { Text("${it.label}: ${if (it.present) it.scalar else "Absent"}") }
                }
            }
        }
        if (state.missing.isNotEmpty()) {
            item {
                Text(
                    "Offered fields are not registered here: ${state.missing.joinToString()}. " +
                        "Create a new conversation to use this catalogue.",
                )
            }
        }
    }
}

@Composable
private fun GroupField(
    field: FieldUi,
    busy: Boolean,
    action: (MetadataEdit) -> Unit,
) {
    var text by remember(field.id, field.scalar, field.present) { mutableStateOf(field.scalar) }
    var key by remember(field.id) { mutableStateOf("") }
    when (field.shape) {
        FieldShape.STRING, FieldShape.BYTES -> {
            Text(if (field.present) "Current value: ${field.scalar}" else "Absent")
            OutlinedTextField(
                text,
                { text = it },
                label = {
                    Text(
                        if (field.shape ==
                            FieldShape.BYTES
                        ) {
                            "Value (hex)"
                        } else {
                            "Value"
                        },
                    )
                },
                modifier =
                    Modifier.fillMaxWidth().testTag(
                        "metadata-value-${field.id.componentId}",
                    ),
                enabled = !busy,
            )
            Row {
                TextButton(
                    {
                        action(MetadataEdit.Scalar(field.id, text))
                    },
                    Modifier
                        .heightIn(
                            min = 48.dp,
                        ).testTag("metadata-set-${field.id.componentId}"),
                    enabled = !busy,
                ) { Text("Set") }
                TextButton(
                    { action(MetadataEdit.Scalar(field.id, null)) },
                    Modifier.heightIn(min = 48.dp),
                    enabled =
                        !busy && field.present && !field.immutable,
                ) { Text("Clear") }
            }
        }

        FieldShape.BYTE_MAP, FieldShape.BYTE_SET, FieldShape.INBOX_SET -> {
            field.entries.forEach { entry ->
                Row(Modifier.fillMaxWidth()) {
                    Text(
                        "${entry.key}${if (field.shape == FieldShape.BYTE_MAP) ": ${entry.value}" else ""}",
                        Modifier.weight(1f),
                    )
                    if (field.shape ==
                        FieldShape.BYTE_MAP
                    ) {
                        TextButton({
                            key = entry.key
                            text = entry.value
                        }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Edit") }
                    }
                    TextButton(
                        {
                            action(MetadataEdit.Entry(field.id, EntryAction.DELETE, entry.key))
                        },
                        Modifier
                            .heightIn(min = 48.dp)
                            .testTag("metadata-delete-${field.id.componentId}-${entry.key}"),
                        enabled = !busy,
                    ) { Text("Delete") }
                }
            }
            if (field.immutable) Text("Set once. This first entry will lock the component.")
            OutlinedTextField(key, { key = it }, label = {
                Text(
                    if (field.shape ==
                        FieldShape.INBOX_SET
                    ) {
                        "Inbox ID"
                    } else {
                        "Key (hex)"
                    },
                )
            }, modifier = Modifier.fillMaxWidth().testTag("metadata-key-${field.id.componentId}"), enabled = !busy)
            if (field.shape ==
                FieldShape.BYTE_MAP
            ) {
                OutlinedTextField(
                    text,
                    {
                        text = it
                    },
                    label = { Text("Value (hex)") },
                    modifier = Modifier.fillMaxWidth().testTag("metadata-entry-value-${field.id.componentId}"),
                    enabled = !busy,
                )
            }
            Row {
                TextButton(
                    {
                        action(MetadataEdit.Entry(field.id, EntryAction.INSERT, key, text))
                    },
                    Modifier.heightIn(min = 48.dp).testTag("metadata-add-${field.id.componentId}"),
                    enabled = !busy,
                ) { Text("Add") }
                if (field.shape ==
                    FieldShape.BYTE_MAP
                ) {
                    TextButton(
                        {
                            action(MetadataEdit.Entry(field.id, EntryAction.UPDATE, key, text))
                        },
                        Modifier.heightIn(min = 48.dp).testTag("metadata-update-${field.id.componentId}"),
                        enabled = !busy && !field.immutable,
                    ) { Text("Update entry") }
                }
            }
        }

        else -> {
            Text("Unsupported")
        }
    }
}
