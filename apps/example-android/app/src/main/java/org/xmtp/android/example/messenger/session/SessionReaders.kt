package org.xmtp.android.example.messenger.session

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.SessionKey
import uniffi.xmtp_sdk.ConsentState
import uniffi.xmtp_sdk.ConversationStreamOptions
import uniffi.xmtp_sdk.MessageStreamOptions
import uniffi.xmtp_sdk.stream
import uniffi.xmtp_sdk.streamAllMessages

/** AppSession serializes start and stop. No screen owns a default reader. */
internal class SessionReaders(
    private val scope: CoroutineScope,
    private val projection: SessionProjection,
    private val accepts: (SessionKey) -> Boolean,
    private val connection: (SessionKey, String) -> Unit,
    private val readerFailure: (SessionKey, Throwable) -> Unit,
    private val conversationFailure: (SessionKey, Throwable) -> Unit,
) {
    private var messageJob: Job? = null
    private var conversationJob: Job? = null

    val started: Boolean get() = messageJob != null

    fun start(owner: ActiveSession) {
        check(!started) { "The default reader is already started" }
        messageJob =
            scope.launch {
                try {
                    owner.client.conversations
                        .streamAllMessages(
                            MessageStreamOptions(
                                consentStates = listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN),
                                onConnectionStateChange = { _, value -> connection(owner.key, value.toString()) },
                            ),
                        ).collect { message ->
                            if (!accepts(owner.key)) throw CancellationException("Session changed")
                            // Direct collection must await required handling before the next SDK read.
                            projection.handle(owner, message)
                            if (!accepts(owner.key)) throw CancellationException("Session changed during handling")
                        }
                } catch (failure: Throwable) {
                    if (failure is CancellationException) throw failure
                    readerFailure(owner.key, failure)
                }
            }
        conversationJob =
            scope.launch {
                try {
                    owner.client.conversations
                        .stream(
                            ConversationStreamOptions(
                                consentStates = listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN),
                            ),
                        ).collect {
                            if (accepts(owner.key)) projection.invalidate(owner)
                        }
                } catch (failure: Throwable) {
                    if (failure is CancellationException) throw failure
                    conversationFailure(owner.key, failure)
                }
            }
    }

    suspend fun stop() {
        messageJob?.cancelAndJoin()
        messageJob = null
        conversationJob?.cancelAndJoin()
        conversationJob = null
    }
}
