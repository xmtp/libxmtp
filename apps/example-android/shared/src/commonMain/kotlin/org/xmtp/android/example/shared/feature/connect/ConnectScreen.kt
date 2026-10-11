package org.xmtp.android.example.shared.feature.connect
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable fun ConnectScreen(
    state: ConnectState,
    action: (ConnectAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val backend = state.backend
    var credential by remember(backend) { mutableStateOf("") }
    val requiresCredential = state.credentialsRequiredFor == backend.trim().trimEnd('/')
    LaunchedEffect(Unit) { action(ConnectAction.InspectCurrent) }

    fun connectCredential() = credential.takeIf { requiresCredential && it.isNotBlank() }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(
                24.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    16.dp,
                ),
    ) {
        state.failure?.message?.takeIf { !state.pendingReset }?.let { message ->
            Notice(message, "Retry") {
                if (state.failure?.code == "restore") {
                    action(ConnectAction.Retry)
                } else {
                    action(ConnectAction.Connect(backend, connectCredential()))
                }
            }
        }

        Spacer(
            Modifier
                .height(
                    24.dp,
                ),
        )
        Row(
            verticalAlignment =
                Alignment.CenterVertically,
            horizontalArrangement =
                Arrangement
                    .spacedBy(
                        12.dp,
                    ),
        ) {
            Image(
                xmtpLogo,
                "XMTP logo",
                Modifier
                    .size(
                        64.dp,
                    ),
            )
            Text(
                "XMTP",
                style =
                    MaterialTheme.typography.displaySmall,
                color = ink,
                fontWeight =
                    FontWeight.Bold,
            )
        }
        Text(
            "Messenger",
            style =
                MaterialTheme.typography.headlineMedium,
        )
        OutlinedTextField(
            backend,
            {
                action(ConnectAction.ChangeBackend(it))
            },
            label = {
                Text("Backend URL")
            },
            modifier =
                Modifier
                    .fillMaxWidth(),
            singleLine = true,
        )
        if (requiresCredential) {
            OutlinedTextField(
                credential,
                {
                    credential = it
                },
                label = {
                    Text("Credential")
                },
                keyboardOptions =
                    androidx.compose.foundation.text.KeyboardOptions(
                        keyboardType = androidx.compose.ui.text.input.KeyboardType.Password,
                    ),
                visualTransformation =
                    androidx.compose.ui.text.input
                        .PasswordVisualTransformation(),
                modifier =
                    Modifier
                        .fillMaxWidth(),
                singleLine = true,
            )
        }
        if (state.migrationRequired) {
            Text(
                "An old account is stored on this device. Its wallet key is unavailable. Reset only the selected local account to continue.",
            )
            state.migrationAccounts.forEach { id ->
                Action("Reset local account ${id.take(12)}") {
                    action(
                        ConnectAction
                            .ResetLegacyAccount(id),
                    )
                }
            }
        } else {
            Action(
                "Connect",
                !state
                    .busy &&
                    backend
                        .isNotBlank(),
            ) {
                action(
                    ConnectAction
                        .Connect(
                            backend,
                            connectCredential(),
                        ),
                )
            }
        }
        Text(
            "This app is for testing and debugging purposes only.",
            style =
                MaterialTheme.typography.bodySmall,
        )
        Spacer(
            Modifier
                .height(
                    24.dp,
                ),
        )
    }
}
