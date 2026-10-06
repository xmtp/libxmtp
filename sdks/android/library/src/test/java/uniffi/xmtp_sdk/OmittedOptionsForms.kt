package uniffi.xmtp_sdk

// Compile-only consumer forms, moved from the Kotlin conformance program
// (ConformanceSupport.kt and ConsumerPositive.kt). Nothing calls them. They
// must compile against the generated API: the omitted `options` arguments
// (Rust `default(options = None)`), exhaustive Group/Dm narrowing, typed IDs
// and the received-content shapes. The compile-fail forms stay in
// crates/xmtp_sdk/conformance/kotlin/negative.

@Suppress("unused")
internal suspend fun consumeOmittedSendOptions(
    group: Group,
    conversations: Conversations,
    id: MessageId,
    reaction: Reaction,
    encoded: EncodedContent,
) {
    group.send(encoded)
    group.prepareMessage(encoded)
    conversations.reactToMessage(id, reaction)
    conversations.replyToMessage(id, encoded)
}

@Suppress("unused")
internal suspend fun consumeOmittedTypedSendOptions(
    group: Group,
    id: MessageId,
    reaction: Reaction,
    encoded: EncodedContent,
    attachment: Attachment,
    remote: RemoteAttachment,
    multiRemote: MultiRemoteAttachment,
    transaction: TransactionReference,
    walletCalls: WalletSendCalls,
    actions: Actions,
    intent: Intent,
) {
    group.sendText("text")
    group.sendMarkdown("markdown")
    group.sendReaction(id, null, reaction)
    group.sendReply(id, null, encoded)
    group.sendReadReceipt()
    group.sendAttachment(attachment)
    group.sendRemoteAttachment(remote)
    group.sendMultiRemoteAttachment(multiRemote)
    group.sendTransactionReference(transaction)
    group.sendWalletSendCalls(walletCalls)
    group.sendActions(actions)
    group.sendIntent(intent)
}

@Suppress("unused")
internal fun consumePositive(
    id: ConversationId,
    conversation: Conversation,
    content: MessageContent,
): ConversationId {
    val narrowed: ConversationId =
        when (conversation) {
            is Conversation.Group -> conversation.group.id()
            is Conversation.Dm -> conversation.dm.id()
        }
    if (content is MessageContent.Custom) {
        val encoded: EncodedContent = content.encoded
        check(encoded.type.typeId.isNotEmpty())
    }
    return if (narrowed == id) id else narrowed
}

@Suppress("unused")
internal fun consumeStandardIds(content: StandardContent): MessageId? =
    when (content) {
        is StandardContent.Reaction -> {
            val inbox: InboxId? = content.referenceInboxId
            check(inbox == null || inbox.toString().isNotEmpty())
            content.reference
        }

        is StandardContent.Reply -> {
            content.reference
        }

        is StandardContent.DeleteMessage -> {
            content.messageId
        }

        else -> {
            null
        }
    }

@Suppress("unused")
internal fun receivedDetails(message: Message): String? {
    val raw: ByteArray = message.rawBytes
    val encoded: EncodedContent? = message.encoded
    val type: ContentTypeId? = message.contentType
    check(raw.isNotEmpty() || encoded == null || type != null)
    return when (val content = message.content) {
        is SDKMessageContent.Unknown -> content.error.code
        is SDKMessageContent.Custom -> content.error?.code
        is SDKMessageContent.Standard -> null
    }
}
