package uniffi.xmtp_sdk

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.deliveryTestMessage
import java.lang.ref.WeakReference
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.nio.file.Files
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.concurrent.thread

// Hand-written Kotlin in SDKTypes.kt and SDKClient.kt: the weak ClientRegistry
// that Message.client() reads, default storage resolution, and Message equality.
class ClientOwnershipTest {
    // A fresh endpoint prevents API-client caching from hiding build's native work.
    private class Forwarder : AutoCloseable {
        private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        private val sockets = CopyOnWriteArrayList<Socket>()
        val bytes = AtomicLong()
        val url = "http://127.0.0.1:${server.localPort}"

        init {
            val upstream = URI(liveEnv("XMTP_BACKEND_URL"))
            thread(isDaemon = true) {
                runCatching {
                    while (!server.isClosed) {
                        val incoming = server.accept().also { sockets.add(it) }
                        val outgoing = Socket(upstream.host, upstream.port).also { sockets.add(it) }
                        pump(incoming, outgoing, true)
                        pump(outgoing, incoming, false)
                    }
                }
            }
        }

        private fun pump(
            from: Socket,
            to: Socket,
            count: Boolean,
        ) = thread(isDaemon = true) {
            runCatching {
                val buffer = ByteArray(8192)
                val input = from.getInputStream()
                val output = to.getOutputStream()
                while (true) {
                    val size = input.read(buffer)
                    if (size < 0) break
                    if (count) bytes.addAndGet(size.toLong())
                    output.write(buffer, 0, size)
                }
            }
        }

        override fun close() {
            server.close()
            sockets.forEach { it.close() }
        }
    }

    private class StartupCodec(
        val failure: Throwable,
        val failFirst: Boolean,
    ) : ContentCodec<String> {
        val noteType = ContentTypeId("example.org", "startup", 1u, 0u)
        var reads = 0
        override val type: ContentTypeId
            get() {
                if (++reads > if (failFirst) 0 else 1) throw failure
                return noteType
            }

        override fun encode(value: String) = EncodedContent(noteType, emptyMap(), null, value.toByteArray())

        override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()
    }

    private suspend fun codecBeforeNative(build: Boolean) =
        withTimeout(60_000) {
            val directory = Files.createTempDirectory("xmtp-codec-startup-")
            try {
                withClients {
                    val delegate = generateLocalSigner()
                    val identity = delegate.identity()
                    val storage = StorageOptions(location = StorageLocation.Directory(directory.toString()))
                    val seed = if (build) create(delegate, liveOptions().copy(storage = storage)) else null
                    val inbox = seed?.inboxId()
                    seed?.end()
                    val calls = AtomicInteger()
                    val signer =
                        object : Signer by delegate {
                            override suspend fun identity(): PublicIdentity {
                                calls.incrementAndGet()
                                return delegate.identity()
                            }
                        }
                    Forwarder().use { endpoint ->
                        suspend fun open(codec: ContentCodec<*>): SDKClient =
                            if (build) {
                                build(
                                    identity,
                                    liveOptions(endpoint.url).copy(storage = storage, allowOffline = false),
                                    inbox,
                                    listOf(codec),
                                )
                            } else {
                                create(signer, liveOptions(), codecs = listOf(codec))
                            }
                        val failure = IllegalStateException("codec type failed")
                        val error = runCatching { open(StartupCodec(failure, true)) }.exceptionOrNull()
                        assertSame(failure, error)
                        assertEquals("create invoked the signer before codec validation", 0, calls.get())
                        assertEquals("build sent upstream bytes before codec validation", 0L, endpoint.bytes.get())
                        val once = StartupCodec(failure, false)
                        val host = open(once)
                        val encoded = once.encode("usable registry")
                        val decoded = host.decodeCustom(encoded, encoded.content) as SDKMessageContent.Custom
                        assertEquals("usable registry", decoded.value)
                        assertNull(decoded.error)
                        assertEquals("The prepared registry was rebuilt", 1, once.reads)
                        if (build) {
                            assertTrue("The endpoint did not forward online build", endpoint.bytes.get() > 0)
                        } else {
                            assertTrue("The signer was not observed", calls.get() > 0)
                        }
                    }
                }
            } finally {
                directory.toFile().deleteRecursively()
            }
        }

    @Test
    fun createValidatesCodecsBeforeNativeWork() = runBlocking { codecBeforeNative(false) }

    @Test
    fun buildValidatesCodecsBeforeNativeWork() = runBlocking { codecBeforeNative(true) }

    private suspend fun expectClientClosed(action: suspend () -> Unit) {
        val error = runCatching { action() }.exceptionOrNull()
        assertTrue("Expected ClientClosed, got $error", error is XmtpException.ClientClosed)
    }

    private suspend fun releasedMessage(): Pair<Message, WeakReference<SDKClient>> {
        // This client is not ended: the test checks that the registry does not keep it alive.
        val host = SDKClient.create(generateLocalSigner(), liveOptions())
        val group = host.conversations.createGroup(emptyList())
        val id = group.sendText("weak owner")
        return group.messages().first { it.id == id } to WeakReference(host)
    }

    @Test
    fun messageClientFollowsItsOwner() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val host = create()
                    val group = host.conversations.createGroup(emptyList())
                    val id = group.sendText("owned message")
                    val sent = group.messages().first { it.id == id }
                    assertSame(host, sent.client())
                    // refresh() reads the stored message again through its owner.
                    assertEquals(sent, sent.refresh())
                    assertNotNull("A published message has a delivery cursor", sent.deliveryCursor)
                    host.end()
                    expectClientClosed { sent.client() }
                    expectClientClosed { sent.refresh() }
                }
                val (orphan, weak) = releasedMessage()
                repeat(50) {
                    if (weak.get() == null) return@repeat
                    System.gc()
                    delay(50)
                }
                assertNull("The registry kept the host client alive", weak.get())
                expectClientClosed { orphan.client() }
                expectClientClosed { orphan.refresh() }
            }
        }

    @Test
    fun defaultStorageNeedsAHostDirectory() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val signer = generateLocalSigner()
                    val existing = create(signer)
                    val identity = signer.identity()
                    val options = liveOptions().copy(storage = StorageOptions(location = StorageLocation.Default))
                    val missing =
                        runCatching {
                            SDKClient.build(
                                identity,
                                options,
                                existing.inboxId(),
                            )
                        }.exceptionOrNull()
                    assertTrue(
                        "Expected StorageLocationRequired, got $missing",
                        missing is XmtpException.StorageLocationRequired,
                    )
                    val directory = Files.createTempDirectory("xmtp-default-storage-")
                    try {
                        // The default directory has no record of this identity, so build creates no database.
                        val unknown =
                            runCatching {
                                SDKClient.build(
                                    identity,
                                    options,
                                    existing.inboxId(),
                                    defaultDirectory = directory.toString(),
                                )
                            }.exceptionOrNull()
                        assertTrue("Expected IdentityNotFound, got $unknown", unknown is XmtpException.IdentityNotFound)
                        val databases =
                            Files.walk(directory).use { paths ->
                                paths.filter { it.fileName.toString().endsWith(".db3") }.count()
                            }
                        assertEquals(0L, databases)
                    } finally {
                        directory.toFile().deleteRecursively()
                    }
                }
            }
        }

    @Test
    fun messageEqualityComparesItsData() {
        val encoded =
            EncodedContent(
                ContentTypeId("xmtp.org", "text", 1u, 0u),
                mapOf("encoding" to "UTF-8"),
                null,
                byteArrayOf(1, 2),
            )
        val base = deliveryTestMessage(encoded = encoded, fallback = "fallback")
        val data = base.data

        // Generated byte records and Message compare byte arrays by content.
        val encodedCopy = encoded.copy(content = encoded.content.copyOf())
        assertEquals(encoded, encodedCopy)
        assertEquals(encoded.hashCode(), encodedCopy.hashCode())
        val copied = Message(data.copy(rawBytes = data.rawBytes.copyOf(), encoded = encodedCopy))
        assertEquals(base, copied)
        assertEquals(base.hashCode(), copied.hashCode())

        val changes =
            mapOf(
                "client key" to data.copy(clientKey = data.clientKey + 1uL),
                "delivery cursor" to data.copy(deliveryCursor = null),
                "envelope parameters" to data.copy(encoded = encoded.copy(parameters = mapOf("key" to "different"))),
                "raw bytes" to data.copy(rawBytes = byteArrayOf(9)),
                "delivery status" to data.copy(deliveryStatus = DeliveryStatus.FAILED),
                "reactions" to
                    data.copy(
                        reactions =
                            listOf(
                                ReactionMessage(
                                    "0a".repeat(32),
                                    "0b".repeat(32),
                                    Timestamp(5),
                                    DeliveryStatus.PUBLISHED,
                                    Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE),
                                ),
                            ),
                    ),
            )
        for ((field, changed) in changes) assertNotEquals(field, base, Message(changed))

        val reaction = Reaction("thumbs", ReactionAction.ADDED, ReactionSchema.UNICODE)
        val reacted = Message(data.copy(content = MessageContent.Reaction("0c".repeat(32), null, reaction)))
        assertNotEquals(
            "reaction target",
            reacted,
            Message(data.copy(content = MessageContent.Reaction("0d".repeat(32), null, reaction))),
        )

        val parent =
            ReplyParent(
                "0e".repeat(32),
                "03".repeat(32),
                Timestamp(1),
                MessageKind.APPLICATION,
                DeliveryStatus.PUBLISHED,
                byteArrayOf(1),
                null,
                null,
                null,
                MessageBody.Text("parent"),
            )
        val reply = Message(data.copy(inReplyTo = parent))
        assertEquals("reply parent copy", reply, Message(data.copy(inReplyTo = parent.copy(rawBytes = byteArrayOf(1)))))
        assertNotEquals(
            "reply parent content",
            reply,
            Message(data.copy(inReplyTo = parent.copy(content = MessageBody.Text("changed")))),
        )

        val details = ErrorDetails("MalformedEnvelope", ErrorCategory.INPUT, false, "invalid protobuf")
        val raw = byteArrayOf(-1, -128)
        val unknown = Message(data.copy(rawBytes = raw, content = MessageContent.Unknown(null, raw, details)))
        val unknownCopy =
            Message(data.copy(rawBytes = raw.copyOf(), content = MessageContent.Unknown(null, raw.copyOf(), details)))
        assertEquals(unknown, unknownCopy)
        assertEquals(unknown.hashCode(), unknownCopy.hashCode())
        assertNotEquals(
            "failure details",
            unknown,
            Message(
                data.copy(rawBytes = raw, content = MessageContent.Unknown(null, raw, details.copy(message = "other"))),
            ),
        )
    }
}
