package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.xmtp.android.example.messenger.platform.EffectIdentity
import org.xmtp.android.example.shared.attachments.AttachmentSelection
import uniffi.xmtp_sdk.DownloadedAttachment
import java.util.concurrent.atomic.AtomicLong

internal sealed interface AttachmentRequest {
    val identity: EffectIdentity

    data class Select(
        override val identity: EffectIdentity,
        val conversationId: String,
        val selection: AttachmentSelection,
    ) : AttachmentRequest

    data class Save(
        override val identity: EffectIdentity,
        val messageId: String,
        val downloaded: DownloadedAttachment,
    ) : AttachmentRequest

    data class Open(
        override val identity: EffectIdentity,
        val hostId: Long,
        val export: PreparedAttachmentExport,
    ) : AttachmentRequest
}

/** Retains launched picker identities until their result arrives, including stale results. */
internal class AttachmentRequests {
    private val counter = AtomicLong()
    private val pending = MutableStateFlow<List<AttachmentRequest>>(emptyList())
    val state = pending.asStateFlow()
    private val launched = mutableSetOf<Long>()
    private var hostId: Long? = null

    fun nextId(): Long = counter.incrementAndGet()

    @Synchronized fun attach(): Long {
        remove { it is AttachmentRequest.Open }
        return nextId().also { hostId = it }
    }

    @Synchronized fun host(): Long? = hostId

    @Synchronized fun acceptsHost(id: Long): Boolean = hostId == id

    @Synchronized fun closeHost(id: Long) {
        if (hostId == id) hostId = null
        remove { it is AttachmentRequest.Open && it.hostId == id }
    }

    @Synchronized fun offer(request: AttachmentRequest): Boolean {
        if (pending.value.any { it::class == request::class }) return false
        if (request is AttachmentRequest.Open && hostId != request.hostId) return false
        pending.value += request
        return true
    }

    @Synchronized fun claim(
        request: AttachmentRequest,
        host: Long,
    ): Boolean {
        if (hostId != host || pending.value.none { it === request }) return false
        if (request is AttachmentRequest.Open && request.hostId != host) return false
        return launched.add(request.identity.requestId)
    }

    @Synchronized fun launchedSelect(): AttachmentRequest.Select? =
        pending.value.filterIsInstance<AttachmentRequest.Select>().singleOrNull { it.identity.requestId in launched }

    @Synchronized fun launchedSave(): AttachmentRequest.Save? =
        pending.value.filterIsInstance<AttachmentRequest.Save>().singleOrNull { it.identity.requestId in launched }

    @Synchronized fun consume(identity: EffectIdentity): AttachmentRequest? {
        val request = pending.value.singleOrNull { it.identity == identity } ?: return null
        if (!launched.remove(identity.requestId)) return null
        pending.value -= request
        return request
    }

    fun retain(accepts: (EffectIdentity) -> Boolean) {
        val snapshot = synchronized(this) { pending.value.toList() }
        val stale = snapshot.filterNot { accepts(it.identity) }.map { it.identity }.toSet()
        synchronized(this) {
            // A stale launched picker still occupies its slot. Its late result cannot claim a new request.
            remove { it.identity in stale && it.identity.requestId !in launched }
        }
    }

    @Synchronized fun withHost(
        id: Long,
        action: () -> Unit,
    ): Boolean {
        if (hostId != id) return false
        action()
        return true
    }

    private fun remove(predicate: (AttachmentRequest) -> Boolean) {
        val removed = pending.value.filter(predicate)
        pending.value -= removed.toSet()
        removed.forEach {
            launched.remove(it.identity.requestId)
            if (it is AttachmentRequest.Open) it.export.close()
        }
    }
}
