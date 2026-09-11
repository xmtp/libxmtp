package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import org.xmtp.android.library.libxmtp.DecodedMessage
import uniffi.xmtpv3.FfiException
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMessageDelivery
import uniffi.xmtpv3.FfiStreamCloser
import java.util.concurrent.atomic.AtomicReference

private const val MESSAGE_DELIVERY_QUEUE_CAPACITY = 1

/**
 * Null skips the item so delivery continues. A decode failure is reported and then
 * acknowledged by the caller, because a rejected item is served again and would stop
 * delivery for good. Cancellation stays terminal.
 */
internal fun <T> decodeOrSkip(delivery: QueuedMessageDelivery<T>): T? =
    try {
        delivery.decode()
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        Log.e("XMTP message delivery", "Skipping an undecodable message", error)
        null
    }

internal class QueuedMessageDelivery<T>(
    val decode: () -> T?,
    val checkOwner: () -> Boolean,
    val acknowledge: () -> Unit,
    val reject: () -> Unit,
)

internal fun FfiMessageDelivery.toQueuedMessageDelivery(): QueuedMessageDelivery<DecodedMessage> =
    QueuedMessageDelivery(
        decode = { DecodedMessage.createForDelivery(message, cursor) },
        checkOwner = { acknowledgement.checkOwner() },
        acknowledge = { acknowledgement.acknowledge() },
        reject = { acknowledgement.reject() },
    )

internal interface MessageDeliveryCallback<T> {
    fun onMessage(delivery: QueuedMessageDelivery<T>)

    fun onError(error: Throwable)

    fun onClose()
}

/**
 * Keep the token through the SDK queue and acknowledge after the direct collector returns.
 * App-added Flow queues have their own collection boundary.
 */
internal fun <T> acknowledgedMessageFlow(
    onClose: (() -> Unit)?,
    subscribe: suspend (MessageDeliveryCallback<T>) -> (() -> Unit),
): Flow<T> =
    flow {
        val queue =
            Channel<QueuedMessageDelivery<T>>(
                capacity = MESSAGE_DELIVERY_QUEUE_CAPACITY,
                onUndeliveredElement = { it.reject() },
            )
        val failure = AtomicReference<Throwable?>(null)
        val callback =
            object : MessageDeliveryCallback<T> {
                override fun onMessage(delivery: QueuedMessageDelivery<T>) {
                    if (queue.trySend(delivery).isFailure) {
                        delivery.reject()
                        onError(XMTPException("The message delivery queue is closed or full"))
                    }
                }

                override fun onError(error: Throwable) {
                    failure.compareAndSet(null, error)
                    queue.close(error)
                }

                override fun onClose() {
                    queue.close()
                }
            }
        var end: (() -> Unit)? = null
        try {
            end = subscribe(callback)
            for (delivery in queue) {
                var acknowledged = false
                try {
                    failure.get()?.let { throw it }
                    val message = decodeOrSkip(delivery)
                    currentCoroutineContext().ensureActive()
                    if (!delivery.checkOwner()) continue
                    if (message != null) emit(message)
                    currentCoroutineContext().ensureActive()
                    failure.get()?.let { throw it }
                    delivery.acknowledge()
                    acknowledged = true
                } finally {
                    if (!acknowledged) delivery.reject()
                }
            }
        } finally {
            try {
                queue.cancel()
            } finally {
                try {
                    end?.invoke()
                } finally {
                    onClose?.invoke()
                }
            }
        }
    }

internal fun messageDeliveryFlow(
    onClose: (() -> Unit)?,
    subscribe: suspend (FfiMessageCallback) -> FfiStreamCloser,
): Flow<DecodedMessage> =
    acknowledgedMessageFlow(onClose) { callback ->
        val stream =
            subscribe(
                object : FfiMessageCallback {
                    override fun onMessage(delivery: FfiMessageDelivery) {
                        callback.onMessage(delivery.toQueuedMessageDelivery())
                    }

                    override fun onError(error: FfiException) {
                        callback.onError(error)
                    }

                    override fun onClose() {
                        callback.onClose()
                    }
                },
            )
        val end: () -> Unit = { stream.end() }
        end
    }
