import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.lang.ref.WeakReference
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

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

internal fun sameEncoded(
    actual: EncodedContent,
    expected: EncodedContent,
): Boolean =
    actual.type == expected.type && actual.parameters == expected.parameters &&
        actual.fallback == expected.fallback && actual.content.contentEquals(expected.content)

internal fun signCommand(
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

internal class TestSigner : Signer {
    override suspend fun identity() = PublicIdentity(signCommand("identity"), PublicIdentityKind.ETHEREUM)

    override suspend fun kind() = SignerKind.Eoa

    override suspend fun sign(request: SigningRequest): Signature {
        val hex = signCommand("sign", request.text).removePrefix("0x")
        return Signature.Ecdsa(hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray())
    }
}

internal class RecordingSigner(
    private val inner: Signer,
    private val calls: MutableList<String>,
) : Signer {
    override suspend fun identity() = inner.identity()

    override suspend fun kind() = inner.kind()

    override suspend fun sign(request: SigningRequest): Signature {
        calls.add("sign")
        return inner.sign(request)
    }
}

internal class RecordingPreAuthenticate(
    private val calls: MutableList<String>,
    private val fail: Boolean,
) : PreAuthenticate {
    override suspend fun run() {
        calls.add("pre-authenticate")
        if (fail) throw PreAuthenticateException.Failed()
    }
}

internal class OrderedLogSink : LogSink {
    val sequence = mutableListOf<String>()

    override fun log(record: LogRecord) {
        if (record.target == "xmtp_sdk::conformance") {
            sequence.add(record.fields["sequence"] ?: "")
        }
    }
}

internal class SampleCodec : SDKContentCodec {
    override val type = ContentTypeId("example.org", "sample", 1u, 0u)

    override fun encode(value: Any) = EncodedContent(type, emptyMap(), null, (value as String).toByteArray())

    override fun decode(encoded: EncodedContent): Any = encoded.content.decodeToString()
}

internal class FailingCodec : SDKContentCodec {
    override val type = SampleCodec().type

    override fun encode(value: Any) = SampleCodec().encode(value)

    override fun decode(encoded: EncodedContent): Any = throw AssertionError("codec decode failed")
}

internal suspend fun releasedMessage(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId: InboxId,
): Pair<Message, WeakReference<SDKClient>> {
    val host = SDKClient.build(identity, options, inboxId)
    val group = host.raw.conversations().createGroup(emptyList(), null)
    val id = group.sendText("weak owner", null)
    val message = group.messages(null).first { it.id == id }
    return message to WeakReference(host)
}

