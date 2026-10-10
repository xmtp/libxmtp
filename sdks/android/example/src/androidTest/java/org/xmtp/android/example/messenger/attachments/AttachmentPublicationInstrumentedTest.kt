package org.xmtp.android.example.messenger.attachments

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.Screen
import uniffi.xmtp_sdk.*
import java.util.UUID

class AttachmentPublicationInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (failure: TimeoutCancellationException) {
            val state = model.state.value
            val owner = model.session.active.value
            val drafts = owner?.let { model.session.preferences.drafts(it.key.profileId) }.orEmpty()
            println(
                "PUBLICATION_WAIT_DIAGNOSTIC stage=$stage screen=${state.screen} support=${state.features.attachments}",
            )
            println("PUBLICATION_WAIT_DIAGNOSTIC draft-phases=${drafts.map { it.phase }}")
            throw failure
        }
    }

    @Test fun actualRecoveryCardRetriesOnlyItsAcceptedMessageId() =
        runBlocking<Unit> {
            val lifecycle = AndroidStreamLifecycle.enabled
            val session = model.session
            val invalidated = session.onInvalidated
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until("availability") {
                    model.state.value.inbox == owner.client.inboxId() &&
                        model.state.value.features.attachments
                }
                model.foreground(false)
                session.onInvalidated = {}
                val chat =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Accepted file recovery"),
                    )
                val pending =
                    owner.client.attachments().create(
                        AttachmentSource.Bytes("accepted".toByteArray(), "accepted.txt", "text/plain"),
                    )
                pending.upload()
                val id = chat.sendRemoteAttachment(pending.remoteAttachment(), SendOptions(optimistic = true))
                val draftId = UUID.randomUUID().toString()
                val ref = "attachment-$draftId"
                val draft = SendDraftRef(draftId, chat.id(), ref, id, SendPhase.ACCEPTED)
                session.secrets.write(owner.key.profileId, ref, AttachmentDescriptor.encode(pending.remoteAttachment()))
                val retainedIds =
                    session.preferences
                        .drafts(owner.key.profileId)
                        .map { it.draftId }
                        .toSet()
                session.preferences.saveDraft(owner.key.profileId, draft)
                session.secrets.delete(owner.key.profileId, ref)
                assertEquals(
                    DeliveryStatus.UNPUBLISHED,
                    owner.client.conversations
                        .getMessageById(id)!!
                        .deliveryStatus,
                )
                model.dispatch(MessengerAction.Navigate(Screen.DRAFTS))
                until("drafts-screen") { model.state.value.screen == Screen.DRAFTS }
                model.featureRefresh(owner, null)
                compose.onNodeWithText("Retry publication").assertIsDisplayed().performClick()
                var published = false
                for (attempt in 0 until 100) {
                    if (owner.client.conversations
                            .getMessageById(id)!!
                            .deliveryStatus == DeliveryStatus.PUBLISHED
                    ) {
                        published = true
                        break
                    }
                    delay(50)
                }
                assertTrue("The recovery action publishes the stored MessageId", published)
                until("reference-cleanup") {
                    session.preferences
                        .drafts(owner.key.profileId)
                        .map { it.draftId }
                        .toSet() == retainedIds
                }
                assertEquals(listOf(id), chat.messages(null).map { it.id })
                assertEquals(Screen.DRAFTS, model.state.value.screen)
                compose.onNodeWithText("Retry publication").assertDoesNotExist()
                println(
                    "ATTACHMENT_PUBLICATION_PROOF stage=actual-recovery-card stored-id=true no-descriptor=true one-message=true",
                )
            } finally {
                session.onInvalidated = invalidated
                withContext(NonCancellable) {
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = lifecycle
                }
            }
        }
}
