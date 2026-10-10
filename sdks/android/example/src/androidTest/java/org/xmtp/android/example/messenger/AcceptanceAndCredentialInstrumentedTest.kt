package org.xmtp.android.example.messenger

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.text.InputType
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger

class AcceptanceAndCredentialInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    @Test fun syntheticProofCaptureStaysPrivateAndOutsideMediaStore() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val name = "b-private-capture-test"
        val bitmap = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888)
        bitmap.eraseColor(android.graphics.Color.BLUE)
        var saved: java.io.File? = null
        try {
            saved = writeProofScreenshot(context, name, bitmap)
            assertEquals(
                java.io.File(context.filesDir, "xmtp-messenger-proof/$name.png").canonicalFile,
                saved.canonicalFile,
            )
            assertEquals(8, BitmapFactory.decodeFile(saved.absolutePath).width)
            context.contentResolver
                .query(
                    MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                    arrayOf(MediaStore.Images.Media._ID),
                    "${MediaStore.Images.Media.DISPLAY_NAME} = ?",
                    arrayOf("$name.png"),
                    null,
                ).use { rows ->
                    assertNotNull(rows)
                    assertEquals("The synthetic capture must not be a shared image", 0, rows!!.count)
                }
            println("CAPTURE_PROOF stage=synthetic-png-private-file-no-mediastore-row")
        } finally {
            bitmap.recycle()
            saved?.delete()
            context.contentResolver.delete(
                MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                "${MediaStore.Images.Media.DISPLAY_NAME} = ?",
                arrayOf("$name.png"),
            )
        }
    }

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun chat(): Pair<ActiveSession, Group> {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        val backend = BuildConfig.XMTP_BACKEND_URL
        model.session.connect(backend, "", localAttachmentNetwork(backend))
        val owner = checkNotNull(model.session.active.value)
        val options = CreateGroupOptions(name = "Acceptance commit")
        val group = owner.client.conversations.createGroup(emptyList(), options)
        model.dispatch(MessengerAction.OpenConversation(group.id()))
        until("current chat") { model.state.value.conversationId == group.id() && !model.state.value.busy }
        model.foreground(false)
        return owner to group
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.session.preferences.beforeDraftCommit = {}
            model.onQueuedActionFinished = {}
            model.sends.messageRead = { client, id -> client.conversations.getMessageById(id) }
            model.inspectBackend = { url ->
                val source = BackendSource.Options(BackendOptions(url = url))
                SDKClient.fetchServerConfiguration(source)
            }
            if (model.session.active.value != null) model.session.deleteAccount()
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    private suspend fun failAcceptedCommit(): CompletableDeferred<Unit> {
        val calls = AtomicInteger()
        val completed = CompletableDeferred<Unit>()
        model.session.preferences.beforeDraftCommit = {
            if (calls.incrementAndGet() == 2) throw IOException("Accepted reference write failed")
        }
        model.onQueuedActionFinished = { if (it is MessengerAction.SendText) completed.complete(Unit) }
        return completed
    }

    @Test fun acceptedCommitFailureKeepsComposerAndRecoversOnlyItsNativeId() =
        runBlocking {
            try {
                val (owner, group) = chat()
                val completed = failAcceptedCommit()
                compose.onNodeWithText("Message").performTextInput("SDK accepted body")
                compose.onNodeWithText("Send").performClick()
                until("failed accepted commit action finished") { completed.isCompleted }
                assertTrue(
                    model.state.value.error
                        ?.contains("Accepted reference write failed") == true,
                )
                assertFalse(
                    model.state.value.textSendResult
                        ?.accepted == true,
                )
                compose.onNode(hasSetTextAction() and hasText("SDK accepted body")).assertExists()
                compose.onNodeWithText("Send").assertIsNotEnabled()
                val selection = publishedSelection().copy(deliveryStatus = DeliveryStatus.UNPUBLISHED)
                val queued = Conversation.Group(group).messages(selection)
                assertEquals("The SDK accepted exactly one row", 1, queued.size)
                val id = queued.single().id
                assertEquals("SDK accepted body", queued.single().toRow(owner.client.inboxId()).text)
                val draft =
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .single()
                assertEquals(SendPhase.QUEUEING, draft.phase)
                assertNull(draft.acceptedMessageId)
                assertTrue(model.sends.knowsAccepted(draft.draftId))

                // A newer edit must survive acknowledgment of the older accepted request.
                val composer = compose.onNode(hasSetTextAction() and hasText("SDK accepted body"))
                composer.performTextReplacement("new draft edit")
                // Only this hook returns null. The SDK row above and below is real.
                val lookups = AtomicInteger()
                model.sends.messageRead = { _, _ ->
                    lookups.incrementAndGet()
                    null
                }
                val refreshFinished = CompletableDeferred<Unit>()
                model.onQueuedActionFinished = {
                    if (it == MessengerAction.Refresh) refreshFinished.complete(Unit)
                }
                model.session.preferences.beforeDraftCommit = {}
                model.dispatch(MessengerAction.Refresh)
                until("acceptance refresh finished with forced null lookup") { refreshFinished.isCompleted }
                val recovered =
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .single()
                assertEquals(
                    "The independent SDK lookup still finds this row",
                    id,
                    owner.client.conversations
                        .getMessageById(id)
                        ?.id,
                )
                println("ACCEPTANCE_PROOF stage=actual-sdk-row-present-forced-null-hook phase=${recovered.phase}")
                assertEquals(
                    "The SDK-issued ID must survive an unavailable projection",
                    id,
                    recovered.acceptedMessageId,
                )
                assertEquals(SendPhase.ACCEPTED, recovered.phase)
                assertEquals("Accepted-ID commit must not require a message lookup", 0, lookups.get())
                until("verified accepted reference and request acknowledgment") {
                    model.state.value.textSendResult
                        ?.accepted == true &&
                        model.session.preferences
                            .drafts(owner.key.profileId)
                            .singleOrNull()
                            ?.acceptedMessageId == id
                }
                compose.onNode(hasSetTextAction() and hasText("new draft edit")).assertExists()
                compose.onNodeWithText("Send").assertIsEnabled()
                assertEquals(listOf(id), Conversation.Group(group).messages(selection).map { it.id })
                model.sends.messageRead = { client, messageId -> client.conversations.getMessageById(messageId) }
                model.dispatch(MessengerAction.RetrySend(id))
                until("retained native ID published") {
                    owner.client.conversations
                        .getMessageById(id)
                        ?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                assertEquals(listOf(id), Conversation.Group(group).messages(publishedSelection()).map { it.id })
                assertEquals(0uL, Conversation.Group(group).countMessages(selection))
                println("ACCEPTANCE_PROOF stage=forced-null-lookup-commit-verified-id-no-duplicate")
            } finally {
                cleanup()
            }
        }

    @Test fun acceptedReferenceCannotReviveAnActuallyDiscardedOrResetDraft() =
        runBlocking {
            try {
                val (owner, group) = chat()
                val completed = failAcceptedCommit()
                compose.onNodeWithText("Message").performTextInput("Discarded accepted reference")
                compose.onNodeWithText("Send").performClick()
                until("accepted SDK row with failed app reference") { completed.isCompleted }
                val draft =
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .single()
                val selection = publishedSelection().copy(deliveryStatus = DeliveryStatus.UNPUBLISHED)
                val id =
                    Conversation
                        .Group(group)
                        .messages(selection)
                        .single()
                        .id
                val accepted = draft.copy(phase = SendPhase.ACCEPTED, acceptedMessageId = id)
                model.session.preferences.beforeDraftCommit = {}
                model.dispatch(MessengerAction.DiscardUnknownSend(draft.draftId))
                until("actual discarded draft") {
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty()
                }
                assertFalse(
                    "An accepted update must not insert a discarded reference",
                    model.session.preferences.saveAcceptedDraft(owner.key.profileId, accepted),
                )
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty(),
                )
                assertNotNull(owner.client.conversations.getMessageById(id))
                model.sends.recoverAccepted(owner.key)
                assertFalse("Explicit discard clears the staged acceptance", model.sends.knowsAccepted(draft.draftId))
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty(),
                )
                model.session.deleteAccount()
                assertFalse(
                    "An accepted update must not insert a reset reference",
                    model.session.preferences.saveAcceptedDraft(owner.key.profileId, accepted),
                )
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty(),
                )
                println("ACCEPTANCE_PROOF stage=real-accepted-id-discard-and-reset-no-reference-revival")
            } finally {
                cleanup()
            }
        }

    @Test fun credentialUsesEffectiveAndroidPasswordInputType() =
        runBlocking {
            try {
                model.session.signOut()
                until("Start") { model.state.value.screen == Screen.START }
                val config =
                    SDKClient.fetchServerConfiguration(
                        BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    )
                // The field uses a scripted public auth capability. No authenticated login is claimed.
                model.inspectBackend = { config.copy(auth = config.auth.copy(enabled = true)) }
                model.dispatch(MessengerAction.InspectBackend(BuildConfig.XMTP_BACKEND_URL))
                until("credential field admitted") { model.state.value.credentialsRequiredFor != null }
                compose.onNodeWithText("Credential").performClick()
                var types = emptyList<Int>()
                until("effective Android password editor") {
                    val descriptor =
                        InstrumentationRegistry
                            .getInstrumentation()
                            .uiAutomation
                            .executeShellCommand("dumpsys input_method")
                    val stream = ParcelFileDescriptor.AutoCloseInputStream(descriptor)
                    val dump = stream.bufferedReader().use { it.readText() }
                    val inputs = Regex("inputType=0x([0-9a-fA-F]+)").findAll(dump)
                    types = inputs.map { it.groupValues[1].toInt(16) }.toList()
                    types.any { (it and InputType.TYPE_MASK_VARIATION) == InputType.TYPE_TEXT_VARIATION_PASSWORD }
                }
                assertTrue(
                    types.any {
                        (it and InputType.TYPE_MASK_CLASS) == InputType.TYPE_CLASS_TEXT &&
                            (it and InputType.TYPE_MASK_VARIATION) == InputType.TYPE_TEXT_VARIATION_PASSWORD
                    },
                )
                println("CREDENTIAL_PROOF stage=effective-ime-password-type types=${types.map { it.toString(16) }}")
            } finally {
                cleanup()
            }
        }
}
