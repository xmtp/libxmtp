package org.xmtp.android.example.messenger.attachments

import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.io.File

class AttachmentOwnershipInstrumentedTest {
    @Test fun completedSelectionOwnsPendingBeforeOldRecoveryAdmitsAnOrphan() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            val snapshot = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val attachments = fixture.client.attachments()
                    val coordinator = fixture.coordinator()
                    coordinator.afterRecoverySnapshot = {
                        snapshot.complete(Unit)
                        release.await()
                    }
                    val recovering = async { coordinator.recover() }
                    withTimeout(30_000) { snapshot.await() }
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    val authority = InstrumentationRegistry.getInstrumentation().context.packageName + ".file-source"
                    val source = Uri.parse("content://$authority/file?bytes=131073&length=0")
                    val selected = coordinator.select(fixture.context.contentResolver, source, fixture.group.id())
                    val ref = checkNotNull(selected.descriptorSecretRef)
                    val remoteBytes = checkNotNull(fixture.secrets.read(fixture.profile.id, ref))
                    val remote = AttachmentDescriptor.decode(remoteBytes)
                    release.complete(Unit)
                    recovering.await()
                    val drafts = fixture.preferences.drafts(fixture.profile.id)
                    assertEquals(listOf(selected), drafts)
                    assertEquals(listOf(selected.draftId), coordinator.cards.value.map { it.id })
                    assertEquals(fixture.group.id(), drafts.single().conversationKey)
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(remote).status())
                    val path = File(attachments.localPath(remote))
                    assertEquals(131073L, path.length())
                    assertArrayEquals(ByteArray(131073) { (it % 8192 % 251).toByte() }, path.readBytes())
                    assertEquals(listOf(remote), attachments.listPending().map { it.remoteAttachment() })
                    val otherSource = Uri.parse("content://$authority/file?bytes=17")
                    val other = coordinator.select(fixture.context.contentResolver, otherSource, fixture.group.id())
                    val otherRef = checkNotNull(other.descriptorSecretRef)
                    val otherBytes = checkNotNull(fixture.secrets.read(fixture.profile.id, otherRef))
                    val otherRemote = AttachmentDescriptor.decode(otherBytes)
                    coordinator.discard(selected.draftId)
                    assertFalse(path.exists())
                    assertEquals(listOf(other), fixture.preferences.drafts(fixture.profile.id))
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(otherRemote).status())
                    assertEquals(17L, File(attachments.localPath(otherRemote)).length())
                    coordinator.discard(other.draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertTrue(attachments.listPending().isEmpty())
                    println(
                        "ATTACHMENT_OWNERSHIP_PROOF stage=old-snapshot-selection-complete" +
                            " single-assigned-ref=true safe-discard=true",
                    )
                }
            } finally {
                release.complete(Unit)
                fixture.close()
            }
        }
}
