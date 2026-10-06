package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path

// The attachment shutdown boundary through the generated Kotlin package. The
// caller cancels the coroutine that waits on PendingAttachment.upload(); the
// SDK keeps the upload running. SDKClient.end() must wait for it, and later
// calls on the held handles must fail with ClientClosed. The test holds the
// upload grant request at the Toxiproxy backend proxy, not the PUT: the SDK
// upload task holds the call gate from its claim until the attempt settles
// (xmtp_sdk/src/attachments.rs PendingAttachment::upload, on_settled_worker),
// so end() waits the same way for both. Rust owns the detached attempt
// (xmtp_mls/src/attachments/tests/lifecycle.rs::dropped_upload_waiter_does_not_cancel_attempt)
// and the closed calls with nothing in flight
// (xmtp_sdk/src/tests/attachment_flows.rs::attachment_calls_fail_closed_after_end).
class AttachmentLifetimeTest {
    // dev/docker/toxiproxy/config.json defines this proxy in front of the backend.
    private val hold = "/proxies/backend/toxics/attachment-lifetime-hold"

    // A latency toxic delays each request chunk to the backend. When the toxic is
    // removed, Toxiproxy passes the delayed data on at once, so the held upload
    // grant request then completes.
    private fun holdBackendRequests() =
        toxiproxy(
            "/proxies/backend/toxics",
            """{"name": "attachment-lifetime-hold", "type": "latency", "stream": "upstream", "attributes": {"latency": 600000}}""",
        )

    private fun releaseBackendRequests() = toxiproxy(hold, method = "DELETE", accepted = (200..299) + 404)

    private fun options(root: Path) =
        liveOptions(liveEnv("XMTP_BACKEND_TOXIC_URL")).copy(
            storage = StorageOptions(location = StorageLocation.Directory(root.toString())),
            attachments = AttachmentOptions(allowPrivateNetwork = true),
        )

    private suspend fun expectClientClosed(
        call: String,
        action: suspend () -> Unit,
    ) {
        val error = runCatching { action() }.exceptionOrNull()
        assertTrue("$call: expected ClientClosed, got $error", error is XmtpException.ClientClosed)
    }

    @Test
    fun endWaitsForACancelledUploadAndHeldHandlesFailClosed() =
        runBlocking {
            toxiproxy("/reset", "")
            val root = Files.createTempDirectory("xmtp-attachment-lifetime-").toRealPath()
            try {
                withTimeout(120_000) {
                    withClients {
                        val signer = generateLocalSigner()
                        val options = options(root)
                        val client = create(signer, options)
                        val attachments = client.attachments()
                        val pending =
                            attachments.create(
                                AttachmentSource.Bytes("held upload".toByteArray(), "held.txt", "text/plain"),
                            )
                        val remote = pending.remoteAttachment()
                        val started = CompletableDeferred<ClientEvent>()
                        // The event reader is registered before events() returns.
                        val events = client.events(EventFilter(kinds = listOf(EventKind.ATTACHMENT_UPLOAD_STARTED)))
                        val reader = launch(Dispatchers.Default) { events.collect { started.complete(it) } }
                        try {
                            holdBackendRequests()
                            val upload = async(Dispatchers.Default) { pending.upload() }
                            val event = withTimeout(10_000) { started.await() }
                            assertEquals(
                                remote.contentDigest,
                                (event as ClientEvent.AttachmentUploadStarted).attachmentUploadStarted.contentDigest,
                            )
                            // Cancel the coroutine that waits on the generated binding call.
                            // The SDK upload keeps running and holds the client open.
                            upload.cancelAndJoin()
                            assertTrue(upload.isCancelled)
                            val ending = async(Dispatchers.Default) { client.end() }
                            withTimeout(10_000) { reader.join() }
                            delay(1_000)
                            assertFalse("end() returned while the upload was held", ending.isCompleted)
                            releaseBackendRequests()
                            withTimeout(30_000) { ending.await() }
                        } finally {
                            releaseBackendRequests()
                            reader.cancelAndJoin()
                        }

                        // Held values stay readable; calls on the held handles fail closed.
                        assertTrue(attachments.offered())
                        assertEquals(remote.contentDigest, pending.remoteAttachment().contentDigest)
                        expectClientClosed("create") {
                            attachments.create(AttachmentSource.Bytes("late".toByteArray(), null, "text/plain"))
                        }
                        expectClientClosed("localPath(remote)") { attachments.localPath(remote) }
                        expectClientClosed("listLocal") { attachments.listLocal() }
                        expectClientClosed("download") { attachments.download(remote) }
                        expectClientClosed("pending") { attachments.pending(remote) }
                        expectClientClosed("listPending") { attachments.listPending() }
                        expectClientClosed("deleteLocal") { attachments.deleteLocal(remote) }
                        expectClientClosed("PendingAttachment.localPath") { pending.localPath() }
                        expectClientClosed("PendingAttachment.status") { pending.status() }
                        expectClientClosed("PendingAttachment.upload") { pending.upload() }

                        // The upload finished before end() released storage.
                        val reopened = build(signer.identity(), options)
                        assertEquals(
                            PendingAttachmentStatus.Complete,
                            reopened.attachments().pending(remote).status(),
                        )
                    }
                }
            } finally {
                toxiproxy("/reset", "")
                root.toFile().deleteRecursively()
            }
        }
}
