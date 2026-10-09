package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.SendPhase
import uniffi.xmtp_sdk.*
import java.io.File

class AttachmentDiscardInstrumentedTest {
    @Test fun discardChecksLateScreenAdmissionAndFinishesAdmittedCleanup() =
        runBlocking<Unit> {
            for (stage in listOf("before-delete", "after-delete", "reset", "accepted")) {
                val fixture = AttachmentTestFixture()
                val entered = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>()
                try {
                    withTimeout(90_000) {
                        fixture.start()
                        val attachments = fixture.client.attachments()
                        val pending =
                            attachments.create(
                                AttachmentSource.Bytes(stage.toByteArray(), "discard.txt", "text/plain"),
                            )
                        val remote = pending.remoteAttachment()
                        val accepted =
                            if (stage == "accepted") {
                                pending.upload()
                                fixture.group.sendRemoteAttachment(remote, SendOptions(optimistic = true))
                            } else {
                                null
                            }
                        val draft =
                            fixture.save(
                                remote,
                                if (accepted ==
                                    null
                                ) {
                                    SendPhase.DRAFT
                                } else {
                                    SendPhase.ACCEPTED
                                },
                                accepted,
                            )
                        val path = File(attachments.localPath(remote))
                        val ref = checkNotNull(draft.descriptorSecretRef)
                        val coordinator = fixture.coordinator()
                        coordinator.recover()
                        var screenCurrent = true
                        val pause: suspend () -> Unit = {
                            entered.complete(Unit)
                            release.await()
                        }
                        if (stage == "before-delete") {
                            coordinator.afterDiscardDescriptorRead = pause
                        } else {
                            coordinator.afterDiscardDelete = pause
                        }
                        var outcome: Result<Unit>? = null
                        val discarding =
                            launch {
                                outcome =
                                    runCatching { coordinator.discard(draft.draftId) { screenCurrent } }
                            }
                        withTimeout(30_000) { entered.await() }
                        if (stage == "before-delete") {
                            val overlap = runCatching { fixture.coordinator().send(draft.draftId, fixture.group) { } }
                            assertTrue(overlap.isFailure)
                            assertEquals(
                                "A marked discard cannot start an upload",
                                SendPhase.DRAFT,
                                fixture.preferences
                                    .drafts(fixture.profile.id)
                                    .single()
                                    .phase,
                            )
                            assertEquals(PendingAttachmentStatus.Waiting, pending.status())
                        } else if (stage != "accepted") {
                            assertFalse(path.exists())
                            assertTrue(attachments.listPending().isEmpty())
                        }
                        screenCurrent = false
                        if (stage == "reset") {
                            fixture.current = false
                            fixture.preferences.removeProfile(fixture.profile.id)
                            fixture.endClient()
                            fixture.paths.root.deleteRecursively()
                        }
                        if (stage == "after-delete") discarding.cancel()
                        release.complete(Unit)
                        discarding.join()
                        if (stage == "before-delete") {
                            assertTrue("Rejected navigation keeps SDK staging data", path.isFile)
                            assertArrayEquals(stage.toByteArray(), path.readBytes())
                            assertEquals(listOf(draft), fixture.preferences.drafts(fixture.profile.id))
                            assertNotNull(fixture.secrets.read(fixture.profile.id, ref))
                            assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(remote).status())
                            assertTrue(checkNotNull(outcome).isFailure)
                            coordinator.afterDiscardDescriptorRead = {}
                            coordinator.discard(draft.draftId)
                        } else {
                            assertTrue(
                                "Admitted deletion clears the persisted draft at $stage",
                                fixture.preferences.drafts(fixture.profile.id).isEmpty(),
                            )
                            assertNull(fixture.secrets.read(fixture.profile.id, ref))
                            if (stage != "reset") assertTrue(coordinator.cards.value.isEmpty())
                        }
                        if (stage == "accepted") {
                            assertTrue("Discarding an accepted reference keeps SDK data", path.isFile)
                            assertEquals(listOf(accepted), fixture.group.messages(null).map { it.id })
                        }
                        println("ATTACHMENT_DISCARD_PROOF stage=$stage late-admission=true no-revival=true")
                    }
                } finally {
                    release.complete(Unit)
                    fixture.close()
                }
            }
        }

    @Test fun actualSdkDeleteFailureDoesNotBlockSendAfterCoordinatorRecreation() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val attachments = fixture.client.attachments()
                    val bytes = "native deletion failure".toByteArray()
                    val pending = attachments.create(AttachmentSource.Bytes(bytes, "failure.txt", "text/plain"))
                    val remote = pending.remoteAttachment()
                    val draft = fixture.save(remote)
                    val path = File(attachments.localPath(remote))
                    val directory = checkNotNull(path.parentFile)
                    val backup = File(directory.parentFile, directory.name + "-discard-test")
                    assertTrue(directory.renameTo(backup))
                    directory.writeText("A regular file cannot be opened as the SDK key directory")
                    val failure =
                        try {
                            runCatching { fixture.coordinator().discard(draft.draftId) }.exceptionOrNull()
                        } finally {
                            assertTrue(directory.delete())
                            assertTrue(backup.renameTo(directory))
                        }
                    assertTrue("The actual SDK rejects key-directory removal", failure is XmtpException.Attachment)
                    assertEquals(listOf(draft), fixture.preferences.drafts(fixture.profile.id))
                    assertArrayEquals(bytes, path.readBytes())
                    assertEquals(PendingAttachmentStatus.Waiting, attachments.pending(remote).status())
                    val recreated = fixture.coordinator()
                    val sent = runCatching { recreated.send(draft.draftId, fixture.group) { } }
                    assertTrue("Failed discard cannot block send in a recreated coordinator: $sent", sent.isSuccess)
                    assertEquals(1, fixture.group.messages(null).size)
                    assertEquals(
                        DeliveryStatus.PUBLISHED,
                        fixture.group
                            .messages(null)
                            .single()
                            .deliveryStatus,
                    )
                    assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                    println("ATTACHMENT_DISCARD_PROOF stage=actual-sdk-delete-failure recreated-send=true")
                }
            } finally {
                fixture.close()
            }
        }
}
