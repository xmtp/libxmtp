package org.xmtp.android.example.shared.feature.settings

import org.xmtp.android.example.shared.*

enum class NotificationPermission { UNKNOWN, GRANTED, DENIED }

data class SettingsState(
    val inbox: String = "",
    /** The inbox's current handle, shown as the profile title. */
    val handle: String? = null,
    /** The inbox's other handles, the most recently claimed first. */
    val otherHandles: List<String> = emptyList(),
    val backend: String = "",
    val serverLabel: String = "",
    val notifications: CapabilityUi = CapabilityUi(),
    val attachments: CapabilityUi = CapabilityUi(),
    val notificationStatus: String = "Off: Firebase is not configured",
    val notificationsEnabled: Boolean = false,
    val notificationPermission: NotificationPermission = NotificationPermission.UNKNOWN,
    val appearance: Appearance = Appearance.SYSTEM,
    val unknownSends: List<UnknownSendRow> = emptyList(),
    val pendingReset: Boolean = false,
    val busy: Boolean = false,
    val failure: AppFailure? = null,
)

sealed interface SettingsAction {
    data class SetNotifications(
        val enabled: Boolean,
    ) : SettingsAction

    data class SetAppearance(
        val value: Appearance,
    ) : SettingsAction

    data class DiscardUnknownSend(
        val draftId: String,
    ) : SettingsAction

    data object SignOut : SettingsAction

    data object DeleteLocalData : SettingsAction

    data object ResumeReset : SettingsAction
}
