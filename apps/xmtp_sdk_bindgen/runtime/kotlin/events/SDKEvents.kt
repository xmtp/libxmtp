package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext

/** Read one event only when the flow collector requests it. */
suspend fun SDKClient.events(filter: EventFilter): Flow<ClientEvent> {
    val reader = raw.events(filter)
    return flow {
        try {
            while (true) {
                emit(reader.next() ?: break)
            }
        } finally {
            withContext(NonCancellable) {
                reader.end()
            }
        }
    }
}

suspend fun SDKClient.startListener(
    filter: EventFilter,
    onEvent: suspend (ClientEvent) -> Unit,
): ListenerID {
    val handler = onEvent
    return raw.startListener(
        filter,
        object : EventListener {
            override suspend fun onEvent(event: ClientEvent) {
                withContext(NonCancellable) {
                    try {
                        handler(event)
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (_: Throwable) {
                        throw ListenerException.Failed()
                    }
                }
            }
        },
    )
}

suspend fun SDKClient.stopListener(id: ListenerID) = raw.stopListener(id)
