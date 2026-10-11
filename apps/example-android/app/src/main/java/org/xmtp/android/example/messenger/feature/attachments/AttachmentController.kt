package org.xmtp.android.example.messenger.feature.attachments

import android.content.Context
import android.graphics.Bitmap
import android.net.Uri
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.attachments.*
import org.xmtp.android.example.messenger.platform.EffectIdentity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.*
import org.xmtp.android.example.shared.feature.attachments.AttachmentState
import uniffi.xmtp_sdk.*

/** Owns attachment transfers and recovery across Activity replacement. */
internal class AttachmentController(
    context: Context,
    private val session: AppSession,
    private val sends: SendCoordinator,
    private val destinations: DestinationScope,
    private val chat: AttachmentDestination,
    private val scope: CoroutineScope,
) {
    private val context = context.applicationContext
    internal val drafts = MutableStateFlow<List<AttachmentCardState>>(emptyList())
    internal val downloads = MutableStateFlow<Map<String, AttachmentCardState>>(emptyMap())
    internal val previews = MutableStateFlow<Map<String, Bitmap>>(emptyMap())
    internal val error = MutableStateFlow<AppFailure?>(null)
    private val capability = MutableStateFlow(CapabilityUi())
    val availability = capability.asStateFlow()
    internal val requests = AttachmentRequests()
    internal val verified = MutableStateFlow<Set<String>>(emptySet())

    val state =
        combine(drafts, downloads, verified, capability, error) { drafts, messages, ids, capability, failure ->
            AttachmentState(drafts, messages, ids, capability, failure)
        }.stateIn(scope, SharingStarted.Eagerly, AttachmentState())

    private data class Services(
        val key: SessionKey,
        val coordinator: AttachmentDraftCoordinator,
        val files: AttachmentFiles,
    )

    private data class Work(
        val active: ActiveSession,
        val token: Long,
        val services: Services,
    )

    private var services: Services? = null
    private val refreshMutex = Mutex()
    internal var beforeDownloadRefresh: suspend () -> Unit = {}
    internal var beforeDownload: suspend () -> Unit = {}
    internal var beforeWatchRecovery: () -> Unit = {}
    internal var beforeWatchRefresh: suspend () -> Unit = {}
    internal var onWatchFinished: () -> Unit = {}
    internal var beforeSupportAdmission: suspend (Boolean, Long) -> Unit = { _, _ -> }

    internal fun verifiedMessageIds() =
        services
            ?.files
            ?.retained
            ?.value
            .orEmpty()

    private val started =
        java.util.concurrent.atomic
            .AtomicBoolean()

    /** Start only after the graph has constructed the session and route owner. */
    fun start() {
        if (!started.compareAndSet(false, true)) return
        scope.launch {
            session.active.collectLatest { active ->
                services?.files?.clearVerified()
                drafts.value = emptyList()
                downloads.value = emptyMap()
                previews.value = emptyMap()
                error.value = null
                capability.value = CapabilityUi()
                verified.value = emptySet()
                requests.retain(::accepts)
                if (active != null) {
                    val watch =
                        active.work.launch {
                            try {
                                beforeWatchRefresh()
                                val bound =
                                    refreshServices(active, beforeRecovery = { beforeWatchRecovery() }) ?: return@launch
                                launch {
                                    bound.files.retained.collect {
                                        session.admit(active.key) { verified.value = it }
                                    }
                                }
                                bound.coordinator.cards.collect {
                                    if (session.accepts(active.key)) drafts.value = it
                                }
                            } catch (failure: CancellationException) {
                                throw failure
                            } catch (failure: Throwable) {
                                if (session.accepts(active.key)) error.value = failure.attachmentFailure()
                            } finally {
                                onWatchFinished()
                            }
                        }
                    try {
                        watch.join()
                    } finally {
                        watch.cancelAndJoin()
                    }
                }
            }
        }
        scope.launch {
            var token = destinations.state.value.token
            destinations.state.collectLatest { destination ->
                if (token != destination.token) {
                    token = destination.token
                    services?.files?.clearVerified()
                    downloads.value = emptyMap()
                    previews.value = emptyMap()
                    error.value = null
                    verified.value = emptySet()
                    requests.retain(::accepts)
                }
                val active = session.active.value ?: return@collectLatest
                if (!acceptsScreen(active.key, destination.token)) return@collectLatest
                // A route bind can follow the active-session observation. Retry admission at its new token.
                val refresh = active.work.async { refreshServices(active, destination.token) }
                try {
                    refresh.await()
                } finally {
                    refresh.cancelAndJoin()
                }
            }
        }
    }

    private fun acceptsScreen(
        key: SessionKey,
        token: Long,
    ): Boolean = session.accepts(key) && destinations.accepts(DestinationOrigin(key, token))

    private fun accepts(identity: EffectIdentity): Boolean =
        acceptsScreen(identity.sessionKey, identity.destinationToken)

    private fun identity(
        active: ActiveSession,
        token: Long,
    ) = EffectIdentity(requests.nextId(), active.key, token)

    fun selectFile(selection: AttachmentSelection = AttachmentSelection.FILE) {
        val conversation = chat.currentConversation() ?: return
        perform { work ->
            if (!uploadSupported(work.active)) return@perform
            if (current(work)) {
                requests.offer(
                    AttachmentRequest.Select(identity(work.active, work.token), conversation.id(), selection),
                )
            }
        }
    }

    internal fun selected(
        identity: EffectIdentity,
        uri: Uri?,
    ) {
        val request = requests.consume(identity) as? AttachmentRequest.Select ?: return
        if (uri == null || !accepts(identity)) return
        perform(identity) { work ->
            work.services.coordinator.select(context.contentResolver, uri, request.conversationId) { current(work) }
        }
    }

    internal fun saved(
        identity: EffectIdentity,
        uri: Uri?,
    ) {
        val request = requests.consume(identity) as? AttachmentRequest.Save ?: return
        if (uri == null || !accepts(identity)) return
        perform(identity) { work ->
            readableRemote(work.active, request.messageId)
            check(current(work)) { "The screen changed" }
            work.services.files.save(request.downloaded, uri)
        }
    }

    /** Launch and admission are synchronous under the session and destination fences. */
    internal fun launch(
        request: AttachmentRequest,
        hostId: Long,
        action: () -> Unit,
    ) {
        if (!requests.claim(request, hostId)) return
        var launched = false
        try {
            session.withCurrent(request.identity.sessionKey) {
                destinations.admit(DestinationOrigin(request.identity.sessionKey, request.identity.destinationToken)) {
                    requests.withHost(hostId) {
                        action()
                        launched = true
                        if (request is AttachmentRequest.Open) request.export.launched()
                    }
                }
            }
        } catch (failure: Throwable) {
            if (failure is CancellationException) throw failure
            if (accepts(request.identity)) error.value = failure.attachmentFailure()
        } finally {
            if (!launched || request is AttachmentRequest.Open) {
                requests.consume(request.identity)
                if (request is AttachmentRequest.Open) request.export.close()
            }
        }
    }

    suspend fun refresh(
        active: ActiveSession,
        token: Long = destinations.state.value.token,
    ) {
        refreshServices(active, token)
    }

    private suspend fun uploadSupported(active: ActiveSession): Boolean =
        active.client.serverConfiguration().attachments != null && active.client.attachments().offered()

    private fun current(work: Work) = acceptsScreen(work.active.key, work.token)

    private suspend fun refreshServices(
        active: ActiveSession,
        token: Long = destinations.state.value.token,
        beforeRecovery: () -> Unit = {},
    ): Services? =
        refreshMutex.withLock {
            if (!session.accepts(active.key)) return@withLock null
            val bound =
                services?.takeIf { it.key == active.key } ?: Services(
                    active.key,
                    AttachmentDraftCoordinator(
                        active.key,
                        active.client,
                        active.paths,
                        session.preferences,
                        session.secrets,
                        sends,
                        session::accepts,
                        { change -> session.admit(active.key, change) },
                    ),
                    AttachmentFiles(context, active.key, active.client, session::accepts),
                ).also { services = it }
            try {
                if (!acceptsScreen(active.key, token)) return@withLock bound
                val supported = uploadSupported(active)
                beforeSupportAdmission(supported, token)
                destinations.admit(DestinationOrigin(active.key, token)) {
                    if (session.accepts(active.key)) {
                        capability.value =
                            CapabilityUi(
                                supported,
                                if (supported) null else "This server does not support file uploads.",
                            )
                    }
                }
                if (!session.accepts(active.key)) return@withLock null
                if (!acceptsScreen(active.key, token)) return@withLock bound
                beforeRecovery()
                bound.coordinator.recover()
                session.admit(active.key) {
                    if (acceptsScreen(active.key, token)) drafts.value = bound.coordinator.cards.value
                }
                bound
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                if (!session.accepts(active.key)) return@withLock null
                if (acceptsScreen(active.key, token)) error.value = failure.attachmentFailure()
                bound
            }
        }

    private fun perform(
        identity: EffectIdentity? = null,
        block: suspend (Work) -> Unit,
    ) {
        val active = session.active.value ?: return
        val token = identity?.destinationToken ?: destinations.state.value.token
        if (identity != null && (identity.sessionKey != active.key || !accepts(identity))) return
        active.work.launch {
            var bound: Services? = null
            try {
                bound = refreshServices(active, token) ?: return@launch
                val work = Work(active, token, bound)
                check(current(work)) { "The screen changed" }
                block(work)
                if (current(work)) {
                    drafts.value = bound.coordinator.cards.value
                    error.value = null
                    chat.refresh()
                }
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                if (acceptsScreen(active.key, token)) {
                    bound?.let { drafts.value = it.coordinator.cards.value }
                    error.value = failure.attachmentFailure()
                }
            }
        }
    }

    fun dispatch(action: AttachmentAction) {
        when (action) {
            AttachmentAction.Select -> {
                selectFile()
            }

            is AttachmentAction.ViewChat -> {
                chat.openConversation(action.conversationId)
            }

            is AttachmentAction.Assign -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    work.services.coordinator.assign(action.draftId, action.conversationId) {
                        acceptsScreen(active.key, token)
                    }
                }
            }

            is AttachmentAction.Send -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    val conversation = checkNotNull(chat.currentConversation())
                    val current = { acceptsScreen(active.key, token) }
                    work.services.coordinator.send(action.draftId, conversation, current) {
                        chat.reconcileFeatureMessage(active, token, it)
                    }
                }
            }

            is AttachmentAction.RetryPublication -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    val conversation = checkNotNull(active.client.conversations.getById(action.conversationId))
                    work.services.coordinator.retryPublication(action.draftId, conversation, {
                        acceptsScreen(active.key, token)
                    }) {
                        chat.reconcileFeatureMessage(active, token, it)
                    }
                }
            }

            is AttachmentAction.Discard -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    work.services.coordinator.discard(action.draftId) {
                        acceptsScreen(active.key, token)
                    }
                }
            }

            is AttachmentAction.Download -> {
                perform { work ->
                    val active = work.active
                    val id = action.messageId
                    val token = work.token
                    val remote = readableRemote(active, action.messageId)
                    val filename = remote.filename ?: "File"
                    val starting = AttachmentCardState(action.messageId, filename, "Downloading", busy = true)
                    if (acceptsScreen(active.key, token)) downloads.update { it + (action.messageId to starting) }
                    try {
                        beforeDownload()
                        val downloaded = work.services.files.download(action.messageId, remote) { current(work) }
                        readableRemote(active, action.messageId)
                        val bitmap = work.services.files.preview(downloaded)
                        if (current(work) && work.services.files.isDownloaded(id)) {
                            val completeName = downloaded.filename ?: "File"
                            val complete =
                                AttachmentCardState(
                                    id,
                                    completeName,
                                    "Verified",
                                    canOpen = true,
                                    mimeType = downloaded.mimeType,
                                    verifiedSizeBytes =
                                        java.io
                                            .File(downloaded.path)
                                            .takeIf { it.isFile }
                                            ?.length(),
                                )
                            downloads.update { states ->
                                states.filter { (id, state) ->
                                    !state.canOpen || work.services.files.isDownloaded(id)
                                } +
                                    (action.messageId to complete)
                            }
                            if (bitmap != null) {
                                previews.update { images ->
                                    val updated = (images - action.messageId) + (action.messageId to bitmap)
                                    updated.entries
                                        .toList()
                                        .takeLast(3)
                                        .associate { it.key to it.value }
                                }
                            }
                            beforeDownloadRefresh()
                        } else {
                            bitmap?.recycle()
                            if (current(work)) {
                                downloads.update { states ->
                                    if (states[id] === starting) states - id else states
                                }
                            }
                        }
                    } catch (failure: CancellationException) {
                        if (current(work)) {
                            downloads.update { states ->
                                if (states[action.messageId] === starting) states - action.messageId else states
                            }
                        }
                        throw failure
                    } catch (failure: Throwable) {
                        val label = failure.attachmentLabel()
                        val name = remote.filename ?: "File"
                        val failed =
                            AttachmentCardState(
                                action.messageId,
                                name,
                                "Failed",
                                canDownload = true,
                                error = label,
                            )
                        val current = acceptsScreen(active.key, token)
                        if (current) downloads.update { it + (action.messageId to failed) }
                        throw failure
                    }
                }
            }

            is AttachmentAction.Open -> {
                val active = session.active.value ?: return
                val bound = services?.takeIf { it.key == active.key } ?: return
                val downloaded = bound.files.verifiedResult(action.messageId) ?: return
                val hostId = requests.host() ?: return
                val origin = identity(active, destinations.state.value.token)
                perform(origin) { work ->
                    readableRemote(work.active, action.messageId)
                    val export =
                        work.services.files.prepareOpen(downloaded) {
                            current(work) && requests.acceptsHost(hostId)
                        }
                    try {
                        readableRemote(work.active, action.messageId)
                        check(current(work) && requests.acceptsHost(hostId)) { "The screen changed" }
                        val request = AttachmentRequest.Open(identity(work.active, work.token), hostId, export)
                        if (!requests.offer(request)) {
                            export.close()
                        }
                    } catch (failure: Throwable) {
                        export.close()
                        throw failure
                    }
                }
            }

            is AttachmentAction.Save -> {
                val active = session.active.value ?: return
                val downloaded =
                    services?.takeIf { it.key == active.key }?.files?.verifiedResult(action.messageId) ?: return
                val token = destinations.state.value.token
                if (acceptsScreen(active.key, token)) {
                    requests.offer(AttachmentRequest.Save(identity(active, token), action.messageId, downloaded))
                }
            }
        }
    }

    private suspend fun readableRemote(
        active: ActiveSession,
        messageId: String,
    ): RemoteAttachment {
        check(session.accepts(active.key)) { "The session changed" }
        val found = active.client.conversations.getMessageById(messageId)
        val message = checkNotNull(found) { "Message is unavailable" }
        val content = (message.content as? SDKMessageContent.Standard)?.value
        return (content as? MessageContent.RemoteAttachment)?.v1 ?: error("File content is unavailable")
    }
}
