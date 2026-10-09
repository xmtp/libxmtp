package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.*
import uniffi.xmtp_sdk.*

/** Connects Android pickers to session-owned work and shared cards. */
class AttachmentHost(private val activity: ComponentActivity, private val model: MessengerViewModel) {
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
    private val pick = activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        val request = pickerRequest.also { pickerRequest = null }
        if (uri != null && request != null && model.acceptsScreen(request.first, request.second)) perform { active ->
            check(active.key == request.first)
            coordinator!!.select(context.contentResolver, uri, request.third) { !closed && model.acceptsScreen(request.first, request.second) }
        }
    }
    private val save = activity.registerForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val request = saveRequest.also { saveRequest = null }
        if (uri != null && request != null && model.acceptsScreen(request.first, request.second)) perform { active ->
            check(active.key == request.first)
            readableRemote(active, request.third)
            files!!.save(request.third, uri)
        }
    }

    init {
        model.featureAction = { action ->
            when (action.name) {
                "select-file" -> withContext(Dispatchers.Main) {
                    val active = checkNotNull(session.active.value)
                    val chat = checkNotNull(model.currentConversation())
                    pickerRequest = Triple(active.key, model.screenToken(), chat.id())
                    pick.launch(arrayOf("*/*"))
                }
                "open-file" -> act(AttachmentAction.Download(action.value))
                else -> previousAction(action)
            }
        }
        model.featureRefresh = { active, chat ->
            previousRefresh(active, chat)
            active.work.async { refresh(active) }.await()
        }
        session.beforeEnd = { active ->
            previousEnd(active)
            AttachmentFiles.revokeProfile(context, active.key.profileId)
            check(!active.paths.exports.exists() || active.paths.exports.deleteRecursively()) { "Cannot clear file exports" }
        }
        activity.lifecycleScope.launch {
            session.active.collectLatest { active ->
                drafts.value = emptyList(); downloads.value = emptyMap(); previews.value = emptyMap(); error.value = null
                if (active != null) {
                    val watch = active.work.launch {
                        refresh(active)
                        coordinator!!.cards.collect { if (session.accepts(active.key)) drafts.value = it }
                    }
                    try { watch.join() } finally { watch.cancelAndJoin() }
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

    private suspend fun refresh(active: ActiveSession) = refreshMutex.withLock {
        if (!session.accepts(active.key)) return@withLock
        if (owner?.key != active.key) {
            owner = active
            coordinator = AttachmentDraftCoordinator(active.key, active.client, active.paths, session.preferences, session.secrets, model.sends, session::accepts)
            files = AttachmentFiles(context, active.key, active.client, session::accepts)
        }
        coordinator!!.recover()
        if (session.accepts(active.key)) {
            drafts.value = coordinator!!.cards.value
            model.setFeatures(model.state.value.features.copy(attachments = true))
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
                if (model.acceptsScreen(active.key, token)) { drafts.value = coordinator!!.cards.value; error.value = null; model.dispatch(MessengerAction.Refresh) }
            } catch (failure: CancellationException) { throw failure }
            catch (failure: Throwable) { if (model.acceptsScreen(active.key, token)) { drafts.value = coordinator!!.cards.value; error.value = failure.attachmentLabel() } }
        }
    }

    private fun act(action: AttachmentAction) {
        when (action) {
            AttachmentAction.Select -> model.dispatch(MessengerAction.Feature("select-file"))
            is AttachmentAction.ViewChat -> model.dispatch(MessengerAction.OpenConversation(action.conversationId))
            is AttachmentAction.Assign -> perform { coordinator!!.assign(action.draftId, action.conversationId) }
            is AttachmentAction.Send -> perform { active ->
                val token = model.screenToken()
                val chat = checkNotNull(model.currentConversation())
                coordinator!!.send(action.draftId, chat, { !closed && model.acceptsScreen(active.key, token) }) { model.reconcileFeatureMessage(active, token, it) }
            }
            is AttachmentAction.Discard -> perform { coordinator!!.discard(action.draftId) }
            is AttachmentAction.Download -> perform { active ->
                val token = model.screenToken()
                val remote = readableRemote(active, action.messageId)
                if (model.acceptsScreen(active.key, token)) downloads.update { it + (action.messageId to AttachmentCardState(action.messageId, remote.filename ?: "File", "Downloading", busy = true)) }
                try {
                    val downloaded = files!!.download(action.messageId, remote)
                    readableRemote(active, action.messageId)
                    val bitmap = files!!.preview(action.messageId)
                    if (model.acceptsScreen(active.key, token)) {
                        downloads.update { it + (action.messageId to AttachmentCardState(action.messageId, downloaded.filename ?: "File", "Verified", canOpen = true)) }
                        if (bitmap != null) previews.update { images -> ((images - action.messageId) + (action.messageId to bitmap)).entries.toList().takeLast(3).associate { it.key to it.value } }
                    } else bitmap?.recycle()
                } catch (failure: Throwable) {
                    if (model.acceptsScreen(active.key, token)) downloads.update { it + (action.messageId to AttachmentCardState(action.messageId, remote.filename ?: "File", "Failed", canDownload = true, error = failure.attachmentLabel())) }
                    throw failure
                }
            }
            is AttachmentAction.Open -> perform { active ->
                val token = model.screenToken()
                readableRemote(active, action.messageId)
                val intent = files!!.openIntent(action.messageId)
                readableRemote(active, action.messageId)
                withContext(Dispatchers.Main) { if (!closed && model.acceptsScreen(active.key, token)) activity.startActivity(intent) }
            }
            is AttachmentAction.Save -> {
                val active = session.active.value ?: return
                saveRequest = Triple(active.key, model.screenToken(), action.messageId)
                save.launch(files!!.filename(action.messageId))
            }
        }
    }

    private suspend fun readableRemote(active: ActiveSession, messageId: String): RemoteAttachment {
        check(session.accepts(active.key)) { "The session changed" }
        val message = checkNotNull(active.client.conversations.getMessageById(messageId)) { "Message is unavailable" }
        return ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.RemoteAttachment)?.v1 ?: error("File content is unavailable")
    }

    @Composable fun Composer() {
        val current = drafts.collectAsState().value
        val chat = model.state.collectAsState().value.conversationId
        Column { error.collectAsState().value?.let { Text(it) }; current.filter { it.conversationId == chat }.forEach { card -> AttachmentCard(card, card.conversationId, ::act) } }
    }

    @Composable fun Message(row: MessageRow) {
        if (!row.attachment || row.deleted) return
        val state = downloads.collectAsState().value[row.id] ?: AttachmentCardState(row.id, row.text, "File", canDownload = true)
        val bitmap = previews.collectAsState().value[row.id]
        Column {
            if (bitmap != null) Image(bitmap.asImageBitmap(), row.text, Modifier.fillMaxWidth().heightIn(max = 240.dp))
            AttachmentCard(state, model.state.value.conversationId.orEmpty(), ::act)
        }
    }

    @Composable fun Recovery() {
        val current = drafts.collectAsState().value
        val conversations = model.state.collectAsState().value.conversations
        val problem = error.collectAsState().value
        LazyColumn {
            item { Text("Upload drafts are retained for 24 hours by default. An interrupted send needs review."); problem?.let { Text(it) } }
            items(current, key = { it.id }) { card ->
                AttachmentCard(card.copy(canSend = false), card.conversationId, ::act)
                if (card.conversationId.isEmpty()) conversations.forEach { chat ->
                    TextButton({ act(AttachmentAction.Assign(card.id, chat.id)) }, Modifier.heightIn(min = 48.dp)) { Text("Assign to ${chat.title}") }
                } else if (!card.unknownOutcome && card.acceptedMessageId == null) TextButton({ act(AttachmentAction.ViewChat(card.conversationId)) }, Modifier.heightIn(min = 48.dp)) { Text("View chat") }
            }
        }
    }
}
