package org.xmtp.android.example.shared.feature.connect

import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.FailureRecovery

object ConnectFixtures {
    val empty = ConnectState()
    val populated =
        ConnectState(
            backend = "https://messenger.example.test",
            label = "Team server",
            normalizedBackend = "https://messenger.example.test",
            probe = ProbeState.READY,
        )
    val loading = populated.copy(probe = ProbeState.CHECKING, busy = true)
    val tokenRequired = populated.copy(credentialsRequiredFor = populated.backend)
    val failure =
        populated.copy(
            probe = ProbeState.FAILED,
            hasAttemptedJoin = true,
            failure =
                AppFailure(
                    "Cannot connect to this server. Try again.",
                    "connect-unavailable",
                    FailureRecovery.RETRY,
                ),
        )
    val resetFailure = failure.copy(pendingReset = true)
    val migration = populated.copy(migrationRequired = true, migrationAccounts = listOf("sample-old-account"))
}
