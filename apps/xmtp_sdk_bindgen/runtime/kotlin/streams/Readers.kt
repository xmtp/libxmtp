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
    connectionState: suspend (R) -> ConnectionState,
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
            val active =
                try {
                    opening.await()
                } catch (error: Throwable) {
                    failure = error
                    throw error
                }
            reader = active
            if (onConnectionStateChange != null) {
                monitor.launch {
                    var previous: ConnectionState? = null

                    fun emitState(current: ConnectionState) {
                        if (previous == current) return
                        onConnectionStateChange(previous, current)
                        previous = current
                    }
                    try {
                        // The first state is the one read at subscription.
                        emitState(connectionState(active))
                        while (previous != ConnectionState.CLOSED) {
                            emitState(connectionStateChanged(active, previous!!))
                        }
                    } catch (_: Throwable) {
                        // The collection reports terminal read errors.
                    }
                }
            }
            while (true) {
                val value =
                    try {
                        owner.raw.clientKey()
                        next(active)
                    } catch (error: Throwable) {
                        failure = error
                        throw error
                    } ?: break
                emit(value)
            }
        } finally {
            monitor.cancel()
            val active = reader
            if (active == null) {
                // Collection can settle before an opener returns. Close its late reader.
                withContext(NonCancellable) {
                    runCatching { end(opening.await()) }
                }
            } else {
                withContext(NonCancellable) {
                    runCatching { end(active) }
                }
            }
            val reason =
                failure
                    ?.takeUnless { it is CancellationException }
                    ?.let(SDKStreamCloseReason::Failed) ?: SDKStreamCloseReason.Closed
            try {
                onClose?.invoke(reason)
            } catch (error: Throwable) {
                System.err.println("XMTP stream close callback failed: $error")
            }
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
