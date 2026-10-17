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

// Resolve the owner before returning a cold Flow. Each collection uses the
// selected opener and the same typed selection mapper.
private fun openConversationStreamOptions(
    ownerKey: ULong,
    open: suspend (ConversationReaderOptions?) -> ConversationReader,
    options: ConversationStreamOptions,
): Flow<Conversation> {
    val owner = ClientRegistry.get(ownerKey) ?: return missingOwner(options.onClose)
    val selection = ConversationReaderOptions(options.conversationKind, options.consentStates)
    return conversationFlow(owner, { open(selection) }, options.onClose, options.onConnectionStateChange)
}

private fun openMessageStreamOptions(
    ownerKey: ULong,
    open: suspend (MessageReaderOptions?) -> MessageReader,
    options: MessageStreamOptions,
): Flow<Message> {
    val owner = ClientRegistry.get(ownerKey) ?: return missingOwner(options.onClose)
    val selection = MessageReaderOptions(options.conversationKind, options.consentStates, options.from)
    return messageFlow(owner, { open(selection) }, options.onClose, options.onConnectionStateChange)
}

private fun openConversationMessageStreamOptions(
    ownerKey: ULong,
    open: suspend (ConversationMessageReaderOptions?) -> MessageReader,
    options: ConversationMessageStreamOptions,
): Flow<Message> {
    val owner = ClientRegistry.get(ownerKey) ?: return missingOwner(options.onClose)
    val selection = ConversationMessageReaderOptions(options.from)
    return messageFlow(owner, { open(selection) }, options.onClose, options.onConnectionStateChange)
}
