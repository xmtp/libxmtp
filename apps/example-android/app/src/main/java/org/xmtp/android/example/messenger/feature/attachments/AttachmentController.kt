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
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceState
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
    private val choiceUi = MutableStateFlow<AttachmentChoiceState?>(null)
    val choice = choiceUi.asStateFlow()
    private var choiceOrigin: EffectIdentity? = null

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
    private val downloadAdmission = Any()
    private val visiblePreviews = mutableMapOf<Pair<Long, String>, Int>()
    private val refreshMutex = Mutex()
    internal var beforeDisplayAdmission: suspend (String) -> Unit = {}
    internal var afterDisplayAdmission: (String) -> Unit = {}
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
                clearInvalidChoice()
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
                    clearInvalidChoice()
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

    private fun showChoiceSheet() {
        val conversationId = chat.currentConversation()?.id() ?: return
        perform { work ->
            if (!uploadSupported(work.active)) return@perform
            session.withCurrent(work.active.key) {
                destinations.admit(DestinationOrigin(work.active.key, work.token)) {
                    if (chat.currentConversation()?.id() != conversationId) return@admit
                    val origin = identity(work.active, work.token)
                    session.withStateLock {
                        choiceOrigin = origin
                        choiceUi.value = AttachmentChoiceState(origin.requestId, conversationId)
                    }
                }
            }
        }
    }

    private fun choose(action: AttachmentAction.Choose) {
        val selected =
            session.withStateLock {
                val displayed = choiceUi.value ?: return@withStateLock null
                val origin = choiceOrigin ?: return@withStateLock null
                if (displayed.instanceId != action.instanceId) return@withStateLock null
                choiceUi.value = null
                choiceOrigin = null
                displayed to origin
            } ?: return
        perform(selected.second) { work ->
            if (!uploadSupported(work.active)) return@perform
            if (chat.currentConversation()?.id() != selected.first.conversationId) return@perform
            if (current(work)) {
                requests.offer(
                    AttachmentRequest.Select(selected.second, selected.first.conversationId, action.selection),
                )
            }
        }
    }

    fun dismissChoiceSheet(instanceId: Long? = null): Boolean =
        session.withStateLock {
            val displayed = choiceUi.value ?: return@withStateLock false
            if (instanceId != null && displayed.instanceId != instanceId) return@withStateLock false
            choiceUi.value = null
            choiceOrigin = null
            true
        }

    private fun clearInvalidChoice() {
        session.withStateLock {
            val origin = choiceOrigin
            if (origin != null && accepts(origin)) return@withStateLock
            choiceUi.value = null
            choiceOrigin = null
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
                showChoiceSheet()
            }

            is AttachmentAction.Choose -> {
                choose(action)
            }

            is AttachmentAction.DismissChoice -> {
                dismissChoiceSheet(action.instanceId)
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
                    download(work, action.messageId, readableRemote(work.active, action.messageId))
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

    internal fun visiblePreviewIds(): Set<String> =
        synchronized(visiblePreviews) {
            visiblePreviews.keys
                .filter { it.first == destinations.state.value.token }
                .map { it.second }
                .toSet()
        }

    /** Restore SDK-owned files when a row returns, and fetch our own attachments automatically. */
    fun display(
        messageId: String,
        mine: Boolean,
    ): () -> Unit {
        val token = destinations.state.value.token
        val visible = token to messageId
        synchronized(visiblePreviews) {
            visiblePreviews[visible] = (visiblePreviews[visible] ?: 0) + 1
        }
        perform { work ->
            val observed = synchronized(downloadAdmission) { downloads.value[messageId] }
            if (observed?.busy == true) return@perform
            val remote = readableRemote(work.active, messageId)
            val localPath =
                work.active.client
                    .attachments()
                    .localPath(remote)
            val local = withContext(Dispatchers.IO) { java.io.File(localPath).isFile }
            beforeDisplayAdmission(messageId)
            if (!current(work)) return@perform
            if (mine || local) {
                download(work, messageId, remote, restoring = true, observed = observed, localOnly = !mine)
            } else {
                changeDownload(work, messageId, observed) {
                    work.services.files.forget(messageId)
                    downloads.update { it - messageId }
                    previews.update { it - messageId }
                }
            }
            afterDisplayAdmission(messageId)
        }
        return {
            synchronized(visiblePreviews) {
                val remaining = (visiblePreviews[visible] ?: 1) - 1
                if (remaining > 0) {
                    visiblePreviews[visible] = remaining
                } else {
                    visiblePreviews.remove(visible)
                    if (destinations.state.value.token == token) previews.update { it - messageId }
                }
            }
        }
    }

    private suspend fun download(
        work: Work,
        messageId: String,
        remote: RemoteAttachment,
        restoring: Boolean = false,
        observed: AttachmentCardState? = null,
        localOnly: Boolean = false,
    ) {
        val active = work.active
        val id = messageId
        val token = work.token
        val filename = remote.filename ?: "File"
        val starting =
            AttachmentCardState(messageId, filename, if (localOnly) "Loading" else "Downloading", busy = true)
        val admitted =
            synchronized(downloadAdmission) {
                if (!current(work) || (restoring && downloads.value[messageId] !== observed)) {
                    false
                } else {
                    downloads.update { it + (messageId to starting) }
                    true
                }
            }
        if (!admitted) return
        try {
            beforeDownload()
            val downloaded =
                if (localOnly) {
                    work.services.files.restore(messageId, remote) { current(work) }
                        ?: run {
                            changeDownload(work, messageId, starting) {
                                work.services.files.forget(messageId)
                                downloads.update { it - messageId }
                                previews.update { it - messageId }
                            }
                            return
                        }
                } else {
                    work.services.files.download(messageId, remote) { current(work) }
                }
            readableRemote(active, messageId)
            val bitmap =
                work.services.files.preview(downloaded) {
                    current(work) && synchronized(visiblePreviews) { (visiblePreviews[token to id] ?: 0) > 0 }
                }
            val dimensions = if (bitmap != null) work.services.files.imageDimensions(downloaded) else null
            if (current(work) && work.services.files.isDownloaded(id)) {
                val completeName = downloaded.filename ?: "File"
                val complete =
                    AttachmentCardState(
                        id,
                        completeName,
                        "Verified",
                        canOpen = true,
                        mimeType = downloaded.mimeType,
                        imageWidth = dimensions?.first,
                        imageHeight = dimensions?.second,
                        verifiedSizeBytes =
                            java.io
                                .File(downloaded.path)
                                .takeIf { it.isFile }
                                ?.length(),
                    )
                val committed =
                    changeDownload(work, messageId, starting) {
                        downloads.update { states -> states + (messageId to complete) }
                    }
                if (bitmap != null) {
                    synchronized(visiblePreviews) {
                        if (committed && current(work) && (visiblePreviews[token to messageId] ?: 0) > 0) {
                            previews.update { images -> images + (messageId to bitmap) }
                        } else {
                            bitmap.recycle()
                        }
                    }
                }
                beforeDownloadRefresh()
            } else {
                bitmap?.recycle()
                changeDownload(work, messageId, starting) { downloads.update { it - messageId } }
            }
        } catch (failure: CancellationException) {
            changeDownload(work, messageId, starting) { downloads.update { it - messageId } }
            throw failure
        } catch (failure: Throwable) {
            val label = failure.attachmentLabel()
            val name = remote.filename ?: "File"
            val failed =
                AttachmentCardState(
                    messageId,
                    name,
                    "Failed",
                    canDownload = true,
                    error = label,
                )
            if (changeDownload(work, messageId, starting) { downloads.update { it + (messageId to failed) } }) {
                throw failure
            }
        }
    }

    /** A delayed local probe must not replace a manual transfer that started after it. */
    private fun changeDownload(
        work: Work,
        messageId: String,
        observed: AttachmentCardState?,
        change: () -> Unit,
    ): Boolean =
        synchronized(downloadAdmission) {
            if (!current(work) || downloads.value[messageId] !== observed) {
                false
            } else {
                change()
                true
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
