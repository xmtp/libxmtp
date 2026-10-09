package org.xmtp.android.example.messenger

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

/** Mutations enter through the real shared settings and ViewModel. */
class GroupSettingsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val peers = mutableListOf<SDKClient>()
    private val readers = mutableListOf<Job>()
    private val work = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var lifecycle = true

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        assertTrue(
            stage,
            withTimeoutOrNull(30_000) {
                while (!check()) delay(20)
                true
            } == true,
        )
    }

    @Before fun openSession() =
        runBlocking<Unit> {
            lifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            model.session.signOut()
            model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            val owner = checkNotNull(model.session.active.value)
            until("The app did not publish its real session") { model.state.value.inbox == owner.client.inboxId() }
        }

    @After fun closeSession() =
        runBlocking<Unit> {
            model.onGroupActionFinished = {}
            readers.forEach { it.cancelAndJoin() }
            withContext(NonCancellable) {
                peers.forEach { it.end() }
                work.cancel()
                if (model.session.active.value != null) model.session.deleteAccount()
            }
            AndroidStreamLifecycle.enabled = lifecycle
        }

    private suspend fun peer(): SDKClient {
        val client =
            SDKClient.create(
                compose.activity,
                generateLocalSigner(),
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    storage = StorageOptions(location = StorageLocation.InMemory),
                    deviceSync = false,
                ),
            )
        peers += client
        readers += work.launch { client.conversations.streamAllMessages().collect {} }
        return client
    }

    private fun settingsNode(matcher: SemanticsMatcher): SemanticsNodeInteraction {
        compose.onNodeWithTag("conversation-settings").performScrollToNode(matcher)
        return compose.onNode(matcher)
    }

    private fun button(label: String): SemanticsNodeInteraction {
        val matcher = hasText(label) and hasClickAction()
        if (model.state.value.screen == Screen.CONVERSATION_SETTINGS) {
            compose.onNodeWithTag("conversation-settings").performScrollToNode(hasText(label))
        }
        return compose.onNode(matcher)
    }

    private suspend fun appGroup(member: SDKClient): Group {
        compose.onNodeWithContentDescription("New conversation").performClick()
        button("Group").performClick()
        compose
            .onNodeWithText("Inbox IDs or Ethereum addresses, separated by commas")
            .performTextReplacement(member.inboxId())
        compose.onNodeWithText("Name").performTextReplacement("Settings proof")
        compose.onNodeWithText("Description").performTextReplacement("Created through the shared form")
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        button("Create").performScrollTo().performClick()
        until("Create did not open its real group") {
            model.state.value.screen == Screen.TIMELINE && model.state.value.conversationId != null
        }
        val owner = checkNotNull(model.session.active.value)
        val chat = checkNotNull(owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)))
        val group = (chat as Conversation.Group).group
        assertEquals("Settings proof", group.state().name)
        settings()
        return group
    }

    private suspend fun settings() {
        compose.onNodeWithContentDescription("Conversation settings").performClick()
        until("The real settings projection did not open") {
            model.state.value.screen == Screen.CONVERSATION_SETTINGS && model.state.value.settings.group
        }
    }

    private suspend fun action(
        expected: MessengerAction,
        click: () -> Unit,
    ) {
        val finished = CompletableDeferred<Unit>()
        model.onGroupActionFinished = { if (it == expected) finished.complete(Unit) }
        try {
            click()
            withTimeout(30_000) { finished.await() }
            compose.waitForIdle()
        } finally {
            model.onGroupActionFinished = {}
        }
    }

    private fun memberButton(
        inbox: String,
        label: String,
    ): SemanticsNodeInteraction {
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasTestTag("settings-member-$inbox"))
        return compose.onNode(
            hasText(label) and hasClickAction() and
                hasAnyAncestor(hasTestTag("settings-member-$inbox")),
        )
    }

    @Test fun editsPresetsAndDurationCommitThroughSharedSettings() =
        runBlocking<Unit> {
            val group = appGroup(peer())
            compose.onNodeWithText("Name").performTextReplacement("Edited through settings")
            compose.onNodeWithText("Description").performTextReplacement("Persisted description")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(MessengerAction.UpdateGroup("Edited through settings", "Persisted description")) {
                button("Save").performScrollTo().performClick()
            }
            assertEquals("Settings did not commit the group name", "Edited through settings", group.state().name)
            assertEquals("Settings did not commit the description", "Persisted description", group.state().description)
            assertEquals("Edited through settings", model.state.value.settings.title)
            action(MessengerAction.SetPreset(true)) { button("Admins only").performScrollTo().performClick() }
            assertEquals(
                "Settings did not commit the admin preset",
                GroupPolicyType.ADMIN_ONLY,
                group.state().permissions.policyType,
            )
            action(MessengerAction.SetPreset(false)) { button("All members").performScrollTo().performClick() }
            assertEquals(
                "Settings did not restore the member preset",
                GroupPolicyType.ALL_MEMBERS,
                group.state().permissions.policyType,
            )
            compose
                .onNodeWithText("Disappearing messages: seconds (0 is Off)")
                .performScrollTo()
                .performTextReplacement("17")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(MessengerAction.SetDisappearing(17)) { button("Save duration").performScrollTo().performClick() }
            assertEquals(
                "Settings did not commit the duration",
                17_000_000_000L,
                group
                    .state()
                    .common.disappearingSettings
                    ?.retentionNs,
            )
            compose
                .onNodeWithText("Disappearing messages: seconds (0 is Off)")
                .performScrollTo()
                .performTextReplacement("0")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(MessengerAction.SetDisappearing(0)) { button("Save duration").performScrollTo().performClick() }
            val cleared = group.state().common
            assertFalse("Settings did not disable disappearing messages", cleared.isDisappearingEnabled)
            assertEquals("Settings retained the old duration", 0L, cleared.disappearingSettings?.retentionNs)
        }

    @Test fun membersAndRolesCommitThroughSharedSettings() =
        runBlocking<Unit> {
            val first = peer()
            val second = peer()
            val group = appGroup(first)
            val added = second.inboxId()
            settingsNode(hasText("Inbox ID")).performTextReplacement(added)
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(MessengerAction.AddMember(added)) { button("Add member").performScrollTo().performClick() }
            assertTrue("Settings did not add the selected member", group.members().any { it.inboxId == added })
            assertTrue(
                model.state.value.settings.members
                    .any { it.inboxId == added && it.role == "Member" },
            )
            action(MessengerAction.SetAdmin(added, true)) { memberButton(added, "Make admin").performClick() }
            assertTrue("Settings did not promote the selected member", group.state().admins.contains(added))
            memberButton(added, "Remove admin").assertIsDisplayed()
            action(MessengerAction.SetAdmin(added, false)) { memberButton(added, "Remove admin").performClick() }
            assertFalse("Settings did not demote the selected admin", group.state().admins.contains(added))
            memberButton(added, "Make admin").assertIsDisplayed()
            action(MessengerAction.RemoveMember(added)) { memberButton(added, "Remove").performClick() }
            assertFalse("Settings did not remove the selected member", group.members().any { it.inboxId == added })
            assertTrue("Settings removed an unrelated member", group.members().any { it.inboxId == first.inboxId() })
            assertFalse(
                model.state.value.settings.members
                    .any { it.inboxId == added },
            )
        }

    @Test fun requestRemovalShowsPendingStateForAnActualMember() =
        runBlocking<Unit> {
            val creator = peer()
            val owner = checkNotNull(model.session.active.value)
            // Only fixture creation uses the other client. The app performs the removal request.
            val fixture =
                creator.conversations.createGroup(
                    listOf(owner.client.inboxId()),
                    CreateGroupOptions(name = "Member removal proof", permissions = GroupPermissionMode.AllMembers),
                )
            until(
                "The app did not receive the fixture Welcome",
            ) { owner.client.conversations.getById(fixture.id()) != null }
            model.dispatch(MessengerAction.OpenConversation(fixture.id()))
            until("The app did not open its member conversation") { model.state.value.conversationId == fixture.id() }
            settings()
            assertTrue(model.state.value.settings.canRequestRemoval)
            val group = (checkNotNull(owner.client.conversations.getById(fixture.id())) as Conversation.Group).group
            // Stop the administrator fixture so it cannot complete removal before the pending UI is checked.
            readers.forEach { it.cancelAndJoin() }
            creator.end()
            peers.remove(creator)
            action(MessengerAction.RequestRemoval) { button("Request removal").performScrollTo().performClick() }
            assertEquals(
                "The app did not request native removal",
                MembershipState.PENDING_REMOVE,
                group.state().membershipState,
            )
            assertTrue(
                "Pending removal incorrectly removed the member",
                group.members().any {
                    it.inboxId ==
                        owner.client.inboxId()
                },
            )
            assertFalse(model.state.value.settings.canRequestRemoval)
            settingsNode(hasText("Membership: PendingRemove")).assertIsDisplayed()
            compose.onNode(hasText("Request removal") and hasClickAction()).assertDoesNotExist()
            saveMessengerScreenshot(compose.activity, "group-settings-pending-remove")
        }
}
