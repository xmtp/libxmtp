package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.SendCoordinator
import org.xmtp.android.example.messenger.SendPhase
import uniffi.xmtp_sdk.*

class AttachmentQueueAdmissionInstrumentedTest {
    @Test fun completeUploadSurvivesQueueScopeRejectionAndNeverRevivesRemovedDrafts() =
        runBlocking<Unit> {
            for (stage in listOf("inside-edit", "after-edit", "discarded", "reset")) {
                val fixture = AttachmentTestFixture()
                val entered = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>()
                try {
                    withTimeout(90_000) {
                        fixture.start()
                        val attachments = fixture.client.attachments()
                        val pending =
                            attachments.create(
                                AttachmentSource.Bytes(stage.toByteArray(), "retained.txt", "text/plain"),
                            )
                        pending.upload()
                        val remote = pending.remoteAttachment()
                        val draft = fixture.save(remote)
                        assertTrue(attachments.listPending().isEmpty())
                        val sends = SendCoordinator(fixture.preferences, { fixture.current })
                        val coordinator =
                            AttachmentDraftCoordinator(
                                fixture.key,
                                fixture.client,
                                fixture.paths,
                                fixture.preferences,
                                fixture.secrets,
                                sends,
                                { fixture.current },
                            )
                        var currentScreen = true
                        var edits = 0
                        if (stage == "inside-edit") {
                            fixture.preferences.beforeDraftAdmission = {
                                edits += 1
                                if (edits == 2) {
                                    entered.complete(Unit)
                                    release.await()
                                }
                            }
                        } else {
                            sends.afterQueueDraftCommit = {
                                entered.complete(Unit)
                                release.await()
                            }
                        }
                        val sending =
                            async {
                                runCatching { coordinator.send(draft.draftId, fixture.group, { currentScreen }) { } }
                            }
                        withTimeout(30_000) { entered.await() }
                        currentScreen = false
                        when (stage) {
                            "discarded" -> {
                                fixture.preferences.removeDraft(fixture.profile.id, draft.draftId)
                            }

                            "reset" -> {
                                fixture.current = false
                                fixture.preferences.removeProfile(fixture.profile.id)
                            }
                        }
                        release.complete(Unit)
                        val outcome = sending.await()
                        assertTrue(outcome.isFailure)
                        assertTrue(fixture.group.messages(null).isEmpty())
                        if (stage == "discarded" || stage == "reset") {
                            assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                        } else {
                            val refs = fixture.preferences.drafts(fixture.profile.id)
                            assertEquals("Keep the Complete draft at $stage", 1, refs.size)
                            val retained = refs.single()
                            assertEquals(draft.draftId, retained.draftId)
                            assertEquals(draft.descriptorSecretRef, retained.descriptorSecretRef)
                            assertEquals(SendPhase.UPLOADING, retained.phase)
                            assertNotNull(
                                fixture.secrets.read(fixture.profile.id, checkNotNull(retained.descriptorSecretRef)),
                            )
                            assertEquals(PendingAttachmentStatus.Complete, attachments.pending(remote).status())
                            currentScreen = true
                            coordinator.recover()
                            assertEquals(
                                "Complete",
                                coordinator.cards.value
                                    .single()
                                    .status,
                            )
                            coordinator.discard(retained.draftId)
                            assertTrue(fixture.preferences.drafts(fixture.profile.id).isEmpty())
                        }
                        println("ATTACHMENT_QUEUE_ADMISSION_PROOF stage=$stage no-message=true no-revival=true")
                    }
                } finally {
                    release.complete(Unit)
                    fixture.preferences.beforeDraftAdmission = {}
                    fixture.close()
                }
            }
        }
}
