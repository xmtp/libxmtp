package org.xmtp.android.example.extension

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingCommand
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.launch

@OptIn(ExperimentalCoroutinesApi::class)
fun <T> Flow<T>.flowWhileShared(
    subscriptionCount: StateFlow<Int>,
    started: SharingStarted,
): Flow<T> {
    return started.command(subscriptionCount)
        .distinctUntilChanged()
        .flowOn(Dispatchers.IO)
        .flatMapLatest {
            when (it) {
                SharingCommand.START -> this
                SharingCommand.STOP, SharingCommand.STOP_AND_RESET_REPLAY_CACHE -> emptyFlow()
            }
        }
}

fun <T> stateFlow(
    scope: CoroutineScope,
    initialValue: T,
    producer: (subscriptionCount: StateFlow<Int>) -> Flow<T>,
): StateFlow<T> {
    val state = MutableStateFlow(initialValue)
    scope.launch(Dispatchers.IO) {
        producer(state.subscriptionCount).collect(state)
    }
    return state.asStateFlow()
}