package org.xmtp.android.example.shared

import androidx.compose.runtime.Composable
import uniffi.xmtp_sdk.SDKClient
import uniffi.xmtp_sdk.inboxId

/** Compile the public SDK to shared UI boundary in the Android test target. */
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
