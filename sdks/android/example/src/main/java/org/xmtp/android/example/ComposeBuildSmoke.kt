package org.xmtp.android.example

import androidx.compose.runtime.Composable
import org.xmtp.android.example.shared.BuildSmokeAction
import org.xmtp.android.example.shared.BuildSmokeScreen
import org.xmtp.android.example.shared.BuildSmokeState
import uniffi.xmtp_sdk.SDKClient
import uniffi.xmtp_sdk.inboxId

/** Compile the public SDK to shared UI boundary before the entry point changes. */
@Composable
internal fun ComposeBuildSmoke(
    client: SDKClient,
    onConnect: () -> Unit,
) {
    BuildSmokeScreen(BuildSmokeState(client.inboxId(), true)) { action ->
        when (action) {
            BuildSmokeAction.CONNECT -> onConnect()
        }
    }
}
