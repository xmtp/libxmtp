import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
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

fun main() =
    runBlocking {
        check(sdkVersion().startsWith("1.12.0"))
        check(MessageID.fromString("a".repeat(64)).toString().length == 64)
        println("Kotlin scenario 1: load, checksums, version passed")

        val signer = TestSigner()
        val directory = Files.createTempDirectory("xmtp-sdk-conformance-")
        val options =
            ClientOptions(
                backend = BackendOptions(url = System.getenv("XMTP_BACKEND_URL") ?: "http://127.0.0.1:9150"),
                storage = StorageOptions(location = StorageLocation.Directory(directory.toString())),
                deviceSync = false,
            )
        val host = SDKClient.create(signer, options)
        val client = host.raw
        val inboxID = client.inboxID()
        val group = client.conversations().createGroup(emptyList())
        val sentID = group.sendText("conformance message")
        val sent = group.messages().first { it.id == sentID }
        check(sent.client() === client)
        host.end()
        check(runCatching { sent.client() }.exceptionOrNull()?.message == "clientClosed")
        val reopenedHost = SDKClient.build(signer.identity(), options, inboxID)
        val reopened = reopenedHost.raw
        check(reopened.inboxID() == inboxID)
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
            withTimeout(100) {
                reopenedHost.messages(liveGroup).collect { message ->
                    check(message.id == adapterID)
                    delivered = true
                }
            }
        } catch (_: TimeoutCancellationException) {
            check(delivered)
        }
        reopenedHost.end()
        println("Kotlin scenario 7: durable stream and idle cancellation passed")
    }
