package org.xmtp.android.example.messenger.attachments

import android.content.ContentResolver
import android.net.Uri
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.attachments.AttachmentCardState
import uniffi.xmtp_sdk.*
import java.io.File
import java.util.UUID

internal fun draftNeedsReview(draft: SendDraftRef) =
    draft.acceptedMessageId == null && draft.phase == SendPhase.QUEUEING

/** Owns one profile's draft actions. It never resumes a typed send on recovery. */
class AttachmentDraftCoordinator(
    private val key: SessionKey,
    private val client: SDKClient,
    private val paths: ProfilePaths,
    private val preferences: MessengerPreferences,
    private val secrets: SecureSecretStore,
    private val sends: SendCoordinator,
    private val accepts: (SessionKey) -> Boolean,
    private val admit: (() -> Unit) -> Boolean = { change ->
        if (accepts(key)) {
            change()
            true
        } else {
            false
        }
    },
) {
    private val attachments = client.attachments()
    private val mutex = Mutex()
    private var swept = false
    private val mutableCards = MutableStateFlow<List<AttachmentCardState>>(emptyList())
    val cards: StateFlow<List<AttachmentCardState>> = mutableCards
    internal var afterRecoverySnapshot: suspend () -> Unit = {}
    internal var afterDiscardDescriptorRead: suspend () -> Unit = {}
    internal var afterDiscardDelete: suspend () -> Unit = {}
    internal var beforeUploadPhaseSave: suspend () -> Unit = {}
    internal var afterAssignSnapshot: suspend () -> Unit = {}

    private fun checkCurrent() = check(accepts(key)) { "The session changed" }

    private fun admitChange(
        screenCurrent: () -> Boolean = { true },
        change: () -> Unit,
    ): Boolean {
        var changed = false
        val accepted =
            admit {
                if (accepts(key) && screenCurrent()) {
                    change()
                    changed = true
                }
            }
        return accepted && changed
    }

    private suspend fun save(
        draft: SendDraftRef,
        screenCurrent: () -> Boolean = { true },
    ): Boolean = preferences.saveDraft(key.profileId, draft) { change -> admitChange(screenCurrent, change) }

    private suspend fun writeDescriptor(
        ref: String,
        remote: RemoteAttachment,
        screenCurrent: () -> Boolean = { true },
    ) = withContext(Dispatchers.IO) {
        val bytes = AttachmentDescriptor.encode(remote)
        check(
            admitChange(screenCurrent) { secrets.write(key.profileId, ref, bytes) },
        ) { "The session or screen changed" }
    }

    private fun operationId(draftId: String) = "${key.profileId}/$draftId"

    private fun update(card: AttachmentCardState) {
        discardState.admitCard(operationId(card.id)) {
            admitChange { mutableCards.update { cards -> cards.filterNot { it.id == card.id } + card } }
        }
    }

    private suspend fun descriptor(draft: SendDraftRef): RemoteAttachment =
        withContext(Dispatchers.IO) {
            val ref = checkNotNull(draft.descriptorSecretRef)
            val bytes = checkNotNull(secrets.read(key.profileId, ref)) { "Draft descriptor is unavailable" }
            AttachmentDescriptor.decode(bytes)
        }

    private fun card(
        draft: SendDraftRef,
        remote: RemoteAttachment?,
        status: String,
        busy: Boolean = false,
        unavailable: Boolean = false,
        error: String? = null,
    ) = AttachmentCardState(
        id = draft.draftId,
        filename = remote?.filename ?: "File",
        status = status,
        busy = busy,
        canSend = !busy && !unavailable && !draftNeedsReview(draft) && draft.acceptedMessageId == null,
        canDiscard = !busy || draft.phase == SendPhase.UPLOADING,
        unknownOutcome = draftNeedsReview(draft),
        unavailable = unavailable,
        error = error,
        conversationId = draft.conversationKey,
        acceptedMessageId = draft.acceptedMessageId,
    )

    private suspend fun sweepOnce() =
        mutex.withLock {
            if (!swept) {
                withContext(Dispatchers.IO) { PrivateFileStager.sweep(paths.temp) }
                swept = true
            }
        }

    suspend fun select(
        resolver: ContentResolver,
        uri: Uri,
        conversationKey: String,
        screenCurrent: () -> Boolean = {
            true
        },
    ): SendDraftRef {
        checkCurrent()
        check(screenCurrent()) { "The screen changed" }
        sweepOnce()
        check(attachments.offered()) { "This backend does not offer file uploads" }
        val configuration =
            checkNotNull(client.serverConfiguration().attachments) { "This backend does not offer file uploads" }
        val source = PrivateFileStager.stage(resolver, uri, paths.temp, configuration.maxUploadBytes)
        var remote: RemoteAttachment? = null
        var saved = false
        var registered = false
        val id = UUID.randomUUID().toString()
        val ref = "attachment-$id"
        try {
            processMutex.withLock {
                activeSecrets.add("${key.profileId}/$ref")
                creating[key.profileId] = (creating[key.profileId] ?: 0) + 1
                registered = true
            }
            checkCurrent()
            check(screenCurrent()) { "The screen changed" }
            val stagedRemote =
                attachments
                    .create(
                        AttachmentSource.Path(source.file.absolutePath, source.filename, source.mimeType),
                    ).remoteAttachment()
            remote = stagedRemote
            checkCurrent()
            check(screenCurrent()) { "The screen changed" }
            writeDescriptor(ref, stagedRemote, screenCurrent)
            checkCurrent()
            check(screenCurrent()) { "The screen changed" }
            val draft = SendDraftRef(id, conversationKey, ref)
            saved = save(draft, screenCurrent)
            check(saved) { "The session or screen changed" }
            checkCurrent()
            update(card(draft, remote, "Waiting"))
            return draft
        } finally {
            withContext(NonCancellable + Dispatchers.IO) {
                try {
                    PrivateFileStager.release(source.file)
                    if (!saved) {
                        remote?.let { attachments.deleteLocal(it) }
                        secrets.delete(key.profileId, ref)
                    }
                } finally {
                    processMutex.withLock {
                        if (registered) {
                            activeSecrets.remove("${key.profileId}/$ref")
                            creating[key.profileId] = ((creating[key.profileId] ?: 1) - 1).coerceAtLeast(0)
                        }
                    }
                }
            }
        }
    }

    suspend fun send(
        draftId: String,
        conversation: Conversation,
        screenCurrent: () -> Boolean = {
            true
        },
        reconcile: suspend (Message) -> Unit,
    ) = sendDraft(draftId, conversation, screenCurrent, false, reconcile)

    suspend fun retryPublication(
        draftId: String,
        conversation: Conversation,
        screenCurrent: () -> Boolean = { true },
        reconcile: suspend (Message) -> Unit,
    ) = sendDraft(draftId, conversation, screenCurrent, true, reconcile)

    private suspend fun sendDraft(
        draftId: String,
        conversation: Conversation,
        screenCurrent: () -> Boolean,
        publicationOnly: Boolean,
        reconcile: suspend (Message) -> Unit,
    ) {
        checkCurrent()
        check(screenCurrent()) { "The screen changed" }
        val admission = { accepts(key) && screenCurrent() }
        processMutex.withLock {
            check(!discardState.isDiscarded(operationId(draftId))) { "The draft was discarded" }
            check(running.add(operationId(draftId))) { "This file action is already running" }
            discardState.retainSnapshots(listOf(operationId(draftId)))
        }
        var draft = SendDraftRef(draftId, "")
        var remote: RemoteAttachment? = null
        try {
            draft = preferences.drafts(key.profileId).single { it.draftId == draftId }
            require(!draftNeedsReview(draft)) { "Review the unknown send outcome in the chat" }
            require(draft.conversationKey == conversation.id()) { "The draft belongs to another chat" }
            require(!publicationOnly || draft.acceptedMessageId != null) { "The draft has no accepted message" }
            draft.acceptedMessageId?.let { id ->
                sends.retry(key, client, conversation, id, admission = admission, reconcile = reconcile)
                clearPublishedSecret(draft)
                return
            }
            val selected = descriptor(draft)
            remote = selected
            val pending = attachments.pending(selected)
            draft = draft.copy(phase = SendPhase.UPLOADING)
            beforeUploadPhaseSave()
            checkCurrent()
            processMutex.withLock {
                check(!discardState.isDiscarded(operationId(draftId))) { "The draft was discarded" }
                check(save(draft, screenCurrent)) { "The session or screen changed" }
            }
            update(card(draft, remote, "Uploading", busy = true))
            pending.upload()
            // A cancelled waiter does not stop native transfer. Discard owns deletion.
            checkCurrent()
            check(screenCurrent()) { "The screen changed" }
            processMutex.withLock {
                check(
                    !discardState.isDiscarded(operationId(draftId)),
                ) { "The draft was discarded" }
            }
            check(pending.status() == PendingAttachmentStatus.Complete) { "Upload has not completed" }
            update(card(draft, remote, "Complete", busy = true))
            // The persisted QUEUEING phase precedes the typed send in SendCoordinator.
            processMutex.withLock {
                check(!discardState.isDiscarded(operationId(draftId))) { "The draft was discarded" }
                queueing.add(operationId(draftId))
            }
            sends.queue(key, client, conversation, draft, admission = admission, reconcile = reconcile) {
                checkCurrent()
                check(screenCurrent()) { "The screen changed" }
                processMutex.withLock {
                    check(
                        !discardState.isDiscarded(operationId(draftId)),
                    ) { "The draft was discarded" }
                }
                conversation.sendRemoteAttachment(selected, SendOptions(optimistic = true))
            }
            clearPublishedSecret(draft)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            val current = preferences.drafts(key.profileId).firstOrNull { it.draftId == draftId }
            if (current !=
                null
            ) {
                update(
                    card(
                        current,
                        remote,
                        if (draftNeedsReview(current)) "Review send outcome" else "Failed",
                        unavailable = error.isExpiredDraft(),
                        error = error.attachmentLabel(),
                    ),
                )
            }
            throw error
        } finally {
            withContext(NonCancellable) {
                processMutex.withLock {
                    running.remove(operationId(draftId))
                    queueing.remove(operationId(draftId))
                    discardState.releaseSnapshots(listOf(operationId(draftId)))
                }
            }
        }
    }

    private suspend fun clearPublishedSecret(draft: SendDraftRef) {
        checkCurrent()
        if (preferences.drafts(key.profileId).none { it.draftId == draft.draftId }) {
            withContext(Dispatchers.IO) {
                admitChange { draft.descriptorSecretRef?.let { secrets.delete(key.profileId, it) } }
            }
            admitChange { mutableCards.update { cards -> cards.filterNot { it.id == draft.draftId } } }
        }
    }

    private suspend fun <T> withDraftSnapshots(
        read: suspend () -> List<SendDraftRef>,
        action: suspend (List<SendDraftRef>) -> T,
    ): T {
        var ids = emptyList<String>()
        try {
            val drafts =
                processMutex.withLock {
                    val latest = read()
                    ids = latest.map { operationId(it.draftId) }
                    discardState.retainSnapshots(ids)
                    latest
                }
            return action(drafts)
        } finally {
            withContext(NonCancellable) {
                processMutex.withLock { discardState.releaseSnapshots(ids) }
            }
        }
    }

    internal fun retainedDiscardMarkers() = discardState.retainedForProfile(key.profileId)

    suspend fun recover() =
        recoveryMutex.withLock {
            checkCurrent()
            sweepOnce()
            admitChange { mutableCards.value = emptyList() }
            withDraftSnapshots(
                { preferences.drafts(key.profileId).filter { it.descriptorSecretRef != null } },
            ) { drafts ->
                afterRecoverySnapshot()
                processMutex.withLock {
                    val retained = preferences.drafts(key.profileId).mapNotNull { it.descriptorSecretRef }.toSet()
                    withContext(Dispatchers.IO) {
                        paths.secrets
                            .listFiles()
                            ?.filter {
                                it.name.matches(Regex("attachment-[a-zA-Z0-9-]+")) &&
                                    it.name !in retained &&
                                    "${key.profileId}/${it.name}" !in activeSecrets
                            }?.forEach { file -> admitChange { secrets.delete(key.profileId, file.name) } }
                    }
                }
                val known = mutableListOf<RemoteAttachment>()
                for (draft in drafts) {
                    checkCurrent()
                    var remote: RemoteAttachment? = null
                    try {
                        if (draft.acceptedMessageId != null) {
                            update(card(draft, null, "Message accepted. Retry publication in the chat."))
                            continue
                        }
                        remote = descriptor(draft)
                        known += remote
                        if (draftNeedsReview(draft)) {
                            update(card(draft, remote, "Review send outcome"))
                        } else {
                            val status = attachments.pending(remote).status()
                            checkCurrent()
                            update(
                                card(
                                    draft,
                                    remote,
                                    status.label(),
                                    busy =
                                        status == PendingAttachmentStatus.Uploading,
                                ),
                            )
                        }
                    } catch (
                        error: CancellationException,
                    ) {
                        throw error
                    } catch (
                        error: Throwable,
                    ) {
                        update(
                            card(
                                draft,
                                remote,
                                "Draft expired or unavailable",
                                unavailable = true,
                                error = error.attachmentLabel(),
                            ),
                        )
                    }
                }
                // The unfinished list is for discovery only. Complete drafts use pending(remote).
                for (pending in attachments.listPending()) {
                    val remote = pending.remoteAttachment()
                    processMutex.withLock {
                        if (known.any { it == remote } || (creating[key.profileId] ?: 0) != 0) return@withLock
                        // Selection registers before SDK creation. Read its latest saved ownership under this metadata lock.
                        val latest =
                            preferences.drafts(key.profileId).filter {
                                it.descriptorSecretRef != null && it.acceptedMessageId == null
                            }
                        var unreadableOwner = false
                        for (candidate in latest) {
                            val candidateRemote =
                                try {
                                    descriptor(candidate)
                                } catch (error: CancellationException) {
                                    throw error
                                } catch (_: Throwable) {
                                    unreadableOwner = true
                                    null
                                }
                            if (candidateRemote == remote) return@withLock
                        }
                        // A damaged reference can still own this SDK record. Do not create a second owner.
                        if (unreadableOwner) return@withLock
                        val id = UUID.randomUUID().toString()
                        val ref = "attachment-$id"
                        checkCurrent()
                        activeSecrets.add("${key.profileId}/$ref")
                        try {
                            writeDescriptor(ref, remote)
                            checkCurrent()
                            val draft = SendDraftRef(id, "", ref)
                            check(save(draft)) { "The session changed" }
                            update(
                                card(draft, remote, "Unassigned file. Discard or select a chat.").copy(canSend = false),
                            )
                        } finally {
                            withContext(
                                NonCancellable,
                            ) { activeSecrets.remove("${key.profileId}/$ref") }
                        }
                    }
                }
            }
        }

    suspend fun assign(
        draftId: String,
        conversationKey: String,
        screenCurrent: () -> Boolean = { true },
    ) {
        checkCurrent()
        withDraftSnapshots(
            {
                check(!discardState.isDiscarded(operationId(draftId))) { "The draft was discarded" }
                listOf(preferences.drafts(key.profileId).single { it.draftId == draftId })
            },
        ) { snapshots ->
            val draft = snapshots.single()
            require(
                draft.conversationKey.isEmpty() && draft.phase != SendPhase.QUEUEING && draft.acceptedMessageId == null,
            )
            afterAssignSnapshot()
            processMutex.withLock {
                check(!discardState.isDiscarded(operationId(draftId))) { "The draft was discarded" }
                val latest = preferences.drafts(key.profileId).singleOrNull { it.draftId == draftId }
                check(latest == draft) { "The draft changed before assignment" }
                check(
                    save(draft.copy(conversationKey = conversationKey), screenCurrent),
                ) { "The session or screen changed" }
            }
            recover()
        }
    }

    suspend fun discard(
        draftId: String,
        screenCurrent: () -> Boolean = { true },
    ) {
        checkCurrent()
        check(screenCurrent()) { "The screen changed" }
        val draft =
            processMutex.withLock {
                check(operationId(draftId) !in queueing) { "Wait for the queue action to finish" }
                preferences
                    .drafts(
                        key.profileId,
                    ).single { it.draftId == draftId }
                    .also {
                        check(
                            discardState.beginDiscard(operationId(draftId)),
                        ) { "This discard is already running" }
                    }
            }
        var complete = false
        try {
            // QUEUEING can have produced a message before the process lost its ID.
            val remote =
                if (draft.acceptedMessageId == null && !draftNeedsReview(draft)) {
                    withContext(Dispatchers.IO) {
                        try {
                            draft.descriptorSecretRef
                                ?.let { secrets.read(key.profileId, it) }
                                ?.let(AttachmentDescriptor::decode)
                        } catch (error: CancellationException) {
                            throw error
                        } catch (_: Throwable) {
                            // Remove the damaged reference without guessing which SDK file it owns.
                            null
                        }
                    }
                } else {
                    null
                }
            afterDiscardDescriptorRead()
            withContext(NonCancellable) {
                check(admitChange(screenCurrent) {}) { "The session or screen changed before discard" }
                try {
                    if (remote != null) attachments.deleteLocal(remote)
                } catch (error: XmtpException.Attachment) {
                    if (!error.isExpiredDraft()) throw error
                }
                afterDiscardDelete()
                // An admitted deletion finishes profile cleanup when navigation changes.
                check(preferences.removeDraft(key.profileId, draftId) { change -> admitChange(change = change) }) {
                    "The session changed"
                }
                withContext(Dispatchers.IO) {
                    admitChange { draft.descriptorSecretRef?.let { secrets.delete(key.profileId, it) } }
                }
                admitChange { mutableCards.update { cards -> cards.filterNot { it.id == draftId } } }
                complete = true
            }
        } finally {
            withContext(NonCancellable) {
                processMutex.withLock { discardState.finishDiscard(operationId(draftId), complete) }
            }
        }
    }

    companion object {
        // These guards survive an Activity recreation while its session work still runs.
        private val processMutex = Mutex()
        private val recoveryMutex = Mutex()
        private val running = mutableSetOf<String>()
        private val queueing = mutableSetOf<String>()
        private val discardState = DraftDiscardState()
        private val activeSecrets = mutableSetOf<String>()
        private val creating = mutableMapOf<String, Int>()
    }
}

internal fun Throwable.isExpiredDraft() =
    this is XmtpException.Attachment && v2.cause == AttachmentFailureCause.STAGED_UNUSABLE

internal fun Throwable.attachmentLabel(): String =
    if (this is XmtpException.Attachment) {
        "${v2.cause}${v2.httpStatus?.let { " (HTTP $it)" } ?: ""}"
    } else {
        message
            ?: javaClass.simpleName
    }

internal fun PendingAttachmentStatus.label(): String =
    when (this) {
        PendingAttachmentStatus.Waiting -> "Waiting"
        PendingAttachmentStatus.Uploading -> "Uploading"
        PendingAttachmentStatus.Complete -> "Complete"
        is PendingAttachmentStatus.Failed -> "Failed: ${v1.cause}"
    }
