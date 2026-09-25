package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext

internal class ListenerStartGate {
    private val lock = Any()
    private var stopped = false

    fun begin(): Boolean = synchronized(lock) { !stopped }

    fun stop() = synchronized(lock) { stopped = true }
}

internal class ListenerGates {
    private val lock = Any()
    private val active = mutableMapOf<ListenerID, ListenerStartGate>()
    private val pending = mutableSetOf<ListenerStartGate>()

    fun pending(gate: ListenerStartGate) = synchronized(lock) { pending.add(gate) }

    fun registered(
        id: ListenerID,
        gate: ListenerStartGate,
    ) = synchronized(lock) {
        pending.remove(gate)
        active[id] = gate
    }

    fun discard(gate: ListenerStartGate) = synchronized(lock) { pending.remove(gate) }

    fun stop(id: ListenerID) = synchronized(lock) { active.remove(id)?.stop() }

    fun stopAll() =
        synchronized(lock) {
            active.values.forEach { it.stop() }
            pending.forEach { it.stop() }
            active.clear()
            pending.clear()
        }
}

internal object EventStartHookForTest {
    @Volatile var beforeCallback: (suspend () -> Unit)? = null
}

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
    val gate = ListenerStartGate()
    listenerGates.pending(gate)
    try {
        val id =
            raw.startListener(
                filter,
                object : EventListener {
                    override suspend fun onEvent(event: ClientEvent) {
                        withContext(NonCancellable) {
                            EventStartHookForTest.beforeCallback?.invoke()
                            if (!gate.begin()) return@withContext
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
        listenerGates.registered(id, gate)
        return id
    } finally {
        listenerGates.discard(gate)
    }
}

suspend fun SDKClient.stopListener(id: ListenerID) {
    listenerGates.stop(id)
    raw.stopListener(id)
}
