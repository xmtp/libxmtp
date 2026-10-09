package org.xmtp.android.example.messenger.attachments

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

/** Run alone. These tests control the shared backend proxy. */
@RunWith(AndroidJUnit4::class)
class AttachmentInterruptionInstrumentedTest {
    @Test fun discardDuringUploadStopsNativeWorkAndPreventsQueue() = runBlocking {
        val proxy = HeldUploadProxy()
        val fixture = AttachmentTestFixture()
        try {
            proxy.release(); proxy.enabled(true)
            withTimeout(120_000) {
                fixture.start(backend = proxy.backend)
                val pending = fixture.client.attachments().create(AttachmentSource.Bytes("held".toByteArray(), "held.txt", "text/plain"))
                val remote = pending.remoteAttachment()
                val draft = fixture.save(remote)
                val path = pending.localPath()
                val coordinator = fixture.coordinator()
                val events = fixture.client.events(EventFilter(kinds = listOf(EventKind.ATTACHMENT_UPLOAD_STARTED)))
                val started = async { events.first() }
                proxy.hold()
                val sending = async { runCatching { coordinator.send(draft.draftId, fixture.group) { } } }
                withTimeout(10_000) { started.await() }
                assertEquals(PendingAttachmentStatus.Uploading, pending.status())
                coordinator.discard(draft.draftId)
                proxy.release()
                assertTrue(withTimeout(30_000) { sending.await() }.isFailure)
                assertFalse(File(path).exists())
                assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                assertTrue(fixture.client.attachments().listPending().isEmpty())
                assertTrue(fixture.group.messages(null).isEmpty())
            }
        } finally {
            withContext(NonCancellable) { proxy.release(); proxy.enabled(true); fixture.close() }
        }
    }

    @Test fun uploadFailureRetainsItsDraftAndRetryQueuesOneMessage() = runBlocking {
        val proxy = HeldUploadProxy()
        val fixture = AttachmentTestFixture()
        try {
            proxy.release(); proxy.enabled(true)
            withTimeout(180_000) {
                fixture.start(backend = proxy.backend)
                val pending = fixture.client.attachments().create(AttachmentSource.Bytes("retry".toByteArray(), "retry.txt", "text/plain"))
                val draft = fixture.save(pending.remoteAttachment())
                val coordinator = fixture.coordinator()
                val events = fixture.client.events(EventFilter(kinds = listOf(EventKind.ATTACHMENT_UPLOAD_STARTED)))
                val started = async { events.first() }
                proxy.hold()
                val sending = async { runCatching { coordinator.send(draft.draftId, fixture.group) { } } }
                withTimeout(10_000) { started.await() }
                proxy.enabled(false)
                proxy.release()
                assertTrue(withTimeout(90_000) { sending.await() }.isFailure)
                assertEquals(draft.draftId, fixture.preferences.drafts(fixture.profile.id).single().draftId)
                assertTrue(pending.status() is PendingAttachmentStatus.Failed)
                assertTrue(fixture.group.messages(null).isEmpty())
                proxy.enabled(true)
                coordinator.send(draft.draftId, fixture.group) { }
                assertEquals(1, fixture.group.messages(null).size)
                assertEquals(DeliveryStatus.PUBLISHED, fixture.group.messages(null).single().deliveryStatus)
            }
        } finally {
            withContext(NonCancellable) { proxy.release(); proxy.enabled(true); fixture.close() }
        }
    }
}
