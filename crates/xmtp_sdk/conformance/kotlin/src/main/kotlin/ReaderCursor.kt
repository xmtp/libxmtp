import kotlinx.coroutines.flow.first
import uniffi.xmtp_sdk.*
import java.nio.ByteBuffer
import java.nio.file.Files
import java.util.Base64

// verifies: PROC-033, PROC-034, PROC-050
suspend fun checkReaderCursor(
    signer: Signer,
    backend: BackendOptions,
) {
    val path = Files.createTempDirectory("f3-cursor-").resolve("client.db").toString()
    val options =
        ClientOptions(
            backend = BackendSource.Options(backend),
            storage = StorageOptions(location = StorageLocation.Path(path)),
            deviceSync = false,
        )
    var host = SDKClient.create(signer, options)
    val identity = signer.identity()
    val inbox = host.inboxId()
    host.conversations().sdkConformanceSeedDeliveryCursor()
    val group = host.conversations().createGroup(emptyList(), null)
    val groupId = group.id()
    val beginning = host.conversations().beginningDeliveryCursor()
    val firstId = group.sendText("large A")
    val first = group.messages(null).first { it.id == firstId }
    val cursor = checkNotNull(first.deliveryCursor)

    fun sequence(value: String): Long {
        check(value.startsWith("dc1_"))
        val bytes = Base64.getUrlDecoder().decode(value.drop(4))
        check(bytes.size == 24)
        return ByteBuffer.wrap(bytes).getLong(16)
    }
    check(sequence(cursor) == 9_007_199_254_740_993L)
    check(
        host
            .conversations()
            .getMessageById(firstId)
            ?.deliveryCursor == cursor,
    )
    check(first.refresh()?.deliveryCursor == cursor)
    check(Message(first.data.copy()) == first)
    check(Message(first.data.copy()).hashCode() == first.hashCode())
    check(Message(first.data.copy(deliveryCursor = null)) != first)
    check(
        host
            .messages(
                options = MessageReaderOptions(conversationKind = ConversationKind.GROUP, from = beginning),
            ).first()
            .deliveryCursor ==
            cursor,
    )
    check(host.messages(group).first().deliveryCursor == cursor)
    val secondId = group.sendText("large B")
    val resume = group.messageReader(ConversationMessageReaderOptions(from = cursor))
    val second = checkNotNull(resume.next())
    check(second.id == secondId)
    check(sequence(checkNotNull(second.deliveryCursor)) == 9_007_199_254_740_994L)
    resume.end()
    val encoded = TextCodec().encode("reply")
    val replyId = group.sendReply(firstId, null, encoded)
    val reply = checkNotNull(host.conversations().getMessageById(replyId))
    check(reply.parent()?.deliveryCursor == cursor)
    val preparedId = group.prepareMessage(encoded)
    val pending = checkNotNull(host.conversations().getMessageById(preparedId)) { "optimistic message missing" }
    check(pending.id == preparedId)
    check(pending.deliveryCursor == null)
    group.publishMessage(preparedId)
    check(
        host
            .conversations()
            .getMessageById(preparedId)
            ?.deliveryCursor != null,
    )
    host.end()
    host = SDKClient.build(identity, options, inbox)
    val restored = host.conversations().getById(groupId) as Conversation.Group
    val repeated = host.messages(restored.group, ConversationMessageReaderOptions(from = cursor)).first()
    check(repeated.id == secondId && repeated.deliveryCursor == second.deliveryCursor)
    host.end()
    println("Kotlin F3 exact large cursor, full message, equality, selection, and reopen passed")
}

// verifies: DMS-017, PROC-034, PROC-050
suspend fun checkRestoredPeer(backend: BackendOptions) {
    val options =
        ClientOptions(
            backend = BackendSource.Options(backend),
            storage = StorageOptions(location = StorageLocation.InMemory),
            deviceSync = false,
        )
    val a = SDKClient.create(generateLocalSigner(), options)
    val b = SDKClient.create(generateLocalSigner(), options)
    val c = SDKClient.create(generateLocalSigner(), options)
    try {
        val dm = a.conversations().createDm(b.inboxId())
        val other = b.conversations().createDm(a.inboxId())
        check(dm.id() != other.id())
        check(dm.peerInboxId() == b.inboxId())
        check(other.peerInboxId() == a.inboxId())
        a.conversations().syncAll(null)
        val id = dm.sendText("foreign restored DM")
        val key = ByteArray(32) { 9 }
        val archive = a.archives().exportToBytes(key, ArchiveOptions(elements = listOf(ArchiveElement.MESSAGES)))
        c.archives().importFromBytes(archive, key)
        val restored = (c.conversations().getById(dm.id()) as Conversation.Dm).dm
        check(restored.peerInboxId() == null)
        val listed = c.conversations().listDms(ListConversationsOptions(includeDuplicateDms = true))
        check(listed.size == 2)
        for (item in listed) check(item.peerInboxId() == null)
        val duplicates = restored.duplicateDms()
        check(duplicates.size == 1 && duplicates[0].peerInboxId() == null)
        val cursor = checkNotNull(restored.messages(null).first { it.id == id }.deliveryCursor)
        val beginning = c.conversations().beginningDeliveryCursor()
        val first = c.messages(restored, ConversationMessageReaderOptions(from = beginning)).first()
        check(first.id == id && first.deliveryCursor == cursor)
        val reader = restored.messageReader()
        check(reader.next()?.deliveryCursor == cursor)
        reader.end()
        check(
            runCatching {
                restored.messageReader(ConversationMessageReaderOptions(from = "invalid"))
            }.exceptionOrNull() is XmtpException.InvalidCursor,
        )
        val foreign = a.conversations().beginningDeliveryCursor()
        check(
            runCatching {
                restored.messageReader(ConversationMessageReaderOptions(from = foreign))
            }.exceptionOrNull() is XmtpException.ForeignCursor,
        )
    } finally {
        c.end()
        b.end()
        a.end()
    }
    println("Kotlin F3 Restored peer lookup/list/duplicates and Dm selection passed")
}
