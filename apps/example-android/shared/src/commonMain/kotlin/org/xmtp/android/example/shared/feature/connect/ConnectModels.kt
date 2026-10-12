package org.xmtp.android.example.shared.feature.connect

import org.xmtp.android.example.shared.*

enum class ProbeState { IDLE, CHECKING, READY, FAILED }

data class ConnectState(
    val backend: String = "",
    val label: String = "",
    val normalizedBackend: String? = null,
    val probe: ProbeState = ProbeState.IDLE,
    val credentialsRequiredFor: String? = null,
    val busy: Boolean = false,
    val failure: AppFailure? = null,
    val advanced: Boolean = false,
    val pendingReset: Boolean = false,
    val migrationRequired: Boolean = false,
    val migrationAccounts: List<String> = emptyList(),
)

sealed interface ConnectAction {
    data class ChangeBackend(
        val value: String,
    ) : ConnectAction

    data class ChangeLabel(
        val value: String,
    ) : ConnectAction

    data object InspectCurrent : ConnectAction

    data class Inspect(
        val backend: String,
    ) : ConnectAction

    data class Connect(
        val backend: String,
        val credential: String?,
    ) : ConnectAction

    data object Retry : ConnectAction

    data object ToggleAdvanced : ConnectAction

    data object ResumeReset : ConnectAction

    data class ResetLegacyAccount(
        val inboxId: String,
    ) : ConnectAction
}
