package uniffi.xmtp_sdk

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.nio.file.Path

// The generated attachment records that a real transfer lifts back to Kotlin:
// 64-bit options, the offered configuration, the pending and local lists, the
// downloaded record, and the upload, download and delete event payloads. Rust owns the
// transfer rules:
// xmtp_sdk/src/tests/attachment_flows.rs::attachment_uploads_after_its_record_is_sent_and_downloads_on_another_client.
class AttachmentFlowTest {
    private fun options(
        root: Path,
        attachments: AttachmentOptions = AttachmentOptions(allowPrivateNetwork = true),
    ) = liveOptions().copy(
        storage = StorageOptions(location = StorageLocation.Directory(root.toString())),
        attachments = attachments,
    )

    @Test
    fun aRealTransferLiftsEachRecord() =
        runBlocking {
            val root = Files.createTempDirectory("xmtp-attachment-flow-").toRealPath()
            try {
                withTimeout(90_000) {
                    withClients {
                        // Values near the u64 maximum do not fit a signed Long.
                        val large = ULong.MAX_VALUE - 1uL
                        val settings = AttachmentOptions(large, large - 2uL, true)
                        val sender = create(options = options(root.resolve("sender"), settings))
                        assertEquals(settings, sender.options().attachments)
                        assertEquals(
                            AttachmentsConfiguration(liveEnv("XMTP_S3_BASE_URL"), 104_857_600uL, 0uL),
                            sender.serverConfiguration().attachments,
                        )
                        val attachments = sender.attachments()
                        assertTrue("The backend does not offer attachments", attachments.offered())

                        val content = "attachment bytes"
                        val pending =
                            attachments.create(AttachmentSource.Bytes(content.toByteArray(), "note.txt", "text/plain"))
                        val remote = pending.remoteAttachment()
                        assertEquals(PendingAttachmentStatus.Waiting, pending.status())
                        assertEquals(listOf(remote), attachments.listPending().map { it.remoteAttachment() })
                        assertEquals(pending.localPath(), attachments.localPath(remote))
                        assertEquals(content, File(pending.localPath()).readText())
                        // The event reader is registered before events() returns.
                        val uploadEvents =
                            sender.events(
                                EventFilter(
                                    kinds =
                                        listOf(
                                            EventKind.ATTACHMENT_UPLOAD_STARTED,
                                            EventKind.ATTACHMENT_UPLOAD_COMPLETED,
                                        ),
                                ),
                            )
                        pending.upload()
                        assertEquals(PendingAttachmentStatus.Complete, pending.status())
                        val (uploadStarted, uploadCompleted) = withTimeout(10_000) { uploadEvents.take(2).toList() }
                        val uploadRef = (uploadStarted as ClientEvent.AttachmentUploadStarted).attachmentUploadStarted
                        assertEquals(remote.url, uploadRef.url)
                        assertEquals(remote.contentDigest, uploadRef.contentDigest)
                        assertEquals(
                            uploadRef,
                            (uploadCompleted as ClientEvent.AttachmentUploadCompleted).attachmentUploadCompleted,
                        )
                        assertEquals(
                            emptyList<RemoteAttachment>(),
                            attachments.listPending().map { it.remoteAttachment() },
                        )

                        val receiver = create(options = options(root.resolve("receiver")))
                        val receiving = receiver.attachments()
                        // The event reader is registered before events() returns.
                        val events =
                            receiver.events(
                                EventFilter(
                                    kinds =
                                        listOf(
                                            EventKind.ATTACHMENT_DOWNLOAD_STARTED,
                                            EventKind.ATTACHMENT_DOWNLOAD_COMPLETED,
                                            EventKind.ATTACHMENT_DELETED,
                                        ),
                                ),
                            )
                        val before = System.currentTimeMillis()
                        val downloaded = receiving.download(remote)
                        val after = System.currentTimeMillis()
                        assertEquals(
                            DownloadedAttachment(receiving.localPath(remote), "text/plain", "note.txt"),
                            downloaded,
                        )
                        assertEquals(content, File(downloaded.path).readText())

                        val directory = Path.of(checkNotNull(receiver.storage().path())).parent.resolve("attachments")
                        val local = receiving.listLocal().single()
                        assertEquals(directory.relativize(Path.of(downloaded.path)).toString(), local.path)
                        val createdMs = local.createdAt.ns / 1_000_000
                        assertTrue(
                            "createdAt $createdMs ms is outside the download, $before..$after ms",
                            createdMs in (before - 5_000)..(after + 5_000),
                        )
                        receiving.deleteLocal(remote)
                        assertEquals(emptyList<LocalAttachment>(), receiving.listLocal())
                        assertFalse(Files.exists(Path.of(downloaded.path)))

                        val (started, completed, deleted) = withTimeout(10_000) { events.take(3).toList() }
                        val startedRef = (started as ClientEvent.AttachmentDownloadStarted).attachmentDownloadStarted
                        assertEquals(remote.url, startedRef.url)
                        assertEquals(remote.contentDigest, startedRef.contentDigest)
                        assertEquals(
                            startedRef,
                            (completed as ClientEvent.AttachmentDownloadCompleted).attachmentDownloadCompleted,
                        )
                        assertEquals(startedRef, (deleted as ClientEvent.AttachmentDeleted).attachmentDeleted)
                    }
                }
            } finally {
                root.toFile().deleteRecursively()
            }
        }
}
