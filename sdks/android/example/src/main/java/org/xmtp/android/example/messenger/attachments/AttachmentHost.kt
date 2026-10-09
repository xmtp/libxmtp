package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.graphics.asImageBitmap
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
    private var owner: ActiveSession? = null
    private var coordinator: AttachmentDraftCoordinator? = null
    private var files: AttachmentFiles? = null
    private val refreshMutex = Mutex()
    private var pickerRequest: Triple<SessionKey, Long, String>? = null
    private var saveRequest: Triple<SessionKey, Long, String>? = null
    private val previousAction = model.featureAction
    private val previousRefresh = model.featureRefresh
    private val previousEnd = session.beforeEnd
    internal var beforeDownloadRefresh: suspend () -> Unit = {}
    private val pick =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            val request = pickerRequest.also { pickerRequest = null }
            if (uri != null && request != null && model.acceptsScreen(request.first, request.second)) {
                perform { active ->
                    check(active.key == request.first)
                    coordinator!!.select(context.contentResolver, uri, request.third) {
                        !closed && model.acceptsScreen(request.first, request.second)
                    }
                }
            }
        }
    private val saveContract = ActivityResultContracts.CreateDocument("application/octet-stream")
    private val save =
        activity.registerForActivityResult(saveContract) { uri ->
            val request = saveRequest.also { saveRequest = null }
            if (uri != null && request != null && model.acceptsScreen(request.first, request.second)) {
                perform { active ->
                    check(active.key == request.first)
                    readableRemote(active, request.third)
                    files!!.save(request.third, uri)
                }
            }
        }

    init {
        model.featureAction = { action ->
            when (action.name) {
                "select-file" -> {
                    withContext(Dispatchers.Main) {
                        val active = checkNotNull(session.active.value)
                        val chat = checkNotNull(model.currentConversation())
                        pickerRequest = Triple(active.key, model.screenToken(), chat.id())
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
        session.beforeEnd = { active ->
            previousEnd(active)
            AttachmentFiles.revokeProfile(context, active.key.profileId)
            val exports = active.paths.exports
            check(!exports.exists() || exports.deleteRecursively()) { "Cannot clear file exports" }
        }
        activity.lifecycleScope.launch {
            session.active.collectLatest { active ->
                drafts.value = emptyList()
                downloads.value = emptyMap()
                previews.value = emptyMap()
                error.value = null
                if (active != null) {
                    val watch =
                        active.work.launch {
                            refresh(active)
                            coordinator!!.cards.collect { if (session.accepts(active.key)) drafts.value = it }
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
        session.beforeEnd = previousEnd
    }

    private suspend fun refresh(active: ActiveSession) =
        refreshMutex.withLock {
            if (!session.accepts(active.key)) return@withLock
            if (owner?.key != active.key) {
                owner = active
                coordinator =
                    AttachmentDraftCoordinator(
                        active.key,
                        active.client,
                        active.paths,
                        session.preferences,
                        session.secrets,
                        model.sends,
                        session::accepts,
                        { change -> session.admit(active.key, change) },
                    )
                files = AttachmentFiles(context, active.key, active.client, session::accepts)
            }
            coordinator!!.recover()
            if (session.accepts(active.key)) {
                drafts.value = coordinator!!.cards.value
                model.setFeatures(
                    model.state.value.features
                        .copy(attachments = true),
                )
            }
        }

    private fun perform(block: suspend (ActiveSession) -> Unit) {
        if (closed) return
        val active = session.active.value ?: return
        val token = model.screenToken()
        active.work.launch {
            try {
                refresh(active)
                check(!closed && model.acceptsScreen(active.key, token)) { "The screen changed" }
                block(active)
                if (model.acceptsScreen(active.key, token)) {
                    drafts.value = coordinator!!.cards.value
                    error.value = null
                    model.dispatch(MessengerAction.Refresh)
                }
            } catch (failure: CancellationException) {
                throw failure
            } catch (failure: Throwable) {
                if (model.acceptsScreen(active.key, token)) {
                    drafts.value = coordinator!!.cards.value
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
                perform { active ->
                    val token = model.screenToken()
                    coordinator!!.assign(action.draftId, action.conversationId) {
                        !closed && model.acceptsScreen(active.key, token)
                    }
                }
            }

            is AttachmentAction.Send -> {
                perform { active ->
                    val token = model.screenToken()
                    val chat = checkNotNull(model.currentConversation())
                    val current = { !closed && model.acceptsScreen(active.key, token) }
                    coordinator!!.send(action.draftId, chat, current) {
                        model.reconcileFeatureMessage(active, token, it)
                    }
                }
            }

            is AttachmentAction.Discard -> {
                perform { active ->
                    val token = model.screenToken()
                    coordinator!!.discard(action.draftId) { !closed && model.acceptsScreen(active.key, token) }
                }
            }

            is AttachmentAction.Download -> {
                perform { active ->
                    val id = action.messageId
                    val token = model.screenToken()
                    val remote = readableRemote(active, action.messageId)
                    val filename = remote.filename ?: "File"
                    val starting = AttachmentCardState(action.messageId, filename, "Downloading", busy = true)
                    if (model.acceptsScreen(active.key, token)) downloads.update { it + (action.messageId to starting) }
                    try {
                        val downloaded = files!!.download(action.messageId, remote)
                        readableRemote(active, action.messageId)
                        val bitmap = files!!.preview(action.messageId)
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
                perform { active ->
                    val token = model.screenToken()
                    readableRemote(active, action.messageId)
                    val intent = files!!.openIntent(action.messageId)
                    readableRemote(active, action.messageId)
                    withContext(Dispatchers.Main) {
                        val current = !closed && model.acceptsScreen(active.key, token)
                        if (current) activity.startActivity(intent)
                    }
                }
            }

            is AttachmentAction.Save -> {
                val active = session.active.value ?: return
                saveRequest = Triple(active.key, model.screenToken(), action.messageId)
                save.launch(files!!.filename(action.messageId))
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
