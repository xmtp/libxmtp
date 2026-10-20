package org.xmtp.android.example.messenger.session

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.SessionKey
import uniffi.xmtp_sdk.ConnectionState
import uniffi.xmtp_sdk.ConsentState
import uniffi.xmtp_sdk.ConversationStreamOptions
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.MessageStreamOptions
import uniffi.xmtp_sdk.XmtpException
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

    val started: Boolean get() = conversationJob != null

    /** The default message reader owns the database's consumer lease. Run it only while the app is in the foreground. */
    val messagesStarted: Boolean get() = messageJob != null

    fun start(
        owner: ActiveSession,
        messages: Boolean,
    ) {
        check(!started) { "The session readers are already started" }
        if (messages) startMessages(owner)
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

    fun startMessages(owner: ActiveSession) =
        startMessages(owner.key, { options -> owner.client.conversations.streamAllMessages(options) }) { message ->
            projection.handle(owner, message)
        }

    /** The default reader loop. [open] opens one SDK message stream; JVM tests pass a fake one. */
    internal fun startMessages(
        key: SessionKey,
        open: (MessageStreamOptions) -> Flow<Message>,
        handle: suspend (Message) -> Unit,
    ) {
        check(!messagesStarted) { "The default reader is already started" }
        messageJob =
            scope.launch {
                while (accepts(key)) {
                    try {
                        open(
                            MessageStreamOptions(
                                consentStates = listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN),
                                onConnectionStateChange = { _, value -> connection(key, value.toString()) },
                            ),
                        ).collect { message ->
                            if (!accepts(key)) throw CancellationException("Session changed")
                            // Direct collection must await required handling before the next SDK read.
                            handle(message)
                            if (!accepts(key)) throw CancellationException("Session changed during handling")
                        }
                        return@launch
                    } catch (failure: XmtpException.ConsumerOwned) {
                        // A killed process can leave its lease. Retry until the lease expires.
                        connection(key, ConnectionState.RECONNECTING.toString())
                        delay(1_000)
                    } catch (failure: Throwable) {
                        if (failure is CancellationException) throw failure
                        readerFailure(key, failure)
                        return@launch
                    }
                }
            }
    }

    suspend fun stopMessages() {
        messageJob?.cancelAndJoin()
        messageJob = null
    }

    suspend fun stop() {
        stopMessages()
        conversationJob?.cancelAndJoin()
        conversationJob = null
    }
}
