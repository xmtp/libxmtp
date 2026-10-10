package org.xmtp.android.example.shared

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable

/** Small build fixture. Android owns the state and handles the action. */
@Immutable
data class BuildSmokeState(
    val title: String,
    val canConnect: Boolean,
)

enum class BuildSmokeAction { CONNECT }

/** Compile a stateless commonMain screen before the app entry point changes. */
@Composable
fun BuildSmokeScreen(
    state: BuildSmokeState,
    onAction: (BuildSmokeAction) -> Unit,
) {
    MaterialTheme {
        Column {
            Text(state.title)
            Button(enabled = state.canConnect, onClick = { onAction(BuildSmokeAction.CONNECT) }) {
                Text("Connect")
            }
        }
    }
}
