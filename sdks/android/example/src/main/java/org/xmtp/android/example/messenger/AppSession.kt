package org.xmtp.android.example.messenger
import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.exampleStorageLocation
import uniffi.xmtp_sdk.*
import java.io.File
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
) {
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
    private var messageJob: Job? = null
    private var conversationJob: Job? = null
    private var stopping: ActiveSession? = null
    private var opening: ActiveSession? = null
    internal var beforeClientBuild: (String) -> Unit = {}
    internal var beforeOpeningListener: suspend (ActiveSession) -> Unit = {}
    var onMessage: suspend (
        ActiveSession,
        Message,
    ) -> Unit = {
        _,
        _,
        ->
    }
    var onInvalidated: suspend (ActiveSession) -> Unit = {
    }
    var onEvent: suspend (
        ActiveSession,
        ClientEvent,
    ) -> Unit = {
        _,
        _,
        ->
    }
    var beforeEnd: suspend (ActiveSession) -> Unit = {
    }
    var unregisterNotifications: suspend (SDKClient) -> Unit = {
        it
            .disableNotifications()
    }

    internal var onSessionInvalidated: () -> Unit = {}

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

    suspend fun restore() {
        operation.withLock {
            preferences
                .reset()
                ?.let {
                    recoverReset(it)
                    return
                }
        }
        if (preferences
                .signedIn()
        ) {
            preferences
                .active()
                ?.let {
                    connect(
                        it.backend,
                        null,
                        localAttachmentNetwork(it.backend),
                    )
                }
        }
    }

    suspend fun connect(
        backend: String,
        credential: String?,
        allowPrivateNetwork: Boolean,
    ) {
        val url = validatedBackendUrl(backend)
        val openingGeneration =
            fence
                .reserve()
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
        operation.withLock {
            if (!accepts(key)) return
            check(
                preferences
                    .reset()
                    ?.profileId !=
                    profile.id,
            ) {
                "Finish local reset before connecting"
            }
            preferences
                .setSignedIn(false)
            closeCurrent()
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
            val source =
                if (secrets
                        .read(
                            saved.id,
                            "credential",
                        )?.isNotEmpty() == true
                ) {
                    object : CredentialSource {
                        override suspend fun credential(): Credential {
                            if (!accepts(key)) {
                                throw CredentialException
                                    .Failed()
                            }
                            val value =
                                secrets
                                    .read(
                                        saved.id,
                                        "credential",
                                    )?.toString(
                                        Charsets.UTF_8,
                                    ) ?: throw CredentialException
                                    .Failed()
                            return Credential(
                                name = null,
                                value = value,
                                expiresAtSeconds =
                                    Long.MAX_VALUE,
                            )
                        }
                    }
                } else {
                    null
                }
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
            var published = false
            try {
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
                                onEvent(
                                    owner,
                                    event,
                                )
                                onInvalidated(owner)
                            } catch (error: Throwable) {
                                if (error is CancellationException) throw error
                                if (accepts(key)) {
                                    errorState.value =
                                        error
                                            .toString()
                                }
                            }
                        }
                    }
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
                        startReaders(owner)
                        true
                    } == true
                if (published) opening = null
            } finally {
                if (!published) withContext(NonCancellable) { closeCurrent() }
            }
        }
    }

    private fun startReaders(owner: ActiveSession) {
        messageJob =
            scope.launch {
                try {
                    owner.client.conversations
                        .streamAllMessages(
                            MessageStreamOptions(
                                consentStates =
                                    listOf(
                                        ConsentState.ALLOWED,
                                        ConsentState.UNKNOWN,
                                    ),
                                onConnectionStateChange = {
                                    _,
                                    value,
                                    ->
                                    if (accepts(
                                            owner.key,
                                        )
                                    ) {
                                        connectionState.value =
                                            value
                                                .toString()
                                    }
                                },
                            ),
                        ).collect { message ->
                            // No asynchronous Flow operator occurs before this completed app handling.
                            if (!accepts(
                                    owner.key,
                                )
                            ) {
                                throw CancellationException("Session changed")
                            }
                            onMessage(
                                owner,
                                message,
                            )
                            if (!accepts(owner.key)) throw CancellationException("Session changed during handling")
                        }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (accepts(
                            owner.key,
                        )
                    ) {
                        readerState.value =
                            error
                                .toString()
                    }
                }
            }
        conversationJob =
            scope.launch {
                try {
                    owner.client.conversations
                        .stream(
                            ConversationStreamOptions(
                                consentStates =
                                    listOf(
                                        ConsentState.ALLOWED,
                                        ConsentState.UNKNOWN,
                                    ),
                            ),
                        ).collect {
                            if (accepts(
                                    owner.key,
                                )
                            ) {
                                onInvalidated(owner)
                            }
                        }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (accepts(
                            owner.key,
                        )
                    ) {
                        errorState.value =
                            error
                                .toString()
                    }
                }
            }
    }

    suspend fun retryReader() =
        operation.withLock {
            val owner =
                activeState.value ?: return
            messageJob?.cancelAndJoin()
            conversationJob?.cancelAndJoin()
            if (accepts(
                    owner.key,
                )
            ) {
                readerState.value = null
                startReaders(owner)
            }
        }

    private suspend fun closeCurrent() {
        messageJob?.cancelAndJoin()
        messageJob = null
        conversationJob?.cancelAndJoin()
        conversationJob = null
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
                    beforeEnd(owner)
                } finally {
                    owner.client
                        .end()
                }
                stopping = null
            }
        }
        readerState.value = null
        connectionState.value = ""
    }

    suspend fun signOut() {
        fence
            .replace(null)
        onSessionInvalidated()
        operation.withLock {
            preferences
                .setSignedIn(false)
            val owner =
                activeState.value
            withContext(NonCancellable) {
                try {
                    owner?.client?.let {
                        unregisterNotifications(it)
                    }
                } catch (_: Exception) {
                    // Signed-out state already blocks late push.
                }
                try {
                    closeCurrent()
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

    suspend fun deleteAccount() {
        fence
            .replace(null)
        operation.withLock {
            preferences
                .setSignedIn(false)
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

    suspend fun resetLegacyAccount(inboxId: String) {
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
            preferences
                .setSignedIn(false)
            preferences
                .saveReset(record)
            recoverReset(record)
        }
    }

    private suspend fun recoverReset(initial: ResetRecord) {
        check(
            activeState.value?.profile?.id !=
                initial
                    .profileId && stopping?.profile?.id !=
                initial.profileId,
        ) {
            "Cannot reset an open profile"
        }
        val cleanup =
            ResetCleanup(
                context.filesDir,
            )
        var record = initial
        if (record.phase ==
            ResetPhase.STOPPING
        ) {
            cleanup
                .removeDatabase(
                    record,
                    noOwner =
                        activeState.value?.profile?.id !=
                            record
                                .profileId && stopping?.profile?.id !=
                            record.profileId,
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
        if (record.phase ==
            ResetPhase.DATABASE_REMOVED
        ) {
            cleanup
                .removeFiles(record)
            record =
                record
                    .copy(
                        phase =
                            ResetPhase.FILES_REMOVED,
                    )
            preferences
                .saveReset(record)
        }
        preferences
            .removeProfile(
                record.profileId,
            )
        preferences
            .saveReset(null)
        errorState.value = null
    }
}
