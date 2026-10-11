package org.xmtp.android.example.shared.feature.conversations
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable fun CreateConversationScreen(
    state: CreateConversationState,
    action: (CreateConversationAction) -> Unit,
) {
    val group = state.group
    val recipients = state.recipients
    val name = state.name
    val description = state.description
    val admin = state.adminOnly
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(
                20.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    12.dp,
                ),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(
                onClick = { action(CreateConversationAction.ChangeMode(false)) },
                modifier = Modifier.weight(1f).heightIn(min = 48.dp),
                colors =
                    ButtonDefaults.textButtonColors(
                        containerColor = if (!group) outgoing else Color(0xFFF1F3F7),
                        contentColor = if (!group) blue else ink,
                    ),
            ) { Text("Direct message") }
            TextButton(
                onClick = { action(CreateConversationAction.ChangeMode(true)) },
                modifier = Modifier.weight(1f).heightIn(min = 48.dp),
                colors =
                    ButtonDefaults.textButtonColors(
                        containerColor = if (group) outgoing else Color(0xFFF1F3F7),
                        contentColor = if (group) blue else ink,
                    ),
            ) { Text("Group") }
        }
        Text(
            if (group) "Group" else "Direct message",
            fontWeight =
                FontWeight.Bold,
        )
        OutlinedTextField(
            recipients,
            {
                action(CreateConversationAction.ChangeRecipients(it))
            },
            label = {
                Text("Inbox IDs or Ethereum addresses, separated by commas")
            },
            modifier =
                Modifier
                    .fillMaxWidth(),
        )
        if (group) {
            OutlinedTextField(
                name,
                {
                    action(CreateConversationAction.ChangeName(it))
                },
                label = {
                    Text("Name")
                },
                modifier =
                    Modifier
                        .fillMaxWidth(),
            )
            OutlinedTextField(
                description,
                {
                    action(CreateConversationAction.ChangeDescription(it))
                },
                label = {
                    Text("Description")
                },
                modifier =
                    Modifier
                        .fillMaxWidth(),
            )
            Row(
                verticalAlignment =
                    Alignment.CenterVertically,
            ) {
                Checkbox(
                    admin,
                    {
                        action(CreateConversationAction.ChangePreset(it))
                    },
                )
                Text("Admins only")
            }
        }
        Action(
            "Create",
            recipients
                .isNotBlank(),
        ) {
            action(
                CreateConversationAction.Create,
            )
        }
    }
}
