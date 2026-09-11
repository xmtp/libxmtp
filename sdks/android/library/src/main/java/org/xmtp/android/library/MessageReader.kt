package org.xmtp.android.library

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.sync.Mutex
import org.xmtp.android.library.Conversations.ConversationFilterType
import org.xmtp.android.library.libxmtp.DecodedMessage
import uniffi.xmtpv3.FfiConversationType
import uniffi.xmtpv3.FfiDeliveryCursor
import uniffi.xmtpv3.FfiMessageCatchUpSnapshot
import uniffi.xmtpv3.FfiMessageHistorySnapshot
import uniffi.xmtpv3.FfiMessageReader

typealias DeliveryCursor = FfiDeliveryCursor
typealias MessageCatchUpSnapshot = FfiMessageCatchUpSnapshot

data class MessageHistorySnapshot(
    val messages: List<DecodedMessage>,
    val cursor: DeliveryCursor,
)

internal fun FfiMessageHistorySnapshot.toMessageHistorySnapshot(): MessageHistorySnapshot =
    MessageHistorySnapshot(
        messages =
            messages.mapNotNull { item ->
                DecodedMessage.createForDelivery(item.message, item.cursor)
            },
        cursor = cursor,
    )

internal fun ConversationFilterType.toMessageConversationType(): FfiConversationType? =
    when (this) {
        ConversationFilterType.ALL -> null
        ConversationFilterType.GROUPS -> FfiConversationType.GROUP
        ConversationFilterType.DMS -> FfiConversationType.DM
    }

/** A sequential reader. Close it when finished. Reading a cursor does not acknowledge a message. */
class MessageReader internal constructor(
    private val ffiReader: FfiMessageReader,
) : AutoCloseable {
    private val delivery =
        AcknowledgedMessageReader(
            read = { ffiReader.nextDelivery()?.toQueuedMessageDelivery() },
            end = { ffiReader.end() },
        )

    /** Acknowledges the last returned item, then returns the next item. Cancellation closes this reader. */
    suspend fun next(): DecodedMessage? = delivery.next()

    /** Null selects all groups. An empty list selects no groups. This does not rewind delivery. */
    fun updateScope(groupIds: List<String>?) {
        ffiReader.updateScope(groupIds?.map { it.hexToByteArray() })
    }

    fun updateFilter(
        type: ConversationFilterType = ConversationFilterType.ALL,
        consentStates: List<ConsentState>? = null,
    ) {
        ffiReader.updateFilter(
            type.toMessageConversationType(),
            consentStates?.map { ConsentState.toFfiConsentState(it) },
        )
    }

    fun catchUpSnapshot(): MessageCatchUpSnapshot = ffiReader.catchUpSnapshot()

    suspend fun catchUpChanged(): MessageCatchUpSnapshot = ffiReader.catchUpChanged()

    /** Uses the direct Flow collector boundary. App-added buffering changes that boundary. */
    fun messages(): Flow<DecodedMessage> =
        flow {
            try {
                while (true) {
                    emit(next() ?: break)
                }
            } finally {
                close()
            }
        }

    /** Rejects the last item and closes native ownership before returning. */
    override fun close() {
        delivery.close()
    }
}

/** Keep the last token until acknowledgement succeeds or close explicitly rejects it. */
internal class AcknowledgedMessageReader<T>(
    private val read: suspend () -> QueuedMessageDelivery<T>?,
    private val end: () -> Unit,
) : AutoCloseable {
    private val stateLock = Any()
    private val nextLock = Mutex()
    private var pending: QueuedMessageDelivery<T>? = null
    private var closed = false

    suspend fun next(): T? {
        if (!nextLock.tryLock()) {
            close()
            throw XMTPException("Only one message next request can be active")
        }
        try {
            currentCoroutineContext().ensureActive()
            val previous = synchronized(stateLock) { pending.takeUnless { closed } }
            if (previous != null) {
                previous.acknowledge()
                clearPending(previous)
            }
            while (!isClosed()) {
                val item = read()
                if (item == null) {
                    close()
                    return null
                }
                val accepted =
                    synchronized(stateLock) {
                        if (closed) {
                            false
                        } else {
                            pending = item
                            true
                        }
                    }
                if (!accepted) {
                    item.reject()
                    return null
                }
                currentCoroutineContext().ensureActive()
                val message = decodeOrSkip(item)
                currentCoroutineContext().ensureActive()
                if (!item.checkOwner()) {
                    clearPending(item)?.reject()
                    continue
                }
                currentCoroutineContext().ensureActive()
                if (message == null) {
                    if (isClosed()) return null
                    item.acknowledge()
                    clearPending(item)
                    continue
                }
                return if (isClosed()) null else message
            }
            return null
        } catch (error: Throwable) {
            close()
            throw error
        } finally {
            nextLock.unlock()
        }
    }

    private fun clearPending(item: QueuedMessageDelivery<T>): QueuedMessageDelivery<T>? =
        synchronized(stateLock) {
            if (pending === item) {
                pending.also { pending = null }
            } else {
                null
            }
        }

    private fun isClosed(): Boolean = synchronized(stateLock) { closed }

    override fun close() {
        val last =
            synchronized(stateLock) {
                if (closed) return
                closed = true
                pending.also { pending = null }
            }
        try {
            end()
        } finally {
            last?.reject()
        }
    }
}
