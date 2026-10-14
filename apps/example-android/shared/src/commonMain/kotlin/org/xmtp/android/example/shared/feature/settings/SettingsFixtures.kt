package org.xmtp.android.example.shared.feature.settings

import org.xmtp.android.example.shared.*

object SettingsFixtures {
    val empty = SettingsState()
    val populated =
        SettingsState(
            inbox = "sample-own-inbox",
            handle = "nick#2341",
            otherHandles = listOf("nick#0912", "nickm"),
            backend = "https://messenger.example.test",
            serverLabel = "Team server",
            notifications = CapabilityUi(true),
            attachments = CapabilityUi(true),
            notificationStatus = "Off",
            notificationPermission = NotificationPermission.GRANTED,
        )
    val loading = populated.copy(busy = true)
    val failure =
        populated.copy(
            failure = AppFailure("Cannot save this setting. Try again.", "settings-unavailable", FailureRecovery.RETRY),
        )
    val unconfigured =
        populated.copy(
            notifications = CapabilityUi(false, "Notifications are not configured for this app."),
            notificationStatus = "Off: Firebase is not configured",
        )
    val permissionDenied =
        populated.copy(
            notificationPermission = NotificationPermission.DENIED,
            notificationStatus = "Permission is off",
        )
    val resetFailure = failure.copy(pendingReset = true)
    val recovery = populated.copy(unknownSends = listOf(UnknownSendRow("draft-1", "conversation-alex")))
}
