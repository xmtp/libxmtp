package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.io.File

class AttachmentRecoveryRaceInstrumentedTest {
    private val proofPrefix = "ATTACHMENT_RECOVERY_RACE_PROOF"

    @Test fun discardCannotBeUndoneByAnOldUploadPhaseSnapshot() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val attachments = fixture.client.attachments()
                    val pending =
                        attachments.create(
                            AttachmentSource.Bytes("phase snapshot".toByteArray(), "phase.txt", "text/plain"),
                        )
                    val remote = pending.remoteAttachment()
                    val draft = fixture.save(remote)
                    val coordinator = fixture.coordinator()
                    coordinator.beforeUploadPhaseSave = {
                        entered.complete(Unit)
                        release.await()
                    }
                    val sending = async { runCatching { coordinator.send(draft.draftId, fixture.group) { } } }
                    withTimeout(30_000) { entered.await() }
                    coordinator.discard(draft.draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertFalse(File(attachments.localPath(remote)).exists())
                    release.complete(Unit)
                    assertTrue(sending.await().isFailure)
                    assertTrue(
                        "Discarded draft cannot reappear after stale phase save",
                        fixture.preferences.drafts(fixture.profile.id).isEmpty(),
                    )
                    assertNull(fixture.secrets.read(fixture.profile.id, checkNotNull(draft.descriptorSecretRef)))
                    assertTrue(attachments.listPending().isEmpty())
                    assertTrue(fixture.group.messages(null).isEmpty())
                    val recreated = fixture.coordinator()
                    recreated.recover()
                    assertTrue(recreated.cards.value.isEmpty())
                    println("$proofPrefix stage=discard-before-upload-phase no-revival=true")
                }
            } finally {
                release.complete(Unit)
                fixture.close()
            }
        }

    @Test fun assignmentCannotReviveADiscardedNativeOrphan() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val attachments = fixture.client.attachments()
                    val bytes = "assign the actual orphan".toByteArray()
                    val pending = attachments.create(AttachmentSource.Bytes(bytes, "orphan.txt", "text/plain"))
                    val remote = pending.remoteAttachment()
                    val coordinator = fixture.coordinator()
                    coordinator.recover()
                    val draft = fixture.preferences.drafts(fixture.profile.id).single()
                    assertEquals("", draft.conversationKey)
                    val ref = checkNotNull(draft.descriptorSecretRef)
                    val descriptor = checkNotNull(fixture.secrets.read(fixture.profile.id, ref))
                    assertEquals(remote, AttachmentDescriptor.decode(descriptor))
                    val file = File(attachments.localPath(remote))
                    assertArrayEquals(bytes, file.readBytes())
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(remote).status())
                    assertEquals(1, attachments.listPending().size)
                    coordinator.afterAssignSnapshot = {
                        entered.complete(Unit)
                        release.await()
                    }
                    val assigning = async { runCatching { coordinator.assign(draft.draftId, fixture.group.id()) } }
                    withTimeout(30_000) { entered.await() }
                    coordinator.discard(draft.draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertNull(fixture.secrets.read(fixture.profile.id, ref))
                    assertFalse(file.exists())
                    assertTrue(attachments.listPending().isEmpty())
                    release.complete(Unit)
                    val result = assigning.await()
                    assertTrue(
                        "Assignment cannot recreate a discarded native orphan reference",
                        fixture.preferences.drafts(fixture.profile.id).isEmpty(),
                    )
                    assertEquals("The draft was discarded", result.exceptionOrNull()?.message)
                    assertNull(fixture.secrets.read(fixture.profile.id, ref))
                    assertFalse(file.exists())
                    assertTrue(attachments.listPending().isEmpty())
                    assertTrue(fixture.group.messages(null).isEmpty())
                    val recreated = fixture.coordinator()
                    recreated.recover()
                    assertTrue(recreated.cards.value.isEmpty())
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    val next = attachments.create(AttachmentSource.Bytes(bytes, "next.txt", "text/plain"))
                    val nextRemote = next.remoteAttachment()
                    recreated.recover()
                    val nextDraft = fixture.preferences.drafts(fixture.profile.id).single()
                    assertEquals("", nextDraft.conversationKey)
                    recreated.assign(nextDraft.draftId, fixture.group.id())
                    val assigned = fixture.preferences.drafts(fixture.profile.id).single()
                    assertEquals(nextDraft.draftId, assigned.draftId)
                    assertEquals(fixture.group.id(), assigned.conversationKey)
                    assertEquals(nextDraft.descriptorSecretRef, assigned.descriptorSecretRef)
                    assertArrayEquals(bytes, File(attachments.localPath(nextRemote)).readBytes())
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(nextRemote).status())
                    assertEquals(1, attachments.listPending().size)
                    recreated.discard(nextDraft.draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertNull(fixture.secrets.read(fixture.profile.id, checkNotNull(nextDraft.descriptorSecretRef)))
                    assertFalse(File(attachments.localPath(nextRemote)).exists())
                    assertTrue(attachments.listPending().isEmpty())
                    assertTrue(fixture.group.messages(null).isEmpty())
                    println(
                        "$proofPrefix stage=discard-before-assign no-revival=true native-delete=true valid-assign=true",
                    )
                }
            } finally {
                release.complete(Unit)
                fixture.close()
            }
        }

    @Test fun damagedDescriptorDoesNotBlockHealthyActionsOrGuessOrphanOwnership() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val attachments = fixture.client.attachments()
                    val ownedBytes = "damaged owner".toByteArray()
                    val orphanBytes = "real orphan".toByteArray()
                    val damagedRemote =
                        attachments
                            .create(
                                AttachmentSource.Bytes(ownedBytes, "same.txt", "text/plain"),
                            ).remoteAttachment()
                    val orphanRemote =
                        attachments
                            .create(
                                AttachmentSource.Bytes(orphanBytes, "same.txt", "text/plain"),
                            ).remoteAttachment()
                    val healthyRemote =
                        attachments
                            .create(
                                AttachmentSource.Bytes("healthy".toByteArray(), "same.txt", "text/plain"),
                            ).remoteAttachment()
                    val damaged = fixture.save(damagedRemote)
                    val healthy = fixture.save(healthyRemote)
                    fixture.secrets.write(fixture.profile.id, checkNotNull(damaged.descriptorSecretRef), byteArrayOf(0))
                    val coordinator = fixture.coordinator()
                    val recovery = runCatching { coordinator.recover() }
                    val failureMessage = "Damaged descriptor cannot stop healthy recovery: $recovery"
                    assertTrue(failureMessage, recovery.isSuccess)
                    assertTrue(
                        coordinator.cards.value
                            .single { it.id == damaged.draftId }
                            .unavailable,
                    )
                    assertEquals(
                        "Do not create an owner while saved ownership is unreadable",
                        setOf(damaged.draftId, healthy.draftId),
                        fixture.preferences
                            .drafts(fixture.profile.id)
                            .map { it.draftId }
                            .toSet(),
                    )
                    assertEquals(3, attachments.listPending().size)
                    coordinator.send(healthy.draftId, fixture.group) { }
                    val messages = fixture.group.messages(null)
                    assertEquals(1, messages.size)
                    assertEquals(DeliveryStatus.PUBLISHED, messages.single().deliveryStatus)
                    coordinator.discard(damaged.draftId)
                    assertArrayEquals(ownedBytes, File(attachments.localPath(damagedRemote)).readBytes())
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(damagedRemote).status())
                    coordinator.recover()
                    val orphans = fixture.preferences.drafts(fixture.profile.id)
                    assertEquals(2, orphans.size)
                    assertTrue(orphans.all { it.conversationKey.isEmpty() })
                    val records =
                        orphans.associateBy {
                            val ref = checkNotNull(it.descriptorSecretRef)
                            val bytes = checkNotNull(fixture.secrets.read(fixture.profile.id, ref))
                            AttachmentDescriptor.decode(bytes)
                        }
                    assertEquals(setOf(damagedRemote, orphanRemote), records.keys)
                    coordinator.discard(records.getValue(damagedRemote).draftId)
                    assertFalse(File(attachments.localPath(damagedRemote)).exists())
                    assertArrayEquals(orphanBytes, File(attachments.localPath(orphanRemote)).readBytes())
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(orphanRemote).status())
                    coordinator.discard(records.getValue(orphanRemote).draftId)
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    assertTrue(attachments.listPending().isEmpty())
                    println("$proofPrefix stage=damaged-descriptor")
                    println("ATTACHMENT_RECOVERY_STATE healthy-send=true no-duplicate-owner=true")
                }
            } finally {
                fixture.close()
            }
        }
}
