package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.graphics.asImageBitmap
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.*
import uniffi.xmtp_sdk.*

/** Connects Android pickers to session-owned work and shared cards. */
class AttachmentHost(
    private val activity: ComponentActivity,
    private val model: MessengerViewModel,
) {
    private val context = activity.applicationContext
    private val session = model.session
    private val drafts = MutableStateFlow<List<AttachmentCardState>>(emptyList())
    private val downloads = MutableStateFlow<Map<String, AttachmentCardState>>(emptyMap())
    private val previews = MutableStateFlow<Map<String, Bitmap>>(emptyMap())
    private val error = MutableStateFlow<String?>(null)

    @Volatile private var closed = false

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
    private val requests = ViewModelProvider(activity)[AttachmentRequests::class.java]
    private val previousAction = model.featureAction
    private val previousRefresh = model.featureRefresh
    internal var beforeDownloadRefresh: suspend () -> Unit = {}
    internal var beforeDownload: suspend () -> Unit = {}
    internal var beforeWatchRecovery: () -> Unit = {}
    internal var beforeWatchRefresh: suspend () -> Unit = {}
    internal var onWatchFinished: () -> Unit = {}
    internal var beforeSupportAdmission: suspend (Boolean, Long) -> Unit = { _, _ -> }
    private val pick =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            val request = requests.picker.also { requests.picker = null }
            if (uri != null && request != null && model.acceptsScreen(request.key, request.token)) {
                perform { work ->
                    val active = work.active
                    check(active.key == request.key)
                    work.services.coordinator.select(context.contentResolver, uri, request.value) {
                        !closed && model.acceptsScreen(request.key, request.token)
                    }
                }
            }
        }
    private val saveContract = ActivityResultContracts.CreateDocument("application/octet-stream")
    private val save =
        activity.registerForActivityResult(saveContract) { uri ->
            val request = requests.destination.also { requests.destination = null }
            if (uri != null && request != null && model.acceptsScreen(request.key, request.token)) {
                perform { work ->
                    val active = work.active
                    check(active.key == request.key)
                    val remote = readableRemote(active, request.value)
                    work.services.files.download(request.value, remote)
                    readableRemote(active, request.value)
                    check(!closed && model.acceptsScreen(request.key, request.token)) { "The screen changed" }
                    work.services.files.save(request.value, uri)
                }
            }
        }

    init {
        model.featureAction = { action ->
            when (action.name) {
                "select-file" -> {
                    val active = checkNotNull(session.active.value)
                    val token = model.screenToken()
                    val supported = uploadSupported(active)
                    withContext(Dispatchers.Main) {
                        if (closed || !model.acceptsScreen(active.key, token)) return@withContext
                        if (!supported) {
                            model.setAttachmentAvailability(active, token, false) { !closed }
                            return@withContext
                        }
                        val chat = checkNotNull(model.currentConversation())
                        requests.picker = AttachmentRequest(active.key, token, chat.id())
                        pick.launch(arrayOf("*/*"))
                    }
                }

                "open-file" -> {
                    act(AttachmentAction.Download(action.value))
                }

                else -> {
                    previousAction(action)
                }
            }
        }
        model.featureRefresh = { active, chat ->
            previousRefresh(active, chat)
            active.work.async { refresh(active) }.await()
        }
        activity.lifecycleScope.launch {
            session.active.collectLatest { active ->
                drafts.value = emptyList()
                downloads.value = emptyMap()
                previews.value = emptyMap()
                error.value = null
                requests.retain(active?.key)
                if (active != null) {
                    val watch =
                        active.work.launch {
                            try {
                                beforeWatchRefresh()
                                val bound = refresh(active, beforeRecovery = { beforeWatchRecovery() }) ?: return@launch
                                bound.coordinator.cards.collect {
                                    if (!closed && session.accepts(active.key)) drafts.value = it
                                }
                            } catch (failure: CancellationException) {
                                throw failure
                            } catch (failure: Throwable) {
                                if (!closed && session.accepts(active.key)) error.value = failure.attachmentLabel()
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
        activity.lifecycleScope.launch {
            var token = model.screenToken()
            model.state.collect {
                if (token != model.screenToken()) {
                    token = model.screenToken()
                    downloads.value = emptyMap()
                    previews.value = emptyMap()
                    error.value = null
                }
            }
        }
    }

    fun close() {
        closed = true
        model.featureAction = previousAction
        model.featureRefresh = previousRefresh
    }

    private suspend fun uploadSupported(active: ActiveSession): Boolean =
        active.client.serverConfiguration().attachments != null && active.client.attachments().offered()

    private fun current(work: Work) = !closed && model.acceptsScreen(work.active.key, work.token)

    private suspend fun refresh(
        active: ActiveSession,
        token: Long = model.screenToken(),
        beforeRecovery: () -> Unit = {},
    ): Services? =
        refreshMutex.withLock {
            if (closed || !session.accepts(active.key)) return@withLock null
            val bound =
                services?.takeIf { it.key == active.key } ?: Services(
                    active.key,
                    AttachmentDraftCoordinator(
                        active.key,
                        active.client,
                        active.paths,
                        session.preferences,
                        session.secrets,
                        model.sends,
                        session::accepts,
                        { change -> session.admit(active.key, change) },
                    ),
                    AttachmentFiles(context, active.key, active.client, session::accepts),
                ).also { services = it }
            try {
                if (!model.acceptsScreen(active.key, token)) return@withLock bound
                val supported = uploadSupported(active)
                beforeSupportAdmission(supported, token)
                model.setAttachmentAvailability(active, token, supported) { !closed }
                if (closed || !session.accepts(active.key)) return@withLock null
                if (!model.acceptsScreen(active.key, token)) return@withLock bound
                beforeRecovery()
                bound.coordinator.recover()
                session.admit(active.key) {
                    if (!closed && model.acceptsScreen(active.key, token)) drafts.value = bound.coordinator.cards.value
                }
                bound
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                if (closed || !session.accepts(active.key)) return@withLock null
                if (model.acceptsScreen(active.key, token)) error.value = failure.attachmentLabel()
                bound
            }
        }

    private fun perform(block: suspend (Work) -> Unit) {
        if (closed) return
        val active = session.active.value ?: return
        val token = model.screenToken()
        active.work.launch {
            var bound: Services? = null
            try {
                bound = refresh(active) ?: return@launch
                val work = Work(active, token, bound)
                check(current(work)) { "The screen changed" }
                block(work)
                if (current(work)) {
                    drafts.value = bound.coordinator.cards.value
                    error.value = null
                    model.dispatch(MessengerAction.Refresh)
                }
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                if (!closed && model.acceptsScreen(active.key, token)) {
                    bound?.let { drafts.value = it.coordinator.cards.value }
                    error.value = failure.attachmentLabel()
                }
            }
        }
    }

    private fun act(action: AttachmentAction) {
        when (action) {
            AttachmentAction.Select -> {
                model.dispatch(MessengerAction.Feature("select-file"))
            }

            is AttachmentAction.ViewChat -> {
                model.dispatch(MessengerAction.OpenConversation(action.conversationId))
            }

            is AttachmentAction.Assign -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    work.services.coordinator.assign(action.draftId, action.conversationId) {
                        !closed && model.acceptsScreen(active.key, token)
                    }
                }
            }

            is AttachmentAction.Send -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    val chat = checkNotNull(model.currentConversation())
                    val current = { !closed && model.acceptsScreen(active.key, token) }
                    work.services.coordinator.send(action.draftId, chat, current) {
                        model.reconcileFeatureMessage(active, token, it)
                    }
                }
            }

            is AttachmentAction.RetryPublication -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    val chat = checkNotNull(active.client.conversations.getById(action.conversationId))
                    work.services.coordinator.retryPublication(action.draftId, chat, {
                        !closed && model.acceptsScreen(active.key, token)
                    }) {
                        model.reconcileFeatureMessage(active, token, it)
                    }
                }
            }

            is AttachmentAction.Discard -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    work.services.coordinator.discard(action.draftId) {
                        !closed &&
                            model.acceptsScreen(active.key, token)
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
                    if (model.acceptsScreen(active.key, token)) downloads.update { it + (action.messageId to starting) }
                    try {
                        beforeDownload()
                        val downloaded = work.services.files.download(action.messageId, remote)
                        readableRemote(active, action.messageId)
                        val bitmap = work.services.files.preview(action.messageId)
                        if (model.acceptsScreen(active.key, token)) {
                            val completeName = downloaded.filename ?: "File"
                            val complete = AttachmentCardState(id, completeName, "Verified", canOpen = true)
                            downloads.update { it + (action.messageId to complete) }
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
                        val current = model.acceptsScreen(active.key, token)
                        if (current) downloads.update { it + (action.messageId to failed) }
                        throw failure
                    }
                }
            }

            is AttachmentAction.Open -> {
                perform { work ->
                    val active = work.active
                    val token = work.token
                    readableRemote(active, action.messageId)
                    val intent = work.services.files.openIntent(action.messageId)
                    readableRemote(active, action.messageId)
                    withContext(Dispatchers.Main) {
                        val current = !closed && model.acceptsScreen(active.key, token)
                        if (current) activity.startActivity(intent)
                    }
                }
            }

            is AttachmentAction.Save -> {
                val active = session.active.value ?: return
                requests.destination = AttachmentRequest(active.key, model.screenToken(), action.messageId)
                save.launch(services?.takeIf { it.key == active.key }?.files?.filename(action.messageId) ?: "File")
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

    @Composable fun Composer() {
        val current = drafts.collectAsState().value
        val chat =
            model.state
                .collectAsState()
                .value.conversationId
        AttachmentComposer(current, chat, error.collectAsState().value, ::act)
    }

    @Composable fun Message(row: MessageRow) {
        if (!row.attachment || row.deleted) return
        val cached = downloads.collectAsState().value[row.id]
        val state = cached ?: AttachmentCardState(row.id, row.text, "File", canDownload = true)
        val bitmap = previews.collectAsState().value[row.id]
        val conversationId =
            model.state
                .collectAsState()
                .value.conversationId
                .orEmpty()
        AttachmentMessage(
            state,
            conversationId,
            bitmap?.asImageBitmap(),
            ::act,
            showFilename = row.text.isBlank() || state.filename != row.text,
        )
    }

    @Composable fun Recovery() {
        val current = drafts.collectAsState().value
        val conversations =
            model.state
                .collectAsState()
                .value.conversations
        val problem = error.collectAsState().value
        AttachmentRecovery(current, conversations, problem, ::act)
    }
}
