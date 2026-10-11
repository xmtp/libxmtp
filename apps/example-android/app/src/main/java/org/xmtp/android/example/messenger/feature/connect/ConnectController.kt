package org.xmtp.android.example.messenger.feature.connect

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.data.xmtp.BackendProbe
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.featureFailure
import org.xmtp.android.example.messenger.session.SessionCommands
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.connect.*
import java.util.concurrent.atomic.AtomicLong

internal class ConnectController(
    private val session: AppSession,
    private val commands: SessionCommands,
    private val probe: BackendProbe,
    private val preferences: AppFeaturePreferences,
    private val destinations: DestinationScope,
    private val scope: CoroutineScope,
    backend: String,
    private val legacyAccounts: () -> List<String>,
) {
    private val ui = MutableStateFlow(ConnectState(backend = backend))
    val state: StateFlow<ConnectState> = ui
    private val operations = AtomicLong()
    private val inspections = AtomicLong()
    private var restoreJob: Job? = null
    private var probeJob: Job? = null
    private var started = false
    private var lastAttempt: Pair<String, String?>? = null
    internal var inspectBackend: suspend (String) -> uniffi.xmtp_sdk.ServerConfiguration = probe::inspect
    internal var onBackendProbeFinished: (Long, String) -> Unit = { _, _ -> }

    fun start() {
        if (started) return
        started = true
        restoreJob =
            scope.launch(Dispatchers.IO) {
                try {
                    if (operations.get() != 0L) return@launch
                    val saved = session.preferences.active()
                    if (saved != null &&
                        session.preferences.signedIn() &&
                        operations.get() == 0L
                    ) {
                        ui.update {
                            it.copy(
                                backend =
                                    saved.backend,
                                label =
                                    preferences.label(saved.id),
                            )
                        }
                    }
                    ensureActive()
                    if (operations.get() != 0L) return@launch
                    commands.restore()
                    if (session.active.value == null &&
                        session.preferences.profiles().isEmpty() &&
                        operations.get() == 0L
                    ) {
                        val accounts = legacyAccounts()
                        ui.update { it.copy(migrationRequired = accounts.isNotEmpty(), migrationAccounts = accounts) }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (operations.get() == 0L) {
                        ui.update {
                            it.copy(
                                pendingReset =
                                    session.preferences.reset() != null,
                                failure =
                                    featureFailure(
                                        error,
                                        "Cannot restore this account. Try again.",
                                        "restore",
                                    ),
                            )
                        }
                    }
                }
            }
    }

    internal suspend fun awaitStartupRestore() {
        restoreJob?.join()
    }

    fun cancelRestore() {
        operations.incrementAndGet()
        restoreJob?.cancel()
    }

    fun dispatch(action: ConnectAction) {
        when (action) {
            is ConnectAction.ChangeBackend -> {
                ui.update {
                    it.copy(
                        backend =
                            action.value,
                        failure =
                        null,
                    )
                }
                inspect(action.value)
            }

            is ConnectAction.Inspect -> {
                inspect(action.backend)
            }

            is ConnectAction.ChangeLabel -> {
                ui.update { it.copy(label = action.value) }
            }

            ConnectAction.ToggleAdvanced -> {
                ui.update { it.copy(advanced = !it.advanced) }
            }

            is ConnectAction.Connect -> {
                lastAttempt = action.backend to action.credential
                ui.update { it.copy(backend = action.backend) }
                transition {
                    commands.connect(
                        action.backend,
                        action.credential,
                        localAttachmentNetwork(action.backend),
                    )
                    session.active.value?.let { owner ->
                        preferences.setLabel(
                            owner.key,
                            ui.value.label,
                        )
                    }
                    lastAttempt =
                        null
                }
            }

            ConnectAction.Retry -> {
                val previous =
                    lastAttempt
                transition {
                    if (previous == null) {
                        commands.restore()
                    } else {
                        commands.connect(
                            previous.first,
                            previous.second,
                            localAttachmentNetwork(previous.first),
                        )
                        session.active.value?.let {
                            preferences.setLabel(
                                it.key,
                                ui.value.label,
                            )
                        }
                        lastAttempt =
                            null
                    }
                }
            }

            ConnectAction.ResumeReset -> {
                transition(reset = true) { commands.deleteAccount() }
            }

            is ConnectAction.ResetLegacyAccount -> {
                transition {
                    commands.resetLegacyAccount(action.inboxId)
                    ui.update {
                        it.copy(
                            migrationAccounts =
                                it.migrationAccounts - action.inboxId,
                            migrationRequired =
                                it.migrationAccounts.size > 1,
                        )
                    }
                }
            }
        }
    }

    private fun transition(
        reset: Boolean = false,
        command: suspend () -> Unit,
    ) {
        val attempt = operations.incrementAndGet()
        restoreJob?.cancel()
        scope.launch(Dispatchers.IO) {
            ui.update { it.copy(busy = true, pendingReset = reset || it.pendingReset, failure = null) }
            try {
                if (attempt != operations.get()) return@launch
                command()
                if (attempt == operations.get()) {
                    ui.update { it.copy(pendingReset = false) }
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                if (attempt == operations.get()) {
                    ui.update {
                        it.copy(
                            failure =
                                featureFailure(
                                    error,
                                    if (reset) {
                                        "Local data removal is not complete. Try again."
                                    } else {
                                        "Cannot connect to this server. Check the address and try again."
                                    },
                                    if (reset) "reset" else "connect",
                                ),
                        )
                    }
                }
            } finally {
                if (attempt == operations.get()) ui.update { it.copy(busy = false) }
            }
        }
    }

    private fun inspect(backend: String) {
        val address = backend.trim().trimEnd('/')
        val attempt = inspections.incrementAndGet()
        val token = destinations.state.value.token
        ui.update {
            it.copy(
                backend =
                backend,
                normalizedBackend =
                address,
                probe =
                    ProbeState.CHECKING,
                credentialsRequiredFor =
                null,
                failure =
                null,
            )
        }
        probeJob?.cancel()
        probeJob =
            scope.launch(Dispatchers.IO) {
                try {
                    delay(300)
                    val result = inspectBackend(validatedBackendUrl(address))
                    if (attempt == inspections.get() &&
                        token == destinations.state.value.token &&
                        destinations.state.value.route == AppRoute.Connect &&
                        ui.value.normalizedBackend == address
                    ) {
                        ui.update {
                            it.copy(
                                probe =
                                    ProbeState.READY,
                                credentialsRequiredFor =
                                    address.takeIf {
                                        result.auth.enabled
                                    },
                            )
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (attempt == inspections.get() &&
                        token == destinations.state.value.token
                    ) {
                        ui.update {
                            it.copy(
                                probe =
                                    ProbeState.FAILED,
                                credentialsRequiredFor =
                                null,
                                failure =
                                    featureFailure(
                                        error,
                                        "Cannot check this server. Try again.",
                                        "server-probe",
                                    ),
                            )
                        }
                    }
                } finally {
                    onBackendProbeFinished(attempt, address)
                }
            }
    }
}
