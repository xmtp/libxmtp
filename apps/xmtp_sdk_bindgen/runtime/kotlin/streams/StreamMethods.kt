package uniffi.xmtp_sdk

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

data class ConversationStreamOptions(
    val conversationKind: ConversationKind? = null,
    val consentStates: List<ConsentState>? = null,
    val onClose: ((SDKStreamCloseReason) -> Unit)? = null,
    val onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
)

data class MessageStreamOptions(
    val conversationKind: ConversationKind? = null,
    val consentStates: List<ConsentState>? = null,
    val from: String? = null,
    val onClose: ((SDKStreamCloseReason) -> Unit)? = null,
    val onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
)

data class ConversationMessageStreamOptions(
    val from: String? = null,
    val onClose: ((SDKStreamCloseReason) -> Unit)? = null,
    val onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
)

private fun <T> missingOwner(onClose: ((SDKStreamCloseReason) -> Unit)?): Flow<T> =
    flow {
        val error = clientClosedError()
        try {
            onClose?.invoke(SDKStreamCloseReason.Failed(error))
        } catch (_: Throwable) {
            System.err.println("XMTP stream close callback failed")
        }
        throw error
    }

/** This cold Flow holds its client. Each collection opens a conversation reader. */
fun Conversations.stream(options: ConversationStreamOptions = ConversationStreamOptions()): Flow<Conversation> {
    val owner = ClientRegistry.get(sdkStreamOwnerKey()) ?: return missingOwner(options.onClose)
    val selection = ConversationReaderOptions(options.conversationKind, options.consentStates)
    return conversationFlow(owner, { conversationReader(selection) }, options.onClose, options.onConnectionStateChange)
}

/**
 * This cold Flow holds its client before and between collections. Each collection
 * opens a reader. With direct sequential collection, the next read acknowledges
 * the previous message after the collector returns. A buffer or asynchronous
 * operator can let acknowledgement start before downstream work finishes.
 * Cancellation cannot undo an acknowledgement that has started.
 *
 * Only one default message reader can own progress in a client database, across
 * all group, DM, and filter scopes. A second reader fails with
 * [XmtpException.ConsumerOwned]. An explicit `from` cursor opens independent
 * replay/live reading. It does not change default progress or create a durable
 * checkpoint for each downstream consumer.
 */
fun Conversations.streamAllMessages(options: MessageStreamOptions = MessageStreamOptions()): Flow<Message> {
    val owner = ClientRegistry.get(sdkStreamOwnerKey()) ?: return missingOwner(options.onClose)
    val selection = MessageReaderOptions(options.conversationKind, options.consentStates, options.from)
    return messageFlow(owner, { messageReader(selection) }, options.onClose, options.onConnectionStateChange)
}

/**
 * This cold Flow holds its client before and between collections. Each collection
 * opens a reader. With direct sequential collection, the next read acknowledges
 * the previous message after the collector returns. A buffer or asynchronous
 * operator can let acknowledgement start before downstream work finishes.
 * Cancellation cannot undo an acknowledgement that has started.
 *
 * Only one default message reader can own progress in a client database, across
 * all group, DM, and filter scopes. A second reader fails with
 * [XmtpException.ConsumerOwned]. An explicit `from` cursor opens independent
 * replay/live reading. It does not change default progress or create a durable
 * checkpoint for each downstream consumer.
 */
fun Group.streamMessages(
    options: ConversationMessageStreamOptions = ConversationMessageStreamOptions(),
): Flow<Message> {
    val owner = ClientRegistry.get(sdkStreamOwnerKey()) ?: return missingOwner(options.onClose)
    val selection = ConversationMessageReaderOptions(options.from)
    return messageFlow(owner, { messageReader(selection) }, options.onClose, options.onConnectionStateChange)
}

/**
 * This cold Flow holds its client before and between collections. Each collection
 * opens a reader. With direct sequential collection, the next read acknowledges
 * the previous message after the collector returns. A buffer or asynchronous
 * operator can let acknowledgement start before downstream work finishes.
 * Cancellation cannot undo an acknowledgement that has started.
 *
 * Only one default message reader can own progress in a client database, across
 * all group, DM, and filter scopes. A second reader fails with
 * [XmtpException.ConsumerOwned]. An explicit `from` cursor opens independent
 * replay/live reading. It does not change default progress or create a durable
 * checkpoint for each downstream consumer.
 */
fun Dm.streamMessages(options: ConversationMessageStreamOptions = ConversationMessageStreamOptions()): Flow<Message> {
    val owner = ClientRegistry.get(sdkStreamOwnerKey()) ?: return missingOwner(options.onClose)
    val selection = ConversationMessageReaderOptions(options.from)
    return messageFlow(owner, { messageReader(selection) }, options.onClose, options.onConnectionStateChange)
}

/**
 * This cold Flow holds its client before and between collections. Each collection
 * opens a reader. With direct sequential collection, the next read acknowledges
 * the previous message after the collector returns. A buffer or asynchronous
 * operator can let acknowledgement start before downstream work finishes.
 * Cancellation cannot undo an acknowledgement that has started.
 *
 * Only one default message reader can own progress in a client database, across
 * all group, DM, and filter scopes. A second reader fails with
 * [XmtpException.ConsumerOwned]. An explicit `from` cursor opens independent
 * replay/live reading. It does not change default progress or create a durable
 * checkpoint for each downstream consumer.
 */
fun Conversation.streamMessages(
    options: ConversationMessageStreamOptions = ConversationMessageStreamOptions(),
): Flow<Message> =
    when (this) {
        is Conversation.Group -> group.streamMessages(options)
        is Conversation.Dm -> dm.streamMessages(options)
    }
