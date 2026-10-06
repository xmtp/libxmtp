package uniffi.xmtp_sdk

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetAddress
import java.net.ServerSocket
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

// Real attachment failures lift to XmtpException.Attachment with one
// AttachmentFailure record, the same record as the Failed pending status.
// Rust owns the failure policy:
// xmtp_sdk/src/tests/attachment_flows.rs::attachment_failures_carry_one_record_in_errors_and_status,
// ::attachment_error_category_and_retry_follow_the_cause.
class AttachmentErrorTest {
    /** Answers every HTTP request with 503 and counts the requests. */
    private class UnavailableServer : AutoCloseable {
        private val requests = AtomicInteger()
        private val server = ServerSocket(0, 50, InetAddress.getLoopbackAddress())
        private val thread =
            thread(isDaemon = true) {
                while (!server.isClosed) {
                    val socket = runCatching { server.accept() }.getOrNull() ?: break
                    socket.use {
                        val reader = it.getInputStream().bufferedReader()
                        while (reader.readLine()?.isNotEmpty() == true) {
                            // Read the request head.
                        }
                        requests.incrementAndGet()
                        it.getOutputStream().write(
                            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                                .toByteArray(),
                        )
                    }
                }
            }
        val url = "http://127.0.0.1:${server.localPort}/file"

        fun requests(): Int = requests.get()

        override fun close() {
            server.close()
            thread.join(5_000)
        }
    }

    private fun options(root: Path) =
        liveOptions().copy(
            storage = StorageOptions(location = StorageLocation.Directory(root.toString())),
            attachments = AttachmentOptions(allowPrivateNetwork = true),
        )

    private suspend fun thrown(action: suspend () -> Unit): XmtpException.Attachment {
        val error = runCatching { action() }.exceptionOrNull()
        assertTrue("Expected an attachment error, got $error", error is XmtpException.Attachment)
        return (error as XmtpException.Attachment).also { assertEquals("Attachment", it.v1.code) }
    }

    private fun failure(
        cause: AttachmentFailureCause,
        httpStatus: UShort? = null,
    ) = AttachmentFailure(cause, null, false, false, httpStatus)

    @Test
    fun failuresCarryOneRecordInTheErrorAndTheStatus() =
        runBlocking {
            val root = Files.createTempDirectory("xmtp-attachment-errors-").toRealPath()
            try {
                withTimeout(60_000) {
                    withClients {
                        val client = create(options = options(root.resolve("sender")))
                        val attachments = client.attachments()
                        val missing =
                            AttachmentSource.Path(
                                root.resolve("missing.bin").toString(),
                                null,
                                "application/octet-stream",
                            )
                        assertEquals(
                            AttachmentFailureCause.SOURCE_UNREADABLE,
                            thrown { attachments.create(missing) }.v2.cause,
                        )

                        val pending =
                            attachments.create(
                                AttachmentSource.Bytes("staged".toByteArray(), "note.txt", "text/plain"),
                            )
                        val remote = pending.remoteAttachment()
                        val staged =
                            Path
                                .of(checkNotNull(client.storage().path()))
                                .parent
                                .resolve("attachments")
                                .resolve(".staged")
                                .resolve(remote.contentDigest)
                        Files.delete(staged)
                        // The event reader is registered before events() returns.
                        val events =
                            client.events(
                                EventFilter(
                                    kinds =
                                        listOf(
                                            EventKind.ATTACHMENT_UPLOAD_STARTED,
                                            EventKind.ATTACHMENT_UPLOAD_FAILED,
                                        ),
                                ),
                            )
                        val unusable = thrown { pending.upload() }.v2
                        assertEquals(failure(AttachmentFailureCause.STAGED_UNUSABLE), unusable)
                        assertEquals(PendingAttachmentStatus.Failed(unusable), pending.status())
                        val (started, failed) = withTimeout(10_000) { events.take(2).toList() }
                        val startedRef = (started as ClientEvent.AttachmentUploadStarted).attachmentUploadStarted
                        val failedRef = (failed as ClientEvent.AttachmentUploadFailed).attachmentUploadFailed
                        assertEquals(remote.contentDigest, startedRef.contentDigest)
                        assertEquals(startedRef.attachmentKey, failedRef.attachmentKey)
                        assertEquals(remote.contentDigest, failedRef.contentDigest)
                        assertEquals("staged_unusable", failedRef.cause)

                        // The creating client holds the plaintext, so another client downloads.
                        val downloader = create(options = options(root.resolve("downloader")))
                        val downloadEvents =
                            downloader.events(EventFilter(kinds = listOf(EventKind.ATTACHMENT_DOWNLOAD_FAILED)))
                        UnavailableServer().use { unavailable ->
                            val error = thrown { downloader.attachments().download(remote.copy(url = unavailable.url)) }
                            assertEquals(failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 503u), error.v2)
                            assertEquals(ErrorCategory.NETWORK, error.v1.category)
                            assertTrue(error.v1.retryable)
                            assertEquals("The SDK retried the download", 1, unavailable.requests())
                            val downloadFailed = withTimeout(10_000) { downloadEvents.take(1).toList() }.single()
                            val downloadRef =
                                (downloadFailed as ClientEvent.AttachmentDownloadFailed).attachmentDownloadFailed
                            assertEquals(unavailable.url, downloadRef.url)
                            assertEquals(remote.contentDigest, downloadRef.contentDigest)
                            assertEquals("http_status", downloadRef.cause)
                        }
                    }
                }
            } finally {
                root.toFile().deleteRecursively()
            }
        }
}
