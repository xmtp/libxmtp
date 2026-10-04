import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

internal fun sha256Hex(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

/** The deployment directory name of an identifier, from the identifier alone. */
internal fun deploymentComponent(identifier: String): String {
    // A short printable identifier with no character the file name table
    // removes keeps its bytes, lowercased.
    check(
        identifier.length in 1..190 &&
            identifier.all { it in ' '..'~' && it !in "<>:\"|?*/\\" } &&
            identifier.first() !in ". " &&
            identifier.last() !in ". ",
    ) { "the expected deployment directory assumes a file-safe identifier: $identifier" }
    return "${identifier.lowercase()}-${sha256Hex(identifier.toByteArray())}"
}

/**
 * A TCP relay to the backend that counts the connections it accepts. After
 * `refuse`, it closes every connection it accepts.
 */
internal class CountingRelay(
    backendUrl: String,
) : AutoCloseable {
    private val target = URI(backendUrl)
    private val server = ServerSocket(0, 50, InetAddress.getLoopbackAddress())
    private val sockets = ConcurrentHashMap.newKeySet<Socket>()
    private val accepted = AtomicInteger()

    @Volatile private var forward = true
    val url = "http://127.0.0.1:${server.localPort}"

    init {
        thread(isDaemon = true) {
            while (true) {
                val inbound =
                    try {
                        server.accept()
                    } catch (_: IOException) {
                        break
                    }
                accepted.incrementAndGet()
                if (!forward) {
                    inbound.close()
                    continue
                }
                val outbound = Socket(target.host, if (target.port > 0) target.port else 80)
                sockets.add(inbound)
                sockets.add(outbound)
                pipe(inbound, outbound)
                pipe(outbound, inbound)
            }
        }
    }

    private fun pipe(
        from: Socket,
        to: Socket,
    ) = thread(isDaemon = true) {
        try {
            from.getInputStream().transferTo(to.getOutputStream())
        } catch (_: IOException) {
        } finally {
            from.close()
            to.close()
            sockets.remove(from)
            sockets.remove(to)
        }
    }

    fun connections(): Int = accepted.get()

    fun refuse() {
        forward = false
        accepted.set(0)
    }

    override fun close() {
        server.close()
        sockets.forEach { it.close() }
    }
}

/** Answer every request with one status and body, and count the requests. */
internal class ServedFile(
    status: Int,
    body: ByteArray,
) : AutoCloseable {
    private val requests = AtomicInteger()
    private val server =
        HttpServer.create(InetSocketAddress(InetAddress.getLoopbackAddress(), 0), 0).apply {
            createContext("/") { exchange ->
                requests.incrementAndGet()
                exchange.requestBody.readAllBytes()
                exchange.responseHeaders.add("connection", "close")
                exchange.sendResponseHeaders(status, if (body.isEmpty()) -1 else body.size.toLong())
                exchange.responseBody.use { it.write(body) }
            }
            start()
        }
    val url = "http://127.0.0.1:${server.address.port}/file"

    fun requests(): Int = requests.get()

    override fun close() = server.stop(0)
}

internal val attachmentKinds =
    listOf(
        EventKind.ATTACHMENT_UPLOAD_STARTED,
        EventKind.ATTACHMENT_UPLOAD_COMPLETED,
        EventKind.ATTACHMENT_UPLOAD_FAILED,
        EventKind.ATTACHMENT_DOWNLOAD_STARTED,
        EventKind.ATTACHMENT_DOWNLOAD_COMPLETED,
        EventKind.ATTACHMENT_DOWNLOAD_FAILED,
        EventKind.ATTACHMENT_DELETED,
    )

internal fun attachmentFilter(kinds: List<EventKind> = attachmentKinds) =
    EventFilter(
        kinds = kinds + EventKind.CONVERSATION_JOINED,
        conversationIds = null,
        contentTypes = null,
        referencesOwnMessages = false,
    )

/** One attachment event: its kind, the attachment, and a failure's cause. */
internal data class AttachmentEvent(
    val kind: EventKind,
    val attachmentKey: String,
    val url: String,
    val contentDigest: String,
    val cause: AttachmentFailureCause? = null,
)

private fun AttachmentRef.event(kind: EventKind) = AttachmentEvent(kind, attachmentKey, url, contentDigest)

private fun AttachmentFailed.event(kind: EventKind) = AttachmentEvent(kind, attachmentKey, url, contentDigest, cause)

internal fun attachmentEvent(event: ClientEvent): AttachmentEvent? =
    when (event) {
        is ClientEvent.AttachmentUploadStarted -> {
            event.attachmentUploadStarted.event(
                EventKind.ATTACHMENT_UPLOAD_STARTED,
            )
        }

        is ClientEvent.AttachmentUploadCompleted -> {
            event.attachmentUploadCompleted.event(
                EventKind.ATTACHMENT_UPLOAD_COMPLETED,
            )
        }

        is ClientEvent.AttachmentUploadFailed -> {
            event.attachmentUploadFailed.event(EventKind.ATTACHMENT_UPLOAD_FAILED)
        }

        is ClientEvent.AttachmentDownloadStarted -> {
            event.attachmentDownloadStarted.event(
                EventKind.ATTACHMENT_DOWNLOAD_STARTED,
            )
        }

        is ClientEvent.AttachmentDownloadCompleted -> {
            event.attachmentDownloadCompleted.event(
                EventKind.ATTACHMENT_DOWNLOAD_COMPLETED,
            )
        }

        is ClientEvent.AttachmentDownloadFailed -> {
            event.attachmentDownloadFailed.event(
                EventKind.ATTACHMENT_DOWNLOAD_FAILED,
            )
        }

        is ClientEvent.AttachmentDeleted -> {
            event.attachmentDeleted.event(EventKind.ATTACHMENT_DELETED)
        }

        else -> {
            null
        }
    }

/** Events of one reader, read one at a time with a bound on the wait. */
internal class EventQueue private constructor(
    private val job: Job,
    private val channel: Channel<ClientEvent>,
) {
    suspend fun next(): ClientEvent = withTimeout(10_000) { channel.receive() }

    suspend fun ended(): Boolean = withTimeout(10_000) { channel.receiveCatching().isClosed }

    suspend fun end() = job.cancelAndJoin()

    /** Read attachment events up to the group this creates, which marks the end. */
    suspend fun drain(client: SDKClient): List<AttachmentEvent> {
        val marker = client.conversations().createGroup(emptyList(), null).id()
        val events = mutableListOf<AttachmentEvent>()
        while (true) {
            val event = next()
            if (event is ClientEvent.ConversationJoined) {
                if (event.conversationJoined.conversationId == marker) return events
                continue
            }
            events.add(checkNotNull(attachmentEvent(event)) { "unexpected event $event" })
        }
    }

    companion object {
        suspend fun open(
            scope: CoroutineScope,
            client: SDKClient,
            filter: EventFilter = attachmentFilter(),
        ): EventQueue {
            val flow = client.events(filter)
            val channel = Channel<ClientEvent>(Channel.UNLIMITED)
            val job =
                scope.launch {
                    flow.collect { channel.send(it) }
                    channel.close()
                }
            return EventQueue(job, channel)
        }
    }
}

internal fun failure(
    cause: AttachmentFailureCause,
    credentialKind: CredentialFailureKind? = null,
    retryable: Boolean = false,
    missingScope: Boolean = false,
    httpStatus: UShort? = null,
) = AttachmentFailure(cause, credentialKind, retryable, missingScope, httpStatus)

internal suspend fun thrownAttachment(action: suspend () -> Unit): XmtpException.Attachment {
    val error = runCatching { action() }.exceptionOrNull()
    check(error is XmtpException.Attachment) { "expected an attachment error, got $error" }
    check(error.v1.code == "Attachment")
    return error
}

internal suspend fun thrownFailure(action: suspend () -> Unit): AttachmentFailure = thrownAttachment(action).v2

internal suspend fun checkClientClosed(action: suspend () -> Unit) {
    val error = runCatching { withTimeout(10_000) { action() } }.exceptionOrNull()
    check(error is XmtpException.ClientClosed) { "expected ClientClosed, got $error" }
}

/** The loopback relay changes only the upload grant URL and holds its PUT. */
internal class HeldTransfer private constructor(
    val url: String,
) {
    lateinit var backend: String
        private set

    suspend fun command(action: String): String =
        kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            val connection =
                java.net
                    .URI("$url/$action")
                    .toURL()
                    .openConnection() as java.net.HttpURLConnection
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            try {
                check(connection.responseCode == 200) { "transfer control $action: ${connection.responseCode}" }
                connection.inputStream.bufferedReader().use { it.readText() }
            } finally {
                connection.disconnect()
            }
        }

    suspend fun checkCounts(
        puts: Int,
        grants: Int,
    ) {
        check(
            command("counts") == "{\"puts\":$puts,\"grants\":$grants,\"gets\":0}",
        ) { "unexpected transfer request count" }
    }

    companion object {
        suspend fun open(): HeldTransfer {
            val store = checkNotNull(System.getenv("SDK_FIXTURE_URL"))
            val held = HeldTransfer("$store/transfer/${java.util.UUID.randomUUID()}")
            held.command("arm")
            held.backend = held.command("native-backend")
            return held
        }
    }
}
