package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

sealed interface SDKStreamCloseReason {
    data object Closed : SDKStreamCloseReason

    data class Failed(
        val error: Throwable,
    ) : SDKStreamCloseReason
}

private fun <T, R> readerFlow(
    owner: SDKClient,
    open: suspend () -> R,
    next: suspend (R) -> T?,
    end: suspend (R) -> Unit,
    connectionState: (R) -> ConnectionState,
    connectionStateChanged: suspend (R, ConnectionState) -> ConnectionState,
    onClose: ((SDKStreamCloseReason) -> Unit)?,
    onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)?,
): Flow<T> =
    flow {
        // The independent opener can finish after collection is cancelled.
        val openerScope = CoroutineScope(Dispatchers.Default)
        val opening = openerScope.async { open() }
        var reader: R? = null
        var failure: Throwable? = null
        val monitor = CoroutineScope(Dispatchers.Default)
        try {
            val active = opening.await()
            reader = active
            if (onConnectionStateChange != null) {
                monitor.launch {
                    var previous: ConnectionState? = null

                    fun emitState(current: ConnectionState) {
                        if (previous == current) return
                        onConnectionStateChange(previous, current)
                        previous = current
                    }
                    emitState(ConnectionState.CONNECTING)
                    emitState(connectionState(active))
                    try {
                        while (true) {
                            emitState(connectionStateChanged(active, previous!!))
                        }
                    } catch (_: Throwable) {
                        // The collection reports terminal read errors.
                    }
                }
            }
            while (true) {
                owner.raw.clientKey()
                val value = next(active) ?: break
                emit(value)
            }
        } catch (error: Throwable) {
            failure = error
            throw error
        } finally {
            monitor.cancel()
            val active = reader
            if (active == null) {
                // Collection can settle before an opener returns. Close its late reader.
                openerScope.launch { runCatching { end(opening.await()) } }
            } else {
                withContext(NonCancellable) {
                    runCatching { end(active) }
                }
            }
            val reason =
                failure
                    ?.takeUnless { it is CancellationException }
                    ?.let(SDKStreamCloseReason::Failed) ?: SDKStreamCloseReason.Closed
            onClose?.invoke(reason)
        }
    }

internal fun messageFlow(
    owner: SDKClient,
    open: suspend () -> MessageReader,
    onClose: ((SDKStreamCloseReason) -> Unit)?,
    onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)?,
): Flow<Message> =
    readerFlow(
        owner,
        open,
        next = { it.next() },
        end = { it.end() },
        connectionState = { it.connectionState() },
        connectionStateChanged = { reader, previous -> reader.connectionStateChanged(previous) },
        onClose = onClose,
        onConnectionStateChange = onConnectionStateChange,
    )

internal fun conversationFlow(
    owner: SDKClient,
    open: suspend () -> ConversationReader,
    onClose: ((SDKStreamCloseReason) -> Unit)?,
    onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)?,
): Flow<Conversation> =
    readerFlow(
        owner,
        open,
        next = { it.next() },
        end = { it.end() },
        connectionState = { it.connectionState() },
        connectionStateChanged = { reader, previous -> reader.connectionStateChanged(previous) },
        onClose = onClose,
        onConnectionStateChange = onConnectionStateChange,
    )
