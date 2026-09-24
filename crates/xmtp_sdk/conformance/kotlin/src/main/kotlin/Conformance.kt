import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.lang.ref.WeakReference
import java.nio.file.Files

private fun signCommand(
    action: String,
    text: String? = null,
): String {
    val args =
        listOf(
            System.getenv("SDK_NODE_BIN"),
            System.getenv("SDK_SIGN_SCRIPT"),
            action,
        ) + listOfNotNull(text)
    val process = ProcessBuilder(args).start()
    val result =
        process.inputStream
            .bufferedReader()
            .readText()
            .trim()
    check(process.waitFor() == 0) { process.errorStream.bufferedReader().readText() }
    return result
}

private class TestSigner : Signer {
    override suspend fun identity() = PublicIdentity(signCommand("identity"), PublicIdentityKind.ETHEREUM)

    override suspend fun kind() = SignerKind.Eoa

    override suspend fun sign(request: SigningRequest): Signature {
        val hex = signCommand("sign", request.text).removePrefix("0x")
        return Signature.Ecdsa(hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray())
    }
}

private suspend fun releasedMessage(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxID: InboxID,
): Pair<Message, WeakReference<SDKClient>> {
    val host = SDKClient.build(identity, options, inboxID)
    val group = host.raw.conversations().createGroup(emptyList())
    val id = group.sendText("weak owner")
    val message = group.messages().first { it.id == id }
    return message to WeakReference(host)
}

fun main() =
    runBlocking {
        check(sdkVersion().startsWith("1.12.0"))
        check(MessageID.fromString("a".repeat(64)).toString().length == 64)
        println("Kotlin scenario 1: load, checksums, version passed")

        val signer = TestSigner()
        val directory = Files.createTempDirectory("xmtp-sdk-conformance-")
        val options =
            ClientOptions(
                backend = BackendOptions(url = checkNotNull(System.getenv("XMTP_BACKEND_URL"))),
                storage = StorageOptions(location = StorageLocation.Directory(directory.toString())),
                deviceSync = false,
            )
        val host = SDKClient.create(signer, options)
        val client = host.raw
        val inboxID = client.inboxID()
        val group = client.conversations().createGroup(emptyList())
        val sentID = group.sendText("conformance message")
        val sent = group.messages().first { it.id == sentID }
        check(sent.client() === host)
        host.end()
        check(runCatching { sent.client() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(Message(sent.data.copy(clientKey = sent.data.clientKey + 1uL)) != sent)
        val reopenedHost = SDKClient.build(signer.identity(), options, inboxID)
        val reopened = reopenedHost.raw
        check(reopened.inboxID() == inboxID)
        val defaultDirectory = Files.createTempDirectory("xmtp-sdk-default-")
        val defaultClient =
            SDKClient.build(
                signer.identity(),
                options.copy(storage = options.storage.copy(location = StorageLocation.Default)),
                inboxID,
                defaultDirectory = defaultDirectory.toString(),
            )
        check(Files.list(defaultDirectory).use { paths -> paths.anyMatch { it.fileName.toString().endsWith(".db3") } })
        defaultClient.end()
        val (orphan, weak) = releasedMessage(signer.identity(), options, inboxID)
        repeat(50) {
            if (weak.get() == null) return@repeat
            System.gc()
            delay(50)
        }
        check(weak.get() == null) { "the registry kept the host client alive" }
        check(runCatching { orphan.client() }.exceptionOrNull() is XmtpException.ClientClosed)
        println("Kotlin scenario 2: create, reopen, end passed")

        val liveGroup = reopened.conversations().createGroup(emptyList())
        val reader = liveGroup.messageReader()
        val liveID = liveGroup.sendText("durable stream")
        check(reader.next()?.id == liveID)
        reader.end()
        val replay = liveGroup.messageReader()
        check(replay.next()?.id == liveID)
        val pending = async { replay.next() }
        delay(50)
        pending.cancel()
        replay.end()
        runCatching { pending.await() }
        val adapterID = liveGroup.sendText("adapter stream")
        var delivered = false
        try {
            withTimeout(10_000) {
                reopenedHost.messages(liveGroup).collect { message ->
                    check(message.id == adapterID)
                    delivered = true
                }
            }
        } catch (_: TimeoutCancellationException) {
            check(delivered)
        }
        val protocolGroup = reopened.conversations().createGroup(emptyList())
        val firstID = protocolGroup.sendText("ack on request")
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(1)
                .toList()
                .single()
                .id == firstID,
        )
        val reread = protocolGroup.messageReader()
        check(withTimeout(3_000) { reread.next() }?.id == firstID) {
            "adapter prefetched and acknowledged a value"
        }
        reread.end()
        val secondID = protocolGroup.sendText("second request")
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(2)
                .toList()
                .map { it.id } == listOf(firstID, secondID),
        )
        val afterAck = protocolGroup.messageReader()
        check(afterAck.next()?.id == secondID) { "adapter did not acknowledge on next request" }
        afterAck.end()
        val cancelledOpening = async { reopenedHost.messages(protocolGroup).collect {} }
        cancelledOpening.cancel(CancellationException("cancel during reader creation"))
        withTimeout(10_000) { cancelledOpening.join() }
        val reopenedReader = protocolGroup.messageReader()
        reopenedReader.end()
        reopenedHost.end()
        println("Kotlin scenario 7: durable stream and idle cancellation passed")
    }
