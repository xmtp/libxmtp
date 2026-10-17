package org.xmtp.android.example.shared.feature.connect

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable
fun ConnectScreen(
    state: ConnectState,
    action: (ConnectAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val backend = state.backend
    var credential by remember(backend) { mutableStateOf("") }
    val requiresCredential =
        state.credentialsRequiredFor != null &&
            state.credentialsRequiredFor == backend.trim().trimEnd('/')
    LaunchedEffect(Unit) {
        action(ConnectAction.InspectCurrent)
    }

    fun join() {
        action(ConnectAction.Connect(backend, credential.takeIf { requiresCredential && it.isNotBlank() }))
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 32.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        XmtpBrand()
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("A place to connect.", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
            Text(
                "Join a server to start a conversation.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text("Your server", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                OutlinedTextField(
                    value = backend,
                    onValueChange = { action(ConnectAction.ChangeBackend(it)) },
                    label = { Text(if (state.advanced) "Raw server address" else "Server address") },
                    placeholder = { Text("https://your-server.example") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !state.busy && !state.pendingReset,
                    singleLine = true,
                    shape = RoundedCornerShape(16.dp),
                )
                OutlinedTextField(
                    value = state.label,
                    onValueChange = { action(ConnectAction.ChangeLabel(it)) },
                    label = { Text("Server name (optional)") },
                    placeholder = { Text("A name you will recognize") },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !state.busy && !state.pendingReset,
                    singleLine = true,
                    shape = RoundedCornerShape(16.dp),
                )
                when (state.probe) {
                    ProbeState.CHECKING -> {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                            Text("Checking server…", style = MaterialTheme.typography.bodyMedium)
                        }
                    }

                    ProbeState.READY -> {
                        Text(
                            "Server ready",
                            color = MaterialTheme.colorScheme.primary,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }

                    ProbeState.FAILED -> {
                        if (state.hasAttemptedJoin) {
                            Text(
                                "Server check failed. Check the address and try again.",
                                color = MaterialTheme.colorScheme.error,
                                style = MaterialTheme.typography.bodyMedium,
                            )
                        }
                    }

                    ProbeState.IDLE -> {
                        Unit
                    }
                }
                if (requiresCredential) {
                    OutlinedTextField(
                        value = credential,
                        onValueChange = { credential = it },
                        label = { Text("Access token") },
                        supportingText = { Text("Ask the server owner for an access token.") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                        visualTransformation = PasswordVisualTransformation(),
                        modifier = Modifier.fillMaxWidth(),
                        enabled = !state.busy,
                        singleLine = true,
                        shape = RoundedCornerShape(16.dp),
                    )
                }
                TextButton(
                    onClick = { action(ConnectAction.ToggleAdvanced) },
                    modifier = Modifier.heightIn(min = 48.dp),
                ) { Text(if (state.advanced) "Hide advanced settings" else "Advanced settings") }
                if (state.advanced) {
                    Text(
                        "Use the full server address. Keep access tokens in the separate token field.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    state.normalizedBackend?.let {
                        Text("Connect to: $it", style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
        }
        state.failure
            ?.takeIf {
                !state.pendingReset && (it.code != "server-probe" || state.hasAttemptedJoin)
            }?.let { failure ->
                Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(16.dp)) {
                    Column(Modifier.fillMaxWidth().padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite }) {
                        Text(failure.message, color = MaterialTheme.colorScheme.onErrorContainer)
                        TextButton(
                            onClick = {
                                if (failure.code == "restore") action(ConnectAction.Retry) else join()
                            },
                            enabled = !state.busy,
                            modifier = Modifier.heightIn(min = 48.dp),
                        ) { Text("Retry") }
                    }
                }
            }
        if (state.pendingReset) {
            Text(
                "Local data removal is not complete. Continue removal to connect again.",
                style = MaterialTheme.typography.bodyLarge,
            )
            state.failure?.let { Text(it.message, color = MaterialTheme.colorScheme.error) }
            Button(onClick = {
                action(ConnectAction.ResumeReset)
            }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                Text(if (state.busy) "Removing local data…" else "Continue local data removal")
            }
        } else if (state.migrationRequired) {
            Text(
                "An old account is stored on this device. Its wallet key is unavailable. " +
                    "Remove the selected local account to continue.",
            )
            state.migrationAccounts.forEach { id ->
                OutlinedButton(onClick = {
                    action(ConnectAction.ResetLegacyAccount(id))
                }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Text("Remove local account ${id.take(12)}")
                }
            }
        } else {
            Button(
                onClick = { join() },
                enabled =
                    !state.busy && state.probe != ProbeState.CHECKING && backend.isNotBlank() &&
                        (!requiresCredential || credential.isNotBlank()),
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
                    if (state.busy) "Joining server…" else "Join a server",
                    style = MaterialTheme.typography.titleMedium,
                )
            }
        }
        Text(
            "Use a server address from someone you trust.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
