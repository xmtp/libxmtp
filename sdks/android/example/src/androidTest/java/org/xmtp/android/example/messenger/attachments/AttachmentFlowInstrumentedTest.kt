package org.xmtp.android.example.messenger.attachments

import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.messenger.SendPhase
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class AttachmentFlowInstrumentedTest {
    @Test fun uriLengthHintsAndNamesDoNotControlPrivateCopy() = runBlocking {
        val fixture = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                fixture.start()
                val authority = InstrumentationRegistry.getInstrumentation().context.packageName + ".file-source"
                val coordinator = fixture.coordinator()
                for (hint in listOf("", "&length=0")) {
                    val uri = Uri.parse("content://$authority/file?bytes=131073$hint")
                    val draft = coordinator.select(fixture.context.contentResolver, uri, fixture.group.id())
                    val remote = AttachmentDescriptor.decode(checkNotNull(fixture.secrets.read(fixture.profile.id, checkNotNull(draft.descriptorSecretRef))))
                    assertEquals("../../same.bin", remote.filename)
                    assertEquals(131073L, File(fixture.client.attachments().localPath(remote)).length())
                    assertTrue(fixture.paths.temp.listFiles().orEmpty().isEmpty())
                    coordinator.discard(draft.draftId)
                    assertFalse(File(fixture.client.attachments().localPath(remote)).exists())
                }
                try {
                    PrivateFileStager.stage(fixture.context.contentResolver, Uri.parse("content://$authority/file?bytes=131073&length=0"), fixture.paths.temp, 65536uL)
                    fail("The actual stream must exceed the copy ceiling")
                } catch (_: IllegalArgumentException) { }
                assertTrue(fixture.paths.temp.listFiles().orEmpty().isEmpty())
            }
        } finally { fixture.close() }
    }

    @Test fun completeUploadRecoversAndPublishesOneAcceptedMessage() = runBlocking {
        val fixture = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                fixture.start()
                val bytes = ByteArray(131073) { (it % 251).toByte() }
                val pending = fixture.client.attachments().create(AttachmentSource.Bytes(bytes, "same-name.bin", "application/octet-stream"))
                val remote = pending.remoteAttachment()
                val draft = fixture.save(remote)
                pending.upload()
                assertTrue(fixture.client.attachments().listPending().isEmpty())
                fixture.reopen()
                val coordinator = fixture.coordinator()
                coordinator.recover()
                assertEquals("Complete", coordinator.cards.value.single().status)
                coordinator.send(draft.draftId, fixture.group) { }
                val messages = fixture.group.messages(null)
                assertEquals(1, messages.size)
                assertEquals(DeliveryStatus.PUBLISHED, messages.single().deliveryStatus)
                assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                assertNull(fixture.secrets.read(fixture.profile.id, checkNotNull(draft.descriptorSecretRef)))
                val files = AttachmentFiles(fixture.context, fixture.key, fixture.client) { fixture.current }
                val downloaded = files.download(messages.single().id, remote)
                assertArrayEquals(MessageDigest.getInstance("SHA-256").digest(bytes), MessageDigest.getInstance("SHA-256").digest(File(downloaded.path).readBytes()))
            }
        } finally { fixture.close() }
    }

    @Test fun queueInterruptionDoesNotResendAndAcceptedIdNeedsNoDescriptor() = runBlocking {
        val fixture = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                fixture.start()
                val pending = fixture.client.attachments().create(AttachmentSource.Bytes("one".toByteArray(), "same-name.txt", "text/plain"))
                val remote = pending.remoteAttachment()
                pending.upload()
                val unknown = fixture.save(remote, SendPhase.QUEUEING)
                val id = fixture.group.sendRemoteAttachment(remote, SendOptions(optimistic = true))
                val accepted = fixture.save(remote, SendPhase.QUEUEING, id)
                fixture.secrets.delete(fixture.profile.id, checkNotNull(accepted.descriptorSecretRef))
                fixture.reopen()
                val coordinator = fixture.coordinator()
                coordinator.recover()
                assertTrue(coordinator.cards.value.single { it.id == unknown.draftId }.unknownOutcome)
                assertFalse(coordinator.cards.value.single { it.id == unknown.draftId }.canSend)
                assertFalse(coordinator.cards.value.single { it.id == accepted.draftId }.unavailable)
                assertEquals(1, fixture.group.messages(null).size)
                coordinator.send(accepted.draftId, fixture.group) { }
                assertEquals(listOf(id), fixture.group.messages(null).map { it.id })
                coordinator.discard(unknown.draftId)
                assertTrue(File(fixture.client.attachments().localPath(remote)).isFile)
            }
        } finally { fixture.close() }
    }

    @Test fun expiredCompleteDraftIsUnavailableAndQueuesNothing() = runBlocking {
        val fixture = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                fixture.start(age = 1uL)
                val pending = fixture.client.attachments().create(AttachmentSource.Bytes("expiry".toByteArray(), "expired.txt", "text/plain"))
                val remote = pending.remoteAttachment()
                val draft = fixture.save(remote)
                pending.upload()
                withTimeout(15_000) {
                    while (true) {
                        try { fixture.client.attachments().pending(remote); delay(200) }
                        catch (error: XmtpException.Attachment) { if (!error.isExpiredDraft()) throw error; break }
                    }
                }
                val coordinator = fixture.coordinator()
                coordinator.recover()
                assertTrue(coordinator.cards.value.single().unavailable)
                assertFalse(coordinator.cards.value.single().canSend)
                assertTrue(fixture.group.messages(null).isEmpty())
                coordinator.discard(draft.draftId)
                assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
            }
        } finally { fixture.close() }
    }

    @Test fun corruptOrMissingRemoteObjectNeverBecomesReadable() = runBlocking {
        val sender = AttachmentTestFixture()
        val receiver = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                sender.start()
                receiver.start()
                val pending = sender.client.attachments().create(AttachmentSource.Bytes("verified".toByteArray(), "verified.txt", "text/plain"))
                pending.upload()
                val remote = pending.remoteAttachment()
                val files = AttachmentFiles(receiver.context, receiver.key, receiver.client) { receiver.current }
                assertEquals("verified", File(files.download("good", remote).path).readText())
                val corrupt = remote.copy(contentDigest = "b".repeat(64))
                try { files.download("corrupt", corrupt); fail("Digest mismatch must fail") }
                catch (error: XmtpException.Attachment) { assertEquals(AttachmentFailureCause.DIGEST_MISMATCH, error.v2.cause) }
                assertFalse(files.isDownloaded("corrupt"))
                assertFalse(File(receiver.client.attachments().localPath(corrupt)).exists())
                val missing = remote.copy(url = remote.url + "-missing", contentDigest = "c".repeat(64))
                try { files.download("missing", missing); fail("A missing object must fail") }
                catch (error: XmtpException.Attachment) { assertEquals(AttachmentFailureCause.NOT_FOUND, error.v2.cause) }
                assertFalse(files.isDownloaded("missing"))
            }
        } finally { receiver.close(); sender.close() }
    }

    @Test fun orphanIsUnassignedAndDiscardRemovesNativeFiles() = runBlocking {
        val fixture = AttachmentTestFixture()
        try {
            withTimeout(90_000) {
                fixture.start()
                val one = fixture.client.attachments().create(AttachmentSource.Bytes("one".toByteArray(), "same.txt", "text/plain"))
                val two = fixture.client.attachments().create(AttachmentSource.Bytes("two".toByteArray(), "same.txt", "text/plain"))
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
                assertTrue(fixture.client.attachments().listPending().isEmpty())
            }
        } finally { fixture.close() }
    }
}
