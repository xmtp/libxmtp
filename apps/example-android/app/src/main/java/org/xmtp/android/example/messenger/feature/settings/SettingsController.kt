package org.xmtp.android.example.messenger.feature.settings

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.SecureComposerDrafts
import org.xmtp.android.example.messenger.feature.connect.ConnectController
import org.xmtp.android.example.messenger.feature.featureFailure
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.messenger.session.SessionCommands
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.settings.*
import uniffi.xmtp_sdk.inboxId
import java.util.concurrent.atomic.AtomicLong

internal class SettingsController(
    private val session: AppSession,
    private val commands: SessionCommands,
    private val notifications: NotificationController,
    private val sends: SendCoordinator,
    private val preferences: AppFeaturePreferences,
    private val connect: ConnectController,
    private val destinations: DestinationScope,
    private val scope: CoroutineScope,
    private val composerDrafts: SecureComposerDrafts =
        SecureComposerDrafts(
            session,
            session.secrets,
        ),
) {
    private val ui = MutableStateFlow(SettingsState(appearance = preferences.appearance()))
    val state: StateFlow<SettingsState> = ui
    private val operations = AtomicLong()

    fun clear() {
        val previous =
            ui.value
        ui.value =
            SettingsState(
                appearance =
                    preferences.appearance(),
                pendingReset =
                    previous.pendingReset,
                busy =
                    previous.busy,
                failure =
                    previous.failure,
            )
    }

    suspend fun refresh(owner: ActiveSession) {
        sends.recoverAccepted(owner.key)
        notifications.refreshStatus(owner)
        val unknown =
            session.preferences
                .drafts(owner.key.profileId)
                .filter {
                    it.phase == SendPhase.QUEUEING &&
                        it.acceptedMessageId == null &&
                        !sends.isInFlight(it.draftId) &&
                        !sends.knowsAccepted(it.draftId) &&
                        composerDrafts
                            .read(
                                owner.key.profileId,
                                it.conversationKey,
                            ).acceptedId == null
                }.map {
                    UnknownSendRow(
                        it.draftId,
                        it.conversationKey,
                    )
                }
        session.withCurrent(owner.key) {
            ui.update {
                it.copy(
                    inbox =
                        owner.client.inboxId(),
                    backend =
                        owner.profile.backend,
                    serverLabel =
                        preferences.label(owner.key.profileId),
                    notifications =
                        CapabilityUi(
                            notifications.configured,
                            if (notifications.configured) null else "Notifications are not configured.",
                        ),
                    notificationStatus =
                        notifications.status.value,
                    notificationsEnabled =
                        notifications.enabled.value,
                    unknownSends =
                    unknown,
                    appearance =
                        preferences.appearance(),
                )
            }
        }
    }

    fun notificationStatus(
        status: String,
        enabled: Boolean,
    ) {
        ui.update {
            it.copy(
                notificationStatus =
                status,
                notificationsEnabled =
                    enabled &&
                        session.active.value != null,
            )
        }
    }

    fun permission(granted: Boolean) {
        ui.update {
            it.copy(
                notificationPermission =
                    if (granted) NotificationPermission.GRANTED else NotificationPermission.DENIED,
            )
        }
    }

    fun dispatch(action: SettingsAction) {
        if (action is SettingsAction.SetAppearance) {
            preferences.setAppearance(action.value)
            ui.update {
                it.copy(
                    appearance =
                        action.value,
                )
            }
            return
        }
        val owner = session.active.value
        val replacing =
            action == SettingsAction.SignOut ||
                action == SettingsAction.DeleteLocalData ||
                action == SettingsAction.ResumeReset
        val origin = owner?.let(destinations::capture)
        val attempt = if (replacing) operations.incrementAndGet() else operations.get()
        if (replacing) connect.cancelRestore()
        (if (replacing) scope else owner?.work ?: scope).launch(Dispatchers.IO) {
            if (!replacing && (owner == null || origin == null || !destinations.accepts(origin))) return@launch
            ui.update {
                it.copy(
                    busy =
                    true,
                    failure =
                    null,
                    pendingReset =
                        action == SettingsAction.DeleteLocalData ||
                            action == SettingsAction.ResumeReset ||
                            it.pendingReset,
                )
            }
            try {
                when (action) {
                    is SettingsAction.SetNotifications -> {
                        notifications.setEnabled(checkNotNull(owner), action.enabled)
                    }

                    is SettingsAction.DiscardUnknownSend -> {
                        session.preferences.removeDraft(
                            checkNotNull(owner).key.profileId,
                            action.draftId,
                        ) { change ->
                            session.admit(
                                owner.key,
                                change,
                            )
                        }
                    }

                    SettingsAction.SignOut -> {
                        commands.signOut()
                    }

                    SettingsAction.DeleteLocalData,
                    SettingsAction.ResumeReset,
                    -> {
                        commands.deleteAccount()
                        if (attempt == operations.get()) {
                            ui.update {
                                it.copy(
                                    pendingReset =
                                    false,
                                )
                            }
                        }
                    }

                    is SettingsAction.SetAppearance -> {
                        Unit
                    }
                }
                if (owner != null && session.accepts(owner.key)) refresh(owner)
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                if (attempt == operations.get() &&
                    (
                        replacing ||
                            origin?.let(destinations::accepts) == true
                    )
                ) {
                    ui.update {
                        it.copy(
                            failure =
                                featureFailure(
                                    error,
                                    if (it.pendingReset) {
                                        "Local data removal is not complete. Try again."
                                    } else {
                                        "Cannot change this setting. Try again."
                                    },
                                    "settings",
                                ),
                        )
                    }
                }
            } finally {
                if (attempt == operations.get() &&
                    (
                        replacing ||
                            origin?.let(destinations::accepts) == true
                    )
                ) {
                    ui.update {
                        it.copy(
                            busy =
                            false,
                        )
                    }
                }
            }
        }
    }
}
