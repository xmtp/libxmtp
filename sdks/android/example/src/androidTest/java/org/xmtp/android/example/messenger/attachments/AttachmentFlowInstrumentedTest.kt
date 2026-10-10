package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.messenger.SendCoordinator
import org.xmtp.android.example.messenger.SendPhase
import org.xmtp.android.example.messenger.SessionFence
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.MessageDigest

@RunWith(AndroidJUnit4::class)
class AttachmentFlowInstrumentedTest {
    private val sourceAuthority: String
        get() {
            val context = InstrumentationRegistry.getInstrumentation().context
            return context.packageName + ".file-source"
        }

    @Test fun a64MiBProviderFileUploadsAndVerifiesOnAnotherClient() =
        runBlocking {
            val sender = AttachmentTestFixture()
            val receiver = AttachmentTestFixture()
            try {
                withTimeout(180_000) {
                    sender.start()
                    receiver.start()
                    val size = 64 * 1024 * 1024
                    val authority = sourceAuthority
                    val uri = Uri.parse("content://$authority/file?bytes=$size&length=1")
                    val coordinator = sender.coordinator()
                    val draft = coordinator.select(sender.context.contentResolver, uri, sender.group.id())
                    val ref = checkNotNull(draft.descriptorSecretRef)
                    val descriptor = checkNotNull(sender.secrets.read(sender.profile.id, ref))
                    val remote = AttachmentDescriptor.decode(descriptor)
                    coordinator.send(draft.draftId, sender.group) { }
                    assertEquals(1, sender.group.messages(null).size)
                    val downloaded = receiver.client.attachments().download(remote)
                    val file = File(downloaded.path)
                    assertEquals(size.toLong(), file.length())
                    val expected = MessageDigest.getInstance("SHA-256")
                    val pattern = ByteArray(8192) { (it % 251).toByte() }
                    repeat(size / pattern.size) { expected.update(pattern) }
                    val actual = MessageDigest.getInstance("SHA-256")
                    file.inputStream().use { input ->
                        val chunk = ByteArray(64 * 1024)
                        while (true) {
                            val count = input.read(chunk)
                            if (count < 0) break
                            actual.update(chunk, 0, count)
                        }
                    }
                    assertArrayEquals(expected.digest(), actual.digest())
                    assertTrue(
                        sender.paths.temp
                            .listFiles()
                            .orEmpty()
                            .isEmpty(),
                    )
                }
            } finally {
                receiver.close()
                sender.close()
            }
        }

    @Test fun lateAdmissionRejectsDescriptorAndDraftWrites() =
        runBlocking {
            for (dropAt in 1..2) {
                val fixture = AttachmentTestFixture()
                try {
                    withTimeout(90_000) {
                        fixture.start()
                        val fence = SessionFence()
                        assertEquals(fixture.key, fence.replace(fixture.profile.id))
                        var calls = 0
                        val admission: (() -> Unit) -> Boolean = { change ->
                            calls += 1
                            if (calls == 1) {
                                assertTrue(
                                    fixture.paths.secrets
                                        .listFiles()
                                        .orEmpty()
                                        .isEmpty(),
                                )
                            }
                            if (calls == 2) {
                                assertEquals(
                                    1,
                                    fixture.paths.secrets
                                        .listFiles()
                                        .orEmpty()
                                        .size,
                                )
                            }
                            if (calls == dropAt) fence.replace(null)
                            fence.withCurrent(fixture.key) {
                                change()
                                true
                            } == true
                        }
                        val coordinator =
                            AttachmentDraftCoordinator(
                                fixture.key,
                                fixture.client,
                                fixture.paths,
                                fixture.preferences,
                                fixture.secrets,
                                SendCoordinator(fixture.preferences, fence::accepts),
                                fence::accepts,
                                admission,
                            )
                        val authority = sourceAuthority
                        val source = Uri.parse("content://$authority/file?bytes=131073&length=0")
                        try {
                            coordinator.select(fixture.context.contentResolver, source, fixture.group.id())
                            fail("A late generation change must reject persistence")
                        } catch (_: IllegalStateException) {
                        }
                        assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                        assertTrue(
                            fixture.paths.secrets
                                .listFiles()
                                .orEmpty()
                                .isEmpty(),
                        )
                        assertTrue(
                            fixture.paths.temp
                                .listFiles()
                                .orEmpty()
                                .isEmpty(),
                        )
                        assertTrue(
                            fixture.client
                                .attachments()
                                .listPending()
                                .isEmpty(),
                        )
                        assertTrue(fixture.group.messages(null).isEmpty())
                    }
                } finally {
                    fixture.close()
                }
            }
        }

    @Test fun uriLengthHintsAndNamesDoNotControlPrivateCopy() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val authority = sourceAuthority
                    val coordinator = fixture.coordinator()
                    for (hint in listOf("", "&length=0")) {
                        val uri = Uri.parse("content://$authority/file?bytes=131073$hint")
                        val draft = coordinator.select(fixture.context.contentResolver, uri, fixture.group.id())
                        val ref = checkNotNull(draft.descriptorSecretRef)
                        val savedBytes = checkNotNull(fixture.secrets.read(fixture.profile.id, ref))
                        val remote = AttachmentDescriptor.decode(savedBytes)
                        assertEquals("../../same.bin", remote.filename)
                        assertEquals(131073L, File(fixture.client.attachments().localPath(remote)).length())
                        assertTrue(
                            fixture.paths.temp
                                .listFiles()
                                .orEmpty()
                                .isEmpty(),
                        )
                        coordinator.discard(draft.draftId)
                        assertFalse(File(fixture.client.attachments().localPath(remote)).exists())
                    }
                    try {
                        val oversized = Uri.parse("content://$authority/file?bytes=131073&length=0")
                        PrivateFileStager.stage(fixture.context.contentResolver, oversized, fixture.paths.temp, 65536uL)
                        fail("The actual stream must exceed the copy ceiling")
                    } catch (_: IllegalArgumentException) {
                    }
                    assertTrue(
                        fixture.paths.temp
                            .listFiles()
                            .orEmpty()
                            .isEmpty(),
                    )
                }
            } finally {
                fixture.close()
            }
        }

    @Test fun completeUploadRecoversAndPublishesOneAcceptedMessage() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val bytes = ByteArray(131073) { (it % 251).toByte() }
                    val source = AttachmentSource.Bytes(bytes, "same-name.bin", "application/octet-stream")
                    val pending = fixture.client.attachments().create(source)
                    val remote = pending.remoteAttachment()
                    val draft = fixture.save(remote)
                    pending.upload()
                    assertTrue(
                        fixture.client
                            .attachments()
                            .listPending()
                            .isEmpty(),
                    )
                    fixture.reopen()
                    val coordinator = fixture.coordinator()
                    coordinator.recover()
                    assertEquals(
                        "Complete",
                        coordinator.cards.value
                            .single()
                            .status,
                    )
                    coordinator.send(draft.draftId, fixture.group) { }
                    val messages = fixture.group.messages(null)
                    assertEquals(1, messages.size)
                    assertEquals(DeliveryStatus.PUBLISHED, messages.single().deliveryStatus)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertNull(fixture.secrets.read(fixture.profile.id, checkNotNull(draft.descriptorSecretRef)))
                    val files = AttachmentFiles(fixture.context, fixture.key, fixture.client) { fixture.current }
                    val downloaded = files.download(messages.single().id, remote)
                    val expectedDigest = MessageDigest.getInstance("SHA-256").digest(bytes)
                    val actualDigest = MessageDigest.getInstance("SHA-256").digest(File(downloaded.path).readBytes())
                    assertArrayEquals(expectedDigest, actualDigest)
                }
            } finally {
                fixture.close()
            }
        }

    @Test fun queueInterruptionDoesNotResendAndAcceptedIdNeedsNoDescriptor() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val source = AttachmentSource.Bytes("one".toByteArray(), "same-name.txt", "text/plain")
                    val pending = fixture.client.attachments().create(source)
                    val remote = pending.remoteAttachment()
                    pending.upload()
                    val unknown = fixture.save(remote, SendPhase.QUEUEING)
                    val id = fixture.group.sendRemoteAttachment(remote, SendOptions(optimistic = true))
                    val accepted = fixture.save(remote, SendPhase.QUEUEING, id)
                    fixture.secrets.delete(fixture.profile.id, checkNotNull(accepted.descriptorSecretRef))
                    fixture.reopen()
                    val coordinator = fixture.coordinator()
                    coordinator.recover()
                    assertTrue(
                        coordinator.cards.value
                            .single { it.id == unknown.draftId }
                            .unknownOutcome,
                    )
                    assertFalse(
                        coordinator.cards.value
                            .single { it.id == unknown.draftId }
                            .canSend,
                    )
                    assertFalse(
                        coordinator.cards.value
                            .single { it.id == accepted.draftId }
                            .unavailable,
                    )
                    assertEquals(1, fixture.group.messages(null).size)
                    coordinator.send(accepted.draftId, fixture.group) { }
                    assertEquals(listOf(id), fixture.group.messages(null).map { it.id })
                    coordinator.discard(unknown.draftId)
                    assertTrue(File(fixture.client.attachments().localPath(remote)).isFile)
                }
            } finally {
                fixture.close()
            }
        }

    @Test fun expiredCompleteDraftIsUnavailableAndQueuesNothing() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start(age = 1uL)
                    val source = AttachmentSource.Bytes("expiry".toByteArray(), "expired.txt", "text/plain")
                    val pending = fixture.client.attachments().create(source)
                    val remote = pending.remoteAttachment()
                    val draft = fixture.save(remote)
                    pending.upload()
                    withTimeout(15_000) {
                        while (true) {
                            try {
                                fixture.client.attachments().pending(remote)
                                delay(200)
                            } catch (
                                error: XmtpException.Attachment,
                            ) {
                                if (!error.isExpiredDraft()) throw error
                                break
                            }
                        }
                    }
                    val coordinator = fixture.coordinator()
                    coordinator.recover()
                    assertTrue(
                        coordinator.cards.value
                            .single()
                            .unavailable,
                    )
                    assertFalse(
                        coordinator.cards.value
                            .single()
                            .canSend,
                    )
                    assertTrue(fixture.group.messages(null).isEmpty())
                    coordinator.discard(draft.draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                }
            } finally {
                fixture.close()
            }
        }

    @Test fun corruptOrMissingRemoteObjectNeverBecomesReadable() =
        runBlocking {
            val sender = AttachmentTestFixture()
            val receiver = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    sender.start()
                    receiver.start()
                    val source = AttachmentSource.Bytes("verified".toByteArray(), "verified.txt", "text/plain")
                    val pending = sender.client.attachments().create(source)
                    pending.upload()
                    val remote = pending.remoteAttachment()
                    val files = AttachmentFiles(receiver.context, receiver.key, receiver.client) { receiver.current }
                    assertEquals("verified", File(files.download("good", remote).path).readText())
                    val corrupt = remote.copy(contentDigest = "b".repeat(64))
                    try {
                        files.download("corrupt", corrupt)
                        fail("Digest mismatch must fail")
                    } catch (
                        error: XmtpException.Attachment,
                    ) {
                        val expected = AttachmentFailureCause.DIGEST_MISMATCH
                        assertEquals(expected, error.v2.cause)
                    }
                    assertFalse(files.isDownloaded("corrupt"))
                    assertFalse(File(receiver.client.attachments().localPath(corrupt)).exists())
                    val missing = remote.copy(url = remote.url + "-missing", contentDigest = "c".repeat(64))
                    try {
                        files.download("missing", missing)
                        fail("A missing object must fail")
                    } catch (
                        error: XmtpException.Attachment,
                    ) {
                        assertEquals(AttachmentFailureCause.NOT_FOUND, error.v2.cause)
                    }
                    assertFalse(files.isDownloaded("missing"))
                    val image = Bitmap.createBitmap(2048, 1024, Bitmap.Config.ARGB_8888)
                    val encoding = java.io.ByteArrayOutputStream()
                    image.compress(Bitmap.CompressFormat.PNG, 100, encoding)
                    val bytes = encoding.toByteArray()
                    image.recycle()
                    val photo =
                        sender.client.attachments().create(
                            AttachmentSource.Bytes(bytes, "image.bin", "application/octet-stream"),
                        )
                    photo.upload()
                    files.download("photo", photo.remoteAttachment())
                    val preview = checkNotNull(files.preview("photo"))
                    assertTrue(preview.width <= 1024 && preview.height <= 1024)
                    preview.recycle()
                }
            } finally {
                receiver.close()
                sender.close()
            }
        }

    @Test fun orphanIsUnassignedAndDiscardRemovesNativeFiles() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val firstSource = AttachmentSource.Bytes("one".toByteArray(), "same.txt", "text/plain")
                    val secondSource = AttachmentSource.Bytes("two".toByteArray(), "same.txt", "text/plain")
                    val one = fixture.client.attachments().create(firstSource)
                    val two = fixture.client.attachments().create(secondSource)
                    assertNotEquals(one.localPath(), two.localPath())
                    val coordinator = fixture.coordinator()
                    coordinator.recover()
                    assertEquals(2, coordinator.cards.value.size)
                    assertTrue(coordinator.cards.value.none { it.canSend })
                    val drafts = fixture.preferences.drafts(fixture.profile.id)
                    assertTrue(drafts.all { it.conversationKey.isEmpty() })
                    drafts.forEach { coordinator.discard(it.draftId) }
                    assertFalse(File(one.localPath()).exists())
                    assertFalse(File(two.localPath()).exists())
                    assertTrue(
                        fixture.client
                            .attachments()
                            .listPending()
                            .isEmpty(),
                    )
                }
            } finally {
                fixture.close()
            }
        }
}
