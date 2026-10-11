package org.xmtp.android.example.messenger
import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.exampleStorageLocation
import org.xmtp.android.example.messenger.data.profile.ProfileCleanup
import org.xmtp.android.example.messenger.session.LocalSessionDiagnostics
import org.xmtp.android.example.messenger.session.SessionCommands
import org.xmtp.android.example.messenger.session.SessionDiagnosticSink
import org.xmtp.android.example.messenger.session.SessionLifecycle
import org.xmtp.android.example.messenger.session.SessionProjection
import org.xmtp.android.example.messenger.session.SessionReaders
import uniffi.xmtp_sdk.*
import java.nio.file.Files
import java.nio.file.NoSuchFileException
import java.nio.file.attribute.BasicFileAttributes
import java.security.SecureRandom
import java.util.UUID

data class ActiveSession(
    val key: SessionKey,
    val profile: BackendProfile,
    val paths: ProfilePaths,
    val client: SDKClient,
    val work: CoroutineScope =
        CoroutineScope(
            SupervisorJob() +
                Dispatchers.IO,
        ),
)

/** One process owner for clients,
 transfers and the default message collector. */
class AppSession(
    context: Context,
    projectionFactory: (AppSession) -> SessionProjection,
    lifecycleFactory: (AppSession) -> SessionLifecycle,
    private val diagnostics: SessionDiagnosticSink = LocalSessionDiagnostics,
) : SessionCommands {
    private val context =
        context.applicationContext
    val preferences =
        MessengerPreferences(
            this.context,
        )
    val secrets =
        SecureSecretStore(
            this.context,
        )
    private val fence = SessionFence()
    private val sessionCredentials = SessionCredentials(fence) { profile -> secrets.read(profile, "credential") }
    private val scope =
        CoroutineScope(
            SupervisorJob() +
                Dispatchers.IO,
        )
    private val operation = Mutex()
    private val activeState = MutableStateFlow<ActiveSession?>(null)
    val active: StateFlow<ActiveSession?> = activeState
    private val errorState = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = errorState
    private val readerState = MutableStateFlow<String?>(null)
    val readerError: StateFlow<String?> = readerState
    private val connectionState = MutableStateFlow("")
    val connection: StateFlow<String> = connectionState
    private var stopping: ActiveSession? = null
    private var opening: ActiveSession? = null
    internal var beforeSessionStopLock: suspend () -> Unit = {}
    internal var beforeClientBuild: (String) -> Unit = {}
    internal var beforeOpeningListener: suspend (ActiveSession) -> Unit = {}
    internal var beforeSavedRestoreConnect: suspend (BackendProfile) -> Unit = {}

    // Construct process owners after session state, and before any startup work.
    // Factories can read state and stores. They must not start session commands.
    private val lifecycle = lifecycleFactory(this)
    private val projection = projectionFactory(this)
    private val readers =
        SessionReaders(
            scope = scope,
            projection = projection,
            accepts = ::accepts,
            connection = { key, value -> withCurrent(key) { connectionState.value = value } },
            readerFailure = { key, failure ->
                diagnostics.report("session.reader", failure.javaClass.simpleName)
                withCurrent(key) { readerState.value = "Message updates stopped. Retry to continue." }
            },
            conversationFailure = { key, failure ->
                diagnostics.report("session.conversations", failure.javaClass.simpleName)
                withCurrent(key) { errorState.value = "Cannot refresh conversations. Try again." }
            },
        )
    private val cleanup = ProfileCleanup(this.context, preferences, lifecycle)

    fun admit(
        key: SessionKey,
        change: () -> Unit,
    ): Boolean =
        withCurrent(key) {
            change()
            true
        } == true

    fun accepts(key: SessionKey) =
        fence
            .accepts(key)

    fun <T> withCurrent(
        key: SessionKey,
        block: () -> T,
    ): T? =
        fence
            .withCurrent(
                key,
                block,
            )

    private suspend fun markSignedOutForCleanup(): String? {
        val profile = (activeState.value ?: stopping ?: opening)?.key?.profileId ?: preferences.active()?.id
        preferences.beginExportCleanup(profile)
        return profile
    }

    internal var beforeImmediateExportRevoke: suspend (String) -> Unit = {}

    // Startup needs only local storage. A failure keeps the journal for an awaited retry.
    private val startupExportCleanup =
        scope.launch {
            try {
                replayExportCleanup()
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                diagnostics.report("session.export_cleanup", failure.javaClass.simpleName)
                errorState.value = "Cannot clear file exports. Try again."
            }
        }

    internal suspend fun awaitStartupExportCleanup() = startupExportCleanup.join()

    // Call outside operation. Callers that hold operation use recoverExportCleanup.
    internal suspend fun replayExportCleanup() = operation.withLock { recoverExportCleanup() }

    private suspend fun recoverExportCleanup() = cleanup.replayExports()

    internal var beforeAutomaticProfileLookup: suspend () -> Unit = {}
    internal var beforeBoundConnectOperation: suspend (SessionKey) -> Unit = {}

    override suspend fun restore() {
        val restoreIntent = fence.currentGeneration()
        val saved =
            operation.withLock {
                recoverExportCleanup()
                activeState.value?.let { owner ->
                    withCurrent(owner.key) { if (!readers.started) readers.start(owner) }
                    return
                }
                preferences.reset()?.let {
                    recoverReset(it)
                    return
                }
                if (!preferences.signedIn()) return
                preferences.active() ?: return
            }
        val url = validatedBackendUrl(saved.backend)
        beforeSavedRestoreConnect(saved)
        currentCoroutineContext().ensureActive()
        operation.withLock {
            activeState.value?.let { owner ->
                withCurrent(owner.key) { if (!readers.started) readers.start(owner) }
                return
            }
            preferences.reset()?.let {
                recoverReset(it)
                return
            }
            val generation = fence.reserveRestoreIfCurrent(restoreIntent) ?: return
            if (!preferences.signedIn()) return
            if (preferences.active()?.id != saved.id) return
            connectWithGeneration(
                url,
                null,
                localAttachmentNetwork(url),
                generation,
                operationHeld = true,
            )
        }
    }

    /** Open the saved account for local push reads without default collectors. */
    suspend fun restoreForPush(): ActiveSession? {
        replayExportCleanup()
        return operation.withLock {
            activeState.value?.let { return@withLock it.takeIf { owner -> accepts(owner.key) } }
            if (stopping != null || opening != null) return@withLock null
            val generation = fence.reserveRestore() ?: return@withLock null
            if (!preferences.signedIn() || preferences.reset() != null) return@withLock null
            val selected = preferences.active() ?: return@withLock null
            val url = validatedBackendUrl(selected.backend)
            val profile = selected.copy(backend = url, allowPrivateNetwork = localAttachmentNetwork(url))
            val inbox = profile.inboxId ?: return@withLock null
            val identity = profile.identity ?: return@withLock null
            val paths = profile.paths(context.filesDir)
            if (!pushDatabaseExists(paths)) return@withLock null
            val encryption = secrets.read(profile.id, "database-key") ?: return@withLock null
            val key = fence.bind(profile.id, generation) ?: return@withLock null
            var published = false
            try {
                val source = sessionCredentials.forProfile(profile.id, key)
                val backend = BackendSource.Options(BackendOptions(profile.backend, credentials = source))
                val location = StorageLocation.Explicit(paths.database.absolutePath, paths.attachments.absolutePath)
                val storage = StorageOptions(location = location, encryptionKey = encryption)
                val options =
                    ClientOptions(
                        backend = backend,
                        storage = storage,
                        allowOffline = true,
                        attachments = AttachmentOptions(allowPrivateNetwork = profile.allowPrivateNetwork),
                    )
                val publicIdentity = PublicIdentity(identity, PublicIdentityKind.ETHEREUM)
                if (!prepareStoredNotifications(profile, key, paths, options)) return@withLock null
                beforeClientBuild(url)
                val client = SDKClient.build(context, publicIdentity, options, inbox)
                val owner = ActiveSession(key, profile, paths, client)
                opening = owner
                val selected = preferences.active()?.id
                if (!preferences.signedIn() || preferences.reset() != null) return@withLock null
                if (selected != profile.id) return@withLock null
                beforeOpeningListener(owner)
                startEvents(owner)
                published = withCurrent(key) {
                    activeState.value = owner
                    true
                } == true
                if (published) {
                    opening = null
                    owner
                } else {
                    null
                }
            } finally {
                if (!published) {
                    withContext(NonCancellable) {
                        try {
                            closeCurrent(unregister = false)
                        } finally {
                            fence.releaseRestore(key)
                        }
                    }
                }
            }
        }
    }

    /** Clear old notification state before a normal task runner can renew it. */
    private suspend fun prepareStoredNotifications(
        profile: BackendProfile,
        key: SessionKey,
        paths: ProfilePaths,
        options: ClientOptions,
    ): Boolean {
        val url = validatedBackendUrl(profile.backend)
        val inbox = profile.inboxId ?: return accepts(key)
        val identity = profile.identity ?: return accepts(key)
        if (!lifecycle.needsNotificationPreflight(profile.id) || !pushDatabaseExists(paths)) return accepts(key)
        if (!accepts(key)) return false
        val workers = options.workers ?: WorkerOptions()
        val intervals =
            workers.intervals.filter { it.kind != WorkerKind.TASK_RUNNER } +
                WorkerInterval(WorkerKind.TASK_RUNNER, intervalNs = null, jitterNs = null, enabled = false)
        val quietOptions = options.copy(workers = workers.copy(intervals = intervals))
        beforeClientBuild(url)
        val client =
            SDKClient.build(
                context,
                PublicIdentity(identity, PublicIdentityKind.ETHEREUM),
                quietOptions,
                inbox,
            )
        val owner = ActiveSession(key, profile, paths, client)
        opening = owner
        try {
            if (accepts(key)) lifecycle.stopStoredNotifications(owner)
        } finally {
            withContext(NonCancellable) { closeCurrent(unregister = false) }
        }
        return accepts(key)
    }

    private fun pushDatabaseExists(paths: ProfilePaths): Boolean {
        val database = paths.database.toPath()
        val attributes =
            try {
                Files.readAttributes(database, BasicFileAttributes::class.java)
            } catch (_: NoSuchFileException) {
                return false
            }
        return attributes.isRegularFile && Files.isReadable(database)
    }

    override suspend fun connect(
        backend: String,
        credential: String?,
        allowPrivateNetwork: Boolean,
    ) = connectWithGeneration(backend, credential, allowPrivateNetwork, null)

    private suspend fun connectWithGeneration(
        backend: String,
        credential: String?,
        allowPrivateNetwork: Boolean,
        reservedGeneration: Long?,
        operationHeld: Boolean = false,
    ) {
        val url = validatedBackendUrl(backend)
        val openingGeneration = reservedGeneration ?: fence.reserve()
        if (!fence.isReserved(openingGeneration)) return
        if (reservedGeneration != null) beforeAutomaticProfileLookup()
        val profile =
            preferences
                .profiles()
                .firstOrNull {
                    it.backend == url
                }
                ?: BackendProfile(
                    UUID
                        .randomUUID()
                        .toString(),
                    url,
                    allowPrivateNetwork = allowPrivateNetwork,
                )
        val key =
            fence
                .bind(
                    profile.id,
                    openingGeneration,
                ) ?: return
        beforeBoundConnectOperation(key)
        withOperation(operationHeld) {
            if (!accepts(key)) return
            var retryableProfile: BackendProfile? = null
            var published = false
            try {
                retryableProfile =
                    if (reservedGeneration != null && preferences.signedIn()) {
                        preferences.active()?.takeIf { it.id == profile.id }
                    } else {
                        null
                    }
                check(
                    preferences
                        .reset()
                        ?.profileId !=
                        profile.id,
                ) {
                    "Finish local reset before connecting"
                }
                markSignedOutForCleanup()
                closeCurrent()
                recoverExportCleanup()
                val saved =
                    profile
                        .copy(allowPrivateNetwork = allowPrivateNetwork)
                preferences
                    .setActive(saved)
                if (credential != null) {
                    secrets
                        .write(
                            saved.id,
                            "credential",
                            credential
                                .toByteArray(),
                        )
                }
                val wallet =
                    secrets
                        .read(
                            saved.id,
                            "wallet",
                        ) ?: SecureRandom()
                        .generateSeed(32)
                        .also {
                            secrets
                                .write(
                                    saved.id,
                                    "wallet",
                                    it,
                                )
                        }
                val encryption =
                    secrets
                        .read(
                            saved.id,
                            "database-key",
                        ) ?: SecureRandom()
                        .generateSeed(32)
                        .also {
                            secrets
                                .write(
                                    saved.id,
                                    "database-key",
                                    it,
                                )
                        }
                val paths =
                    saved
                        .paths(
                            context.filesDir,
                        )
                check(
                    paths.database.parentFile!!
                        .isDirectory ||
                        paths.database.parentFile!!
                            .mkdirs(),
                )
                check(
                    paths.attachments
                        .isDirectory ||
                        paths.attachments
                            .mkdirs(),
                )
                val source = sessionCredentials.forProfile(saved.id, key)
                val options =
                    ClientOptions(
                        backend =
                            BackendSource
                                .Options(
                                    BackendOptions(
                                        url = url,
                                        credentials = source,
                                    ),
                                ),
                        storage =
                            StorageOptions(
                                location =
                                    StorageLocation
                                        .Explicit(
                                            paths.database.absolutePath,
                                            paths.attachments.absolutePath,
                                        ),
                                encryptionKey = encryption,
                            ),
                        allowOffline =
                            saved.inboxId != null,
                        attachments = AttachmentOptions(allowPrivateNetwork = allowPrivateNetwork),
                    )
                val signer = localSignerFromPrivateKey(wallet)
                if (!prepareStoredNotifications(saved, key, paths, options)) return
                beforeClientBuild(url)
                val client =
                    if (saved.inboxId != null && saved.identity != null) {
                        SDKClient
                            .build(
                                context,
                                PublicIdentity(
                                    saved.identity,
                                    PublicIdentityKind.ETHEREUM,
                                ),
                                options,
                                saved.inboxId,
                            )
                    } else {
                        SDKClient
                            .create(
                                context,
                                signer,
                                options,
                            )
                    }
                val pending = ActiveSession(key, saved, paths, client)
                opening = pending
                if (!accepts(key)) return
                val opened =
                    saved
                        .copy(
                            inboxId =
                                client
                                    .inboxId(),
                            identity =
                                client
                                    .identity()
                                    .identifier,
                        )
                val owner = pending.copy(profile = opened)
                opening = owner
                beforeOpeningListener(owner)
                startEvents(owner)
                if (!accepts(key)) return
                val committed =
                    preferences.commitSession(opened) { change ->
                        withCurrent(key) {
                            change()
                            true
                        } == true
                    }
                published =
                    committed && withCurrent(key) {
                        activeState.value = owner
                        errorState.value = null
                        readers.start(owner)
                        true
                    } == true
                if (published) opening = null
            } finally {
                if (!published) {
                    withContext(NonCancellable) {
                        try {
                            closeCurrent(unregister = false)
                        } finally {
                            if (reservedGeneration != null) {
                                try {
                                    retryableProfile?.let { selected ->
                                        preferences.commitSession(selected) { change ->
                                            withCurrent(key) {
                                                change()
                                                true
                                            } == true
                                        }
                                    }
                                } finally {
                                    fence.releaseRestore(key)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    private suspend inline fun <T> withOperation(
        alreadyHeld: Boolean,
        action: () -> T,
    ): T = if (alreadyHeld) action() else operation.withLock { action() }

    private suspend fun startEvents(owner: ActiveSession) {
        val key = owner.key
        val client = owner.client
        // Register events before exposing the session for initial local reads.
        client
            .startListener(
                EventFilter(
                    kinds =
                        EventKind.entries,
                ),
            ) { event ->
                if (accepts(key)) {
                    try {
                        if (event is ClientEvent.ConsentChanged || event is ClientEvent.ConversationRemoved ||
                            event is ClientEvent.ConversationMembershipChanged ||
                            event is ClientEvent.ConversationPaused
                        ) {
                            lifecycle.onAdmissionChanged(owner)
                        }
                        projection.event(
                            owner,
                            event,
                        )
                        projection.invalidate(owner)
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        diagnostics.report("session.events", error.javaClass.simpleName)
                        withCurrent(key) {
                            errorState.value = "Cannot refresh conversations. Try again."
                        }
                    }
                }
            }
    }

    override suspend fun retryReader() =
        operation.withLock {
            val owner =
                activeState.value ?: return
            readers.stop()
            if (accepts(
                    owner.key,
                )
            ) {
                readerState.value = null
                readers.start(owner)
            }
        }

    private suspend fun closeCurrent(unregister: Boolean = true) {
        readers.stop()
        val owner =
            activeState.value ?: stopping ?: opening
        stopping = owner
        opening = null
        activeState.value = null
        if (owner != null) {
            withContext(NonCancellable) {
                owner.work
                    .coroutineContext[Job]
                    ?.cancelAndJoin()
                try {
                    try {
                        if (unregister) lifecycle.unregisterNotifications(owner.client)
                    } catch (error: Exception) {
                        if (error is CancellationException) throw error
                    }
                    lifecycle.beforeEnd(owner)
                } finally {
                    try {
                        cleanup.finishExports(owner.key.profileId)
                    } finally {
                        owner.client.end()
                    }
                }
                stopping = null
            }
        }
        readerState.value = null
        connectionState.value = ""
    }

    override suspend fun signOut() {
        val stopGeneration = fence.reserve()
        lifecycle.onInvalidated()
        beforeSessionStopLock()
        operation.withLock {
            if (!fence.isReserved(stopGeneration)) return@withLock
            val cleanupProfile = markSignedOutForCleanup()
            withContext(NonCancellable) {
                cleanupProfile?.let { profile ->
                    beforeImmediateExportRevoke(profile)
                    try {
                        cleanup.revokeExports(profile)
                    } catch (_: Exception) {
                        // The journal and final cleanup retain the retry after owner drain.
                    }
                }
                try {
                    closeCurrent()
                    recoverExportCleanup()
                } finally {
                    preferences
                        .active()
                        ?.let {
                            secrets
                                .delete(
                                    it.id,
                                    "credential",
                                )
                        }
                }
            }
        }
    }

    override suspend fun deleteAccount() {
        val stopGeneration = fence.reserve()
        beforeSessionStopLock()
        operation.withLock {
            if (!fence.isReserved(stopGeneration)) return@withLock
            markSignedOutForCleanup()
            var record =
                preferences
                    .reset()
            if (record == null) {
                val profile =
                    preferences
                        .active() ?: error("No local account is selected")
                record =
                    profile
                        .paths(
                            context.filesDir,
                        ).resetRecord(
                            profile.id,
                        )
                preferences
                    .saveReset(record)
                val storage =
                    (activeState.value ?: stopping)
                        ?.takeIf {
                            it.profile.id ==
                                profile.id
                        }?.client
                        ?.storage()
                closeCurrent()
                if (storage != null) {
                    storage
                        .delete()
                    ResetCleanup(
                        context.filesDir,
                    ).removeDatabase(
                        record,
                        activeState.value == null,
                    )
                    record =
                        record
                            .copy(
                                phase =
                                    ResetPhase.DATABASE_REMOVED,
                            )
                    preferences
                        .saveReset(record)
                }
            }
            if (activeState.value?.profile?.id ==
                record
                    .profileId || stopping?.profile?.id ==
                record.profileId
            ) {
                closeCurrent()
            }
            recoverReset(record)
        }
    }

    override suspend fun resetLegacyAccount(inboxId: String) {
        require(
            inboxId
                .matches(Regex("[0-9a-f]{64}")),
        )
        fence
            .replace(null)
        operation.withLock {
            check(
                activeState.value == null,
            ) {
                "Sign out before a legacy reset"
            }
            val location =
                exampleStorageLocation(
                    context.filesDir,
                ) {
                    inboxId
                }
            val explicit =
                location as? StorageLocation.Explicit ?: error("Selected legacy storage is unavailable")
            val record =
                ResetRecord(
                    "legacy-$inboxId",
                    explicit.dbPath,
                    listOf(
                        explicit.attachmentsDir,
                        explicit.dbPath,
                    ),
                    ResetPhase.STOPPING,
                )
            markSignedOutForCleanup()
            preferences
                .saveReset(record)
            recoverReset(record)
        }
    }

    private suspend fun recoverReset(initial: ResetRecord) {
        cleanup.recoverReset(
            initial,
            noOwner =
                activeState.value?.profile?.id != initial.profileId &&
                    stopping?.profile?.id != initial.profileId &&
                    opening?.profile?.id != initial.profileId,
        )
        errorState.value = null
    }
}
