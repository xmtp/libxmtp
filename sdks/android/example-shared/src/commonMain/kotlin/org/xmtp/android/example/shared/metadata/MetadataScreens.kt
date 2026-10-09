package org.xmtp.android.example.shared.metadata

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable
fun MetadataScreen(state: MetadataEditorState, own: Boolean, action: (MetadataEdit) -> Unit) {
    val fields = state.fields.filter { it.userField == own }
    val ownValues = remember(fields) { mutableStateMapOf<FieldUiId, String?>().apply { fields.filter { it.editable }.forEach { put(it.id, if (it.present) it.scalar else null) } } }
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Text(if (own) "These values apply to this conversation. Only your entry can change." else "Fields come from this conversation's committed registry.")
            if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            TextButton({ action(MetadataEdit.Refresh) }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Reload fields") }
        }
        if (fields.isEmpty()) item { Text("No registered fields.") }
        items(fields, key = { it.id.componentId.toInt() }) { field ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(field.label, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Text("Component ${field.id.componentId} · ${field.shape}", style = MaterialTheme.typography.bodySmall)
                Text(field.policy, style = MaterialTheme.typography.bodySmall)
                if (!field.editable) Text("Unsupported${field.unsupportedTag?.let { " type $it" } ?: " field or policy"}")
                else if (own) {
                    OutlinedTextField(ownValues[field.id] ?: "", { ownValues[field.id] = it }, label = { Text(if (field.shape == FieldShape.USER_BYTES) "Value (hex)" else "Value") }, modifier = Modifier.fillMaxWidth(), enabled = !state.busy)
                    Row {
                        TextButton({ ownValues[field.id] = "" }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Set empty") }
                        TextButton({ ownValues[field.id] = null }, Modifier.heightIn(min = 48.dp), enabled = !state.busy) { Text("Clear") }
                        Text(if (ownValues[field.id] == null) "Absent" else "Set", Modifier.padding(12.dp))
                    }
                } else GroupField(field, state.busy, action)
                HorizontalDivider()
            }
        }
        if (own) {
            item { Button({ action(MetadataEdit.Own(ownValues.toMap())) }, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = !state.busy && fields.any { it.editable }) { Text("Save changed fields") } }
            items(state.members, key = { it.inboxId }) { member ->
                Column { Text("Member ${member.inboxId}", fontWeight = FontWeight.Bold); member.values.forEach { Text("${it.label}: ${if (it.present) it.scalar else "Absent"}") } }
            }
        }
        if (state.missing.isNotEmpty()) item {
            Text("Offered fields are not registered here: ${state.missing.joinToString()}. Create a new conversation to use this catalogue.")
        }
    }
}

@Composable
private fun GroupField(field: FieldUi, busy: Boolean, action: (MetadataEdit) -> Unit) {
    var text by remember(field.id, field.scalar, field.present) { mutableStateOf(field.scalar) }
    var key by remember(field.id) { mutableStateOf("") }
    when (field.shape) {
        FieldShape.STRING, FieldShape.BYTES -> {
            Text(if (field.present) "Current value: ${field.scalar}" else "Absent")
            OutlinedTextField(text, { text = it }, label = { Text(if (field.shape == FieldShape.BYTES) "Value (hex)" else "Value") }, modifier = Modifier.fillMaxWidth(), enabled = !busy)
            Row {
                TextButton({ action(MetadataEdit.Scalar(field.id, text)) }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Set") }
                TextButton({ action(MetadataEdit.Scalar(field.id, null)) }, Modifier.heightIn(min = 48.dp), enabled = !busy && field.present) { Text("Clear") }
            }
        }
        FieldShape.BYTE_MAP, FieldShape.BYTE_SET, FieldShape.INBOX_SET -> {
            field.entries.forEach { entry ->
                Row(Modifier.fillMaxWidth()) {
                    Text("${entry.key}${if (field.shape == FieldShape.BYTE_MAP) ": ${entry.value}" else ""}", Modifier.weight(1f))
                    if (field.shape == FieldShape.BYTE_MAP) TextButton({ key = entry.key; text = entry.value }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Edit") }
                    TextButton({ action(MetadataEdit.Entry(field.id, EntryAction.DELETE, entry.key)) }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Delete") }
                }
            }
            OutlinedTextField(key, { key = it }, label = { Text(if (field.shape == FieldShape.INBOX_SET) "Inbox ID" else "Key (hex)") }, modifier = Modifier.fillMaxWidth(), enabled = !busy)
            if (field.shape == FieldShape.BYTE_MAP) OutlinedTextField(text, { text = it }, label = { Text("Value (hex)") }, modifier = Modifier.fillMaxWidth(), enabled = !busy)
            Row {
                TextButton({ action(MetadataEdit.Entry(field.id, EntryAction.INSERT, key, text)) }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Add") }
                if (field.shape == FieldShape.BYTE_MAP) TextButton({ action(MetadataEdit.Entry(field.id, EntryAction.UPDATE, key, text)) }, Modifier.heightIn(min = 48.dp), enabled = !busy) { Text("Update entry") }
            }
        }
        else -> Text("Unsupported")
    }
}
