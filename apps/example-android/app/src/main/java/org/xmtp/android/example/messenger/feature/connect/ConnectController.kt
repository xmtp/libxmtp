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
    private var inspectedDestinationToken: Long? = null
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
                    if (saved != null && session.preferences.signedIn()) {
                        val label = preferences.label(saved.id)
                        session.withStateLock {
                            if (operations.get() == 0L) {
                                ui.update { it.copy(backend = saved.backend, label = label) }
                                inspectCurrent()
                            }
                        }
                    }
                    ensureActive()
                    if (operations.get() != 0L) return@launch
                    commands.restore()
                    if (session.active.value == null && session.preferences.profiles().isEmpty()) {
                        val accounts = legacyAccounts()
                        session.withStateLock {
                            if (operations.get() == 0L && session.active.value == null) {
                                ui.update {
                                    it.copy(migrationRequired = accounts.isNotEmpty(), migrationAccounts = accounts)
                                }
                            }
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (operations.get() == 0L) {
                        val pendingReset = session.preferences.reset() != null
                        val failure =
                            featureFailure(
                                error,
                                "Cannot restore this account. Try again.",
                                "restore",
                            )
                        session.withStateLock {
                            if (operations.get() == 0L) {
                                ui.update { it.copy(pendingReset = pendingReset, failure = failure) }
                            }
                        }
                    }
                }
            }
    }

    internal suspend fun awaitStartupRestore() {
        restoreJob?.join()
    }

    fun cancelRestore() {
        session.withStateLock {
            operations.incrementAndGet()
            ui.update { it.copy(busy = false) }
        }
        restoreJob?.cancel()
    }

    fun dispatch(action: ConnectAction) {
        when (action) {
            is ConnectAction.ChangeBackend -> {
                inspect(action.value)
            }

            ConnectAction.InspectCurrent -> {
                inspectCurrent()
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
                session.withStateLock {
                    lastAttempt = action.backend to action.credential
                    ui.update { it.copy(backend = action.backend, hasAttemptedJoin = true) }
                    transition { attempt ->
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
                        session.withStateLock {
                            if (attempt == operations.get()) lastAttempt = null
                        }
                    }
                }
            }

            ConnectAction.Retry -> {
                session.withStateLock {
                    val previous =
                        lastAttempt
                    transition(restoring = previous == null) { attempt ->
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
                            session.withStateLock {
                                if (attempt == operations.get()) lastAttempt = null
                            }
                        }
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
        restoring: Boolean = false,
        command: suspend (Long) -> Unit,
    ) {
        val attempt =
            session.withStateLock {
                val next = operations.incrementAndGet()
                ui.update { it.copy(busy = true, pendingReset = reset || it.pendingReset, failure = null) }
                next
            }
        restoreJob?.cancel()
        scope.launch(Dispatchers.IO) {
            try {
                if (attempt != operations.get()) return@launch
                command(attempt)
                session.withStateLock {
                    if (attempt == operations.get()) ui.update { it.copy(pendingReset = false) }
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                session.withStateLock {
                    if (attempt == operations.get()) {
                        ui.update {
                            it.copy(
                                failure =
                                    featureFailure(
                                        error,
                                        if (reset) {
                                            "Local data removal is not complete. Try again."
                                        } else if (restoring) {
                                            "Cannot restore this account. Try again."
                                        } else {
                                            "Cannot connect to this server. Check the address and try again."
                                        },
                                        when {
                                            reset -> "reset"
                                            restoring -> "restore"
                                            else -> "connect"
                                        },
                                    ),
                            )
                        }
                    }
                }
            } finally {
                session.withStateLock {
                    if (attempt == operations.get()) ui.update { it.copy(busy = false) }
                }
            }
        }
    }

    private fun inspectCurrent() {
        session.withStateLock {
            val current = ui.value
            val destination = destinations.state.value
            if (destination.route != AppRoute.Connect || current.backend.isBlank()) return@withStateLock
            val address = current.backend.trim().trimEnd('/')
            if (inspectedDestinationToken == destination.token && current.normalizedBackend == address &&
                (current.probe == ProbeState.CHECKING || current.probe == ProbeState.READY)
            ) {
                return@withStateLock
            }
            inspect(current.backend)
        }
    }

    private fun inspect(backend: String) {
        session.withStateLock {
            val address = backend.trim().trimEnd('/')
            val attempt = inspections.incrementAndGet()
            val token = destinations.state.value.token
            val newDestination = inspectedDestinationToken != token
            inspectedDestinationToken = token
            ui.update {
                val changed = address != it.normalizedBackend || newDestination
                it.copy(
                    backend = backend,
                    normalizedBackend = address,
                    probe = ProbeState.CHECKING,
                    credentialsRequiredFor = null,
                    hasAttemptedJoin = it.hasAttemptedJoin && !changed,
                    failure =
                        it.failure.takeUnless { failure ->
                            failure?.code == "server-probe" || (changed && failure?.code == "connect")
                        },
                )
            }
            probeJob?.cancel()
            probeJob =
                scope.launch(Dispatchers.IO) {
                    try {
                        delay(300)
                        val result = inspectBackend(validatedBackendUrl(address))
                        session.withStateLock {
                            if (attempt == inspections.get() &&
                                token == destinations.state.value.token &&
                                destinations.state.value.route == AppRoute.Connect &&
                                ui.value.normalizedBackend == address &&
                                ui.value.backend
                                    .trim()
                                    .trimEnd('/') == address
                            ) {
                                ui.update {
                                    it.copy(
                                        probe = ProbeState.READY,
                                        credentialsRequiredFor = address.takeIf { result.auth.enabled },
                                    )
                                }
                            }
                        }
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        session.withStateLock {
                            if (attempt == inspections.get() &&
                                token == destinations.state.value.token &&
                                destinations.state.value.route == AppRoute.Connect &&
                                ui.value.normalizedBackend == address &&
                                ui.value.backend
                                    .trim()
                                    .trimEnd('/') == address
                            ) {
                                ui.update {
                                    it.copy(
                                        probe = ProbeState.FAILED,
                                        credentialsRequiredFor = null,
                                        failure =
                                            it.failure.takeUnless { failure -> failure?.code == "server-probe" }
                                                ?: featureFailure(
                                                    error,
                                                    "Cannot check this server. Try again.",
                                                    "server-probe",
                                                ),
                                    )
                                }
                            }
                        }
                    } finally {
                        onBackendProbeFinished(attempt, address)
                    }
                }
        }
    }
}
