package org.xmtp.android.example.messenger

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.conversations.CreateConversationAction
import org.xmtp.android.example.shared.feature.conversations.CreateStep
import uniffi.xmtp_sdk.*
import java.util.UUID

/** Real handles enter through the recipient fields and reach the intended native inboxes. */
class RecipientHandlesInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    private val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val peers = mutableListOf<SDKClient>()
    private var lifecycle = true
    private val recipientLabel = "Handle, Ethereum address, or inbox ID"

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError(stage + ": " + model.state.value.error, error)
        }
    }

    @Before fun connect() =
        runBlocking {
            lifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            model.session.signOut()
            model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(model.session.active.value)
            until("Session did not open") { model.state.value.inbox == owner.client.inboxId() }
        }

    @After fun close() =
        runBlocking {
            try {
                withContext(NonCancellable) {
                    try {
                        peers.forEach { it.end() }
                    } finally {
                        if (model.session.active.value != null) model.session.deleteAccount()
                    }
                }
            } finally {
                AndroidStreamLifecycle.enabled = lifecycle
            }
        }

    private suspend fun peer(): SDKClient =
        SDKClient
            .create(
                compose.activity,
                generateLocalSigner(),
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    storage = StorageOptions(location = StorageLocation.InMemory),
                    deviceSync = false,
                ),
            ).also(peers::add)

    private suspend fun claim(
        client: SDKClient,
        coded: Boolean = false,
    ): String {
        val name =
            "peer" +
                UUID
                    .randomUUID()
                    .toString()
                    .replace("-", "")
                    .take(20)
        val handle = name + if (coded) "#2341" else ""
        client.claimHandle(handle)
        return handle
    }

    private fun button(label: String): SemanticsNodeInteraction {
        val matcher = hasText(label) and hasClickAction()
        if (model.state.value.screen == Screen.CONVERSATION_SETTINGS) {
            compose.onNodeWithTag("conversation-settings").performScrollToNode(matcher)
        }
        return compose.onNode(matcher)
    }

    private suspend fun create(
        input: String,
        expected: Set<String>,
        group: Boolean = false,
        handles: Map<String, String> = emptyMap(),
    ): Conversation {
        model.createConversation.clear()
        model.dispatch(MessengerAction.Navigate(Screen.CREATE))
        until("Create screen did not open") { model.state.value.screen == Screen.CREATE }
        model.createConversation.dispatch(CreateConversationAction.ChangeMode(group))
        val field = if (group) compose.onNodeWithText(recipientLabel) else compose.onNodeWithTag("dm-recipient")
        field.performTextReplacement(input)
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        if (group) {
            button("Check and add").performScrollTo().performClick()
            until("Input did not resolve to its claimed inbox") {
                !model.createConversation.state.value.busy &&
                    model.createConversation.state.value.selectedRecipients
                        .map { it.inboxId }
                        .toSet() == expected &&
                    model.createConversation.state.value.failure == null
            }
            handles.forEach { (inbox, handle) ->
                assertEquals(
                    handle,
                    model.createConversation.state.value.selectedRecipients
                        .single { it.inboxId == inbox }
                        .title,
                )
                compose.onNodeWithText(handle).assertExists()
            }
            button("Continue").performScrollTo().performClick()
            until("Review did not open") { model.createConversation.state.value.step == CreateStep.REVIEW }
            button("Create group").performScrollTo().performClick()
        } else {
            compose.onNodeWithText("Check and add").assertDoesNotExist()
            compose.onNodeWithText("Selected ·", substring = true).assertDoesNotExist()
            button("Continue").assertIsEnabled().performClick()
            until("DM Continue did not finish") {
                model.state.value.screen == Screen.TIMELINE ||
                    model.createConversation.state.value.step == CreateStep.REVIEW ||
                    model.createConversation.state.value.failure != null
            }
            assertEquals("DM Continue skips review", CreateStep.RECIPIENTS, model.createConversation.state.value.step)
            assertNull(model.createConversation.state.value.failure)
        }
        until("Created conversation did not open") {
            model.state.value.screen == Screen.TIMELINE && model.state.value.conversationId != null
        }
        val owner = checkNotNull(model.session.active.value)
        return checkNotNull(owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)))
    }

    @Test fun directMessagesAcceptAddressAndInboxIdWithOneContinue() =
        runBlocking {
            val target = peer()
            for (input in listOf(target.identity().identifier, target.inboxId())) {
                val conversation = create(input, setOf(target.inboxId()))
                assertEquals(target.inboxId(), (conversation as Conversation.Dm).dm.peerInboxId())
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("Chats did not reopen") { model.state.value.screen == Screen.CONVERSATIONS }
            }
        }

    @Test fun directMessagesCanChoosePeopleFromChatsAndReplaceTheSelection() =
        runBlocking {
            val first = peer()
            val firstHandle = claim(first)
            val second = peer()
            val secondHandle = claim(second, coded = true)
            val typed = peer()
            for (target in listOf(first, second)) {
                val chat = create(target.inboxId(), setOf(target.inboxId())) as Conversation.Dm
                assertEquals(target.inboxId(), chat.dm.peerInboxId())
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("Chats did not reopen") { model.state.value.screen == Screen.CONVERSATIONS }
            }

            suspend fun openPicker() {
                model.createConversation.clear()
                compose.onNodeWithContentDescription("New message").performClick()
                until("Saved chat participants did not load") {
                    model.createConversation.state.value.knownRecipients
                        .map { it.inboxId }
                        .containsAll(listOf(first.inboxId(), second.inboxId()))
                }
                compose.onNodeWithText("People from your chats").performScrollTo().assertIsDisplayed()
            }

            fun person(handle: String) = compose.onNode(hasText(handle) and hasClickAction())

            suspend fun openedPeer(): String {
                button("Continue").assertIsEnabled().performClick()
                until("Picked recipient did not open directly") {
                    model.state.value.screen == Screen.TIMELINE ||
                        model.createConversation.state.value.step == CreateStep.REVIEW ||
                        model.createConversation.state.value.failure != null
                }
                assertEquals("Picked DM skips review", CreateStep.RECIPIENTS, model.createConversation.state.value.step)
                assertNull(model.createConversation.state.value.failure)
                until("Picked DM timeline did not open") { model.state.value.screen == Screen.TIMELINE }
                val owner = checkNotNull(model.session.active.value)
                val chat =
                    checkNotNull(
                        owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)),
                    ) as Conversation.Dm
                return checkNotNull(chat.dm.peerInboxId())
            }

            openPicker()
            button("Continue").assertIsNotEnabled()
            person(firstHandle).assertIsNotSelected().performScrollTo().performClick()
            compose.onNodeWithTag("dm-recipient").assertTextContains(first.inboxId())
            person(firstHandle).assertIsSelected()
            person(secondHandle).assertIsNotSelected().performScrollTo().performClick()
            compose.onNodeWithTag("dm-recipient").assertTextContains(second.inboxId())
            person(firstHandle).assertIsNotSelected()
            person(secondHandle).assertIsSelected()
            button("Group").performScrollTo().performClick()
            button("Direct message").performScrollTo().performClick()
            person(secondHandle).assertIsSelected()
            compose.onNodeWithText("Check and add").assertDoesNotExist()
            compose.onNodeWithText("Selected ·", substring = true).assertDoesNotExist()
            assertEquals("The most recently picked person receives the DM", second.inboxId(), openedPeer())

            model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
            until("Chats did not reopen") { model.state.value.screen == Screen.CONVERSATIONS }
            openPicker()
            compose.onNodeWithTag("dm-recipient").performScrollTo().performTextReplacement("not a handle")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Continue").performClick()
            until("Invalid recipient did not show an error") {
                !model.createConversation.state.value.busy && model.createConversation.state.value.failure != null
            }
            person(firstHandle).performScrollTo().performClick()
            assertNull(model.createConversation.state.value.failure)
            compose.onNodeWithTag("dm-recipient").assertTextContains(first.inboxId())
            compose.onNodeWithTag("dm-recipient").performScrollTo().performTextReplacement(typed.inboxId())
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            person(firstHandle).assertIsNotSelected()
            person(secondHandle).assertIsNotSelected()
            assertEquals("Typed input replaces the picked recipient", typed.inboxId(), openedPeer())
        }

    @Test fun directMessageErrorsStayInlineAndEditingAllowsKeyboardRetry() =
        runBlocking {
            val target = peer()
            model.createConversation.clear()
            model.dispatch(MessengerAction.Navigate(Screen.CREATE))
            until("Create screen did not open") { model.state.value.screen == Screen.CREATE }
            button("Continue").assertIsNotEnabled()
            val missing =
                "missing" +
                    UUID
                        .randomUUID()
                        .toString()
                        .replace("-", "")
                        .take(20)
            for (input in listOf("not a handle", target.inboxId() + ", " + target.inboxId(), missing)) {
                compose.onNodeWithTag("dm-recipient").performTextReplacement(input)
                androidx.test.espresso.Espresso
                    .closeSoftKeyboard()
                assertNull(model.createConversation.state.value.failure)
                button("Continue").assertIsEnabled().performClick()
                until("Invalid recipient did not show an error") {
                    !model.createConversation.state.value.busy && model.createConversation.state.value.failure != null
                }
                assertEquals(Screen.CREATE, model.state.value.screen)
                assertEquals(CreateStep.RECIPIENTS, model.createConversation.state.value.step)
                compose
                    .onNodeWithText(
                        checkNotNull(model.createConversation.state.value.failure).message,
                    ).assertExists()
                compose.onNodeWithText("Check and add").assertDoesNotExist()
                compose.onNodeWithText("Start conversation").assertDoesNotExist()
                compose.onNodeWithText("Selected ·", substring = true).assertDoesNotExist()
            }
            compose.onNodeWithTag("dm-recipient").performTextReplacement("")
            assertNull(model.createConversation.state.value.failure)
            button("Continue").assertIsNotEnabled()
            compose.onNodeWithTag("dm-recipient").performTextReplacement(target.inboxId())
            compose.onNodeWithTag("dm-recipient").performImeAction()
            until("Keyboard retry did not open the DM") {
                model.state.value.screen == Screen.TIMELINE || model.createConversation.state.value.failure != null
            }
            assertNull(model.createConversation.state.value.failure)
            val owner = checkNotNull(model.session.active.value)
            val conversation =
                checkNotNull(owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)))
            assertEquals(target.inboxId(), (conversation as Conversation.Dm).dm.peerInboxId())
        }

    @Test fun switchingModesKeepsSeparateDraftsAndGroupOptions() =
        runBlocking {
            val first = peer()
            val second = peer()
            model.createConversation.clear()
            model.dispatch(MessengerAction.Navigate(Screen.CREATE))
            until("Create screen did not open") { model.state.value.screen == Screen.CREATE }
            button("Group").performClick()
            compose.onNodeWithText(recipientLabel).performTextReplacement(first.inboxId() + ", " + second.inboxId())
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Check and add").performScrollTo().performClick()
            until("Group recipients did not resolve") {
                !model.createConversation.state.value.busy &&
                    model.createConversation.state.value.selectedRecipients.size == 2
            }
            button("Continue").performScrollTo().performClick()
            until("Group options did not open") { model.createConversation.state.value.step == CreateStep.REVIEW }
            compose.onNodeWithText("Group name").performScrollTo().performTextReplacement("Friends")
            compose.onNodeWithText("Description (optional)").performScrollTo().performTextReplacement("Weekend plans")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Advanced options").performScrollTo().performClick()
            button("Admins only").performScrollTo().performClick()
            button("Direct message").performScrollTo().performClick()
            assertEquals(
                "",
                compose
                    .onNodeWithTag("dm-recipient")
                    .fetchSemanticsNode()
                    .config[SemanticsProperties.EditableText]
                    .text,
            )
            compose.onNodeWithText("Group name").assertDoesNotExist()
            compose.onNodeWithTag("dm-recipient").performTextReplacement(first.inboxId())
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Group").performClick()
            val groupDraft = model.createConversation.state.value
            assertEquals(CreateStep.REVIEW, groupDraft.step)
            assertEquals("Friends", groupDraft.name)
            assertEquals("Weekend plans", groupDraft.description)
            assertTrue(groupDraft.adminOnly)
            assertEquals(
                setOf(first.inboxId(), second.inboxId()),
                groupDraft.selectedRecipients.map { it.inboxId }.toSet(),
            )
            button("Direct message").performScrollTo().performClick()
            assertEquals(
                first.inboxId(),
                compose
                    .onNodeWithTag("dm-recipient")
                    .fetchSemanticsNode()
                    .config[SemanticsProperties.EditableText]
                    .text,
            )
            button("Continue").performClick()
            until("The separate DM draft did not open") { model.state.value.screen == Screen.TIMELINE }
            val owner = checkNotNull(model.session.active.value)
            val dm =
                checkNotNull(
                    owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)),
                ) as Conversation.Dm
            assertEquals(first.inboxId(), dm.dm.peerInboxId())
            model.dispatch(MessengerAction.Navigate(Screen.CREATE))
            until("Create screen did not reopen") { model.state.value.screen == Screen.CREATE }
            button("Group").performClick()
            button("Create group").performScrollTo().performClick()
            until("The saved group draft did not open") { model.state.value.screen == Screen.TIMELINE }
            val group =
                (
                    checkNotNull(
                        owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)),
                    ) as Conversation.Group
                ).group
            assertEquals("Friends", group.state().name)
            assertEquals("Weekend plans", group.state().description)
            assertEquals(GroupPolicyType.ADMIN_ONLY, group.state().permissions.policyType)
            assertTrue(group.members().map { it.inboxId }.containsAll(listOf(first.inboxId(), second.inboxId())))
        }

    @Test fun directMessagesResolveBareAndCodedHandles() =
        runBlocking {
            for (coded in listOf(false, true)) {
                val target = peer()
                val handle = claim(target, coded)
                val conversation =
                    create(
                        " @" + handle.uppercase() + " ",
                        setOf(target.inboxId()),
                        handles =
                            mapOf(target.inboxId() to handle),
                    )
                until("The conversation header did not show the current handle") {
                    model.chat.state.value.title ==
                        "@" + handle
                }
                compose.onNodeWithText("@" + handle).assertExists()
                assertEquals(target.inboxId(), (conversation as Conversation.Dm).dm.peerInboxId())
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("Chats did not reopen") { model.state.value.screen == Screen.CONVERSATIONS }
                model.conversations.refresh()
                until("The conversation list did not show the current handle") {
                    model.conversations.state.value.rows
                        .any { it.id == conversation.id() && it.title == "@" + handle }
                }
                compose.onNodeWithText("@" + handle).assertExists()
            }
        }

    @Test fun groupsResolveMixedRecipientsAndAddAHandleWithoutLeavingItPending() =
        runBlocking {
            val byHandle = peer()
            val handle = claim(byHandle)
            val byAddress = peer()
            val byInbox = peer()
            val expected = setOf(byHandle.inboxId(), byAddress.inboxId(), byInbox.inboxId())
            val conversation =
                create(
                    listOf(
                        handle,
                        byAddress.identity().identifier,
                        byInbox.inboxId(),
                        byHandle.inboxId(),
                    ).joinToString(", "),
                    expected,
                    group = true,
                    handles = mapOf(byHandle.inboxId() to handle),
                )
            val group = (conversation as Conversation.Group).group
            assertTrue(group.members().map { it.inboxId }.containsAll(expected))
            val added = peer()
            val addedHandle = claim(added, coded = true)
            compose.onNodeWithContentDescription("Conversation details").performClick()
            until("Group settings did not open") {
                model.state.value.screen == Screen.CONVERSATION_SETTINGS && model.state.value.settings.group
            }
            assertEquals(
                handle,
                model.group.state.value.settings.members
                    .single { it.inboxId == byHandle.inboxId() }
                    .displayName,
            )
            button("Add member").performClick()
            compose.onNodeWithText(recipientLabel).performTextReplacement("@" + addedHandle)
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Add member").performClick()
            until("Handle did not add the claimed inbox or stayed pending") {
                !model.group.state.value.busy &&
                    model.group.state.value.pendingMemberIds
                        .isEmpty() &&
                    model.group.state.value.settings.members
                        .any { it.inboxId == added.inboxId() }
            }
            assertTrue(group.members().any { it.inboxId == added.inboxId() })
            assertFalse(group.members().any { it.inboxId == addedHandle })
        }
}
