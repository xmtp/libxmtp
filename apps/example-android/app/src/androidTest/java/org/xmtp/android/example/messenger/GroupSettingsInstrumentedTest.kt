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
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.data.xmtp.XmtpQueries
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.conversations.CreateStep
import org.xmtp.android.example.shared.feature.group.GroupAction
import uniffi.xmtp_sdk.*

/** Mutations enter through the real shared settings and ViewModel. */
class GroupSettingsInstrumentedTest {
    private val rejectCreate =
        java.util.concurrent.atomic
            .AtomicBoolean()
    private val nativeQueries = XmtpQueries()
    private val createQueries =
        object : ConversationQueries by nativeQueries {
            override suspend fun create(
                owner: ActiveSession,
                group: Boolean,
                recipients: List<String>,
                name: String,
                description: String,
                adminOnly: Boolean,
            ): Conversation {
                if (rejectCreate.compareAndSet(true, false)) {
                    throw java.io.IOException("Recoverable creation fixture failure")
                }
                return nativeQueries.create(owner, group, recipients, name, description, adminOnly)
            }
        }
    private val graphRule = TestAppGraphRule(conversationQueries = createQueries)
    val compose = createAndroidComposeRule<MainActivity>()
    private lateinit var backend: String

    @get:Rule val admitted =
        org.junit.rules.RuleChain
            .outerRule(
                org.junit.rules.TestRule { base, _ ->
                    object : org.junit.runners.model.Statement() {
                        override fun evaluate() {
                            val arguments =
                                androidx.test.platform.app.InstrumentationRegistry
                                    .getArguments()
                            backend =
                                ownedMetadataBackend(
                                    arguments.getString("metadataBackendUrl"),
                                    arguments.getString("metadataBackendOwned"),
                                )
                            base.evaluate()
                        }
                    }
                },
            ).around(graphRule)
            .around(compose)
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
            model.session.connect(backend, "", true)
            val owner = checkNotNull(model.session.active.value)
            until("The app did not publish its real session") { model.state.value.inbox == owner.client.inboxId() }
        }

    @After fun closeSession() =
        runBlocking<Unit> {
            var failure: Throwable? = null

            suspend fun attempt(step: suspend () -> Unit) {
                try {
                    step()
                } catch (error: Throwable) {
                    if (failure == null) {
                        failure = error
                    } else if (failure !== error) {
                        failure?.addSuppressed(error)
                    }
                }
            }
            try {
                withContext(NonCancellable) {
                    attempt { model.group.beforeGroupWrite = { _, _ -> } }
                    attempt { model.group.onGroupActionFinished = {} }
                    readers.forEach { reader -> attempt { reader.cancelAndJoin() } }
                    peers.forEach { peer -> attempt { peer.end() } }
                    attempt { work.cancel() }
                    attempt { if (model.session.active.value != null) model.session.deleteAccount() }
                }
                failure?.let { throw it }
            } finally {
                AndroidStreamLifecycle.enabled = lifecycle
            }
        }

    private suspend fun peer(): SDKClient {
        val client =
            SDKClient.create(
                compose.activity,
                generateLocalSigner(),
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = backend)),
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
            compose.onNodeWithTag("conversation-settings").performScrollToNode(matcher)
        }
        return compose.onNode(matcher)
    }

    private suspend fun appGroup(
        member: SDKClient,
        failCreation: Boolean = false,
    ): Group {
        compose.onNodeWithContentDescription("New message").performClick()
        button("Group").performClick()
        compose
            .onNodeWithText("Handle, Ethereum address, or inbox ID")
            .performTextReplacement(member.inboxId())
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        button("Check and add").performScrollTo().performClick()
        until("The group form did not resolve its native member") {
            model.createConversation.state.value.selectedRecipients
                .any { it.inboxId == member.inboxId() } &&
                !model.createConversation.state.value.busy
        }
        button("Continue").performScrollTo().performClick()
        until("The group form did not open its review step") {
            model.createConversation.state.value.step == CreateStep.REVIEW
        }
        compose.onNodeWithText("Group name").performTextReplacement("Settings proof")
        compose.onNodeWithText("Description (optional)").performTextReplacement("Created through the shared form")
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        if (failCreation) {
            val owner = checkNotNull(model.session.active.value)
            val before =
                owner.client.conversations
                    .list(ListConversationsOptions())
                    .map { it.id() }
                    .toSet()
            rejectCreate.set(true)
            button("Create group").performScrollTo().performClick()
            until("Recoverable group creation failed") {
                !model.createConversation.state.value.busy && model.createConversation.state.value.failure != null
            }
            assertEquals(
                "conversation-create",
                model.createConversation.state.value.failure
                    ?.code,
            )
            assertEquals(CreateStep.REVIEW, model.createConversation.state.value.step)
            assertEquals("Settings proof", model.createConversation.state.value.name)
            assertEquals("Created through the shared form", model.createConversation.state.value.description)
            assertEquals(
                listOf(member.inboxId()),
                model.createConversation.state.value.selectedRecipients
                    .map { it.inboxId },
            )
            compose.onNodeWithText("Group name").assertTextContains("Settings proof")
            compose.onNodeWithText("Description (optional)").assertTextContains("Created through the shared form")
            compose.onNodeWithText(member.inboxId().take(12), substring = true).assertExists()
            assertEquals(
                before,
                owner.client.conversations
                    .list(ListConversationsOptions())
                    .map { it.id() }
                    .toSet(),
            )
            button("Create group").performScrollTo().assertIsEnabled()
        }
        button("Create group").performScrollTo().performClick()
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
        compose.onNodeWithContentDescription("Conversation details").performClick()
        until("The real settings projection did not open") {
            model.state.value.screen == Screen.CONVERSATION_SETTINGS && model.state.value.settings.group
        }
    }

    private suspend fun action(
        expected: GroupAction,
        click: () -> Unit,
    ) {
        val finished = CompletableDeferred<Unit>()
        model.group.onGroupActionFinished = { if (it == expected) finished.complete(Unit) }
        try {
            click()
            withTimeout(30_000) { finished.await() }
            compose.waitForIdle()
        } finally {
            model.group.onGroupActionFinished = {}
        }
    }

    private fun memberButton(
        inbox: String,
        label: String,
    ): SemanticsNodeInteraction {
        compose.onNodeWithTag("conversation-settings").performScrollToNode(hasTestTag("settings-member-$inbox"))
        return compose.onNode(
            hasText(label) and hasClickAction() and
                hasAnyAncestor(hasTestTag("settings-member-$inbox")),
        )
    }

    @Test fun editsPresetsAndDurationCommitThroughSharedSettings() =
        runBlocking<Unit> {
            val group = appGroup(peer(), failCreation = true)
            button("Edit name and description").performClick()
            settingsNode(hasText("Name")).performTextReplacement("Edited through settings")
            settingsNode(hasText("Description")).performTextReplacement("Persisted description")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(GroupAction.UpdateGroup("Edited through settings", "Persisted description")) {
                button("Save").performScrollTo().performClick()
            }
            assertEquals("Settings did not commit the group name", "Edited through settings", group.state().name)
            assertEquals("Settings did not commit the description", "Persisted description", group.state().description)
            assertEquals("Edited through settings", model.state.value.settings.title)
            button("Change permissions").performClick()
            val beforePreset = group.state().permissions.policySet
            assertEquals(PermissionPolicy.ALLOW, beforePreset.addMember)
            var permissionWrites = 0
            model.group.beforeGroupWrite = { actual, index ->
                if (actual == GroupAction.SetPreset(true)) {
                    permissionWrites += 1
                    if (index == 1) error("Private permission failure must not appear on screen")
                }
            }
            try {
                action(GroupAction.SetPreset(true)) { button("Admins only").performClick() }
            } finally {
                model.group.beforeGroupWrite = { _, _ -> }
            }
            val partialPreset = group.state().permissions
            assertEquals("The preset did not reach its second write", 2, permissionWrites)
            assertEquals(
                "The first native permission write did not commit",
                PermissionPolicy.ADMIN,
                partialPreset.policySet.addMember,
            )
            assertEquals(
                "The later native permission changed after failure",
                beforePreset.updateName,
                partialPreset.policySet.updateName,
            )
            assertEquals("The partial native preset must be Custom", GroupPolicyType.CUSTOM, partialPreset.policyType)
            assertEquals("The partial preset was not projected", "Custom", model.state.value.settings.preset)
            settingsNode(hasText("Cannot update this conversation. Try again.")).assertIsDisplayed()
            compose.onNodeWithText("Private permission failure must not appear on screen").assertDoesNotExist()
            action(GroupAction.SetPreset(false)) { button("All members").performClick() }
            assertEquals(
                "The UI did not recover the partial preset",
                GroupPolicyType.ALL_MEMBERS,
                group.state().permissions.policyType,
            )
            assertNull("The recovered preset retained its failure", model.group.state.value.failure)
            action(GroupAction.SetPreset(true)) { button("Admins only").performClick() }
            assertEquals(
                "Settings did not commit the admin preset",
                GroupPolicyType.ADMIN_ONLY,
                group.state().permissions.policyType,
            )
            action(GroupAction.SetPreset(false)) { button("All members").performScrollTo().performClick() }
            assertEquals(
                "Settings did not restore the member preset",
                GroupPolicyType.ALL_MEMBERS,
                group.state().permissions.policyType,
            )
            button("Change duration").performClick()
            settingsNode(hasText("Duration in seconds")).performTextReplacement("17")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(GroupAction.SetDisappearing(17)) { button("Save duration").performScrollTo().performClick() }
            assertEquals(
                "Settings did not commit the duration",
                17_000_000_000L,
                group
                    .state()
                    .common.disappearingSettings
                    ?.retentionNs,
            )
            settingsNode(hasText("Duration in seconds")).performTextReplacement("0")
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(GroupAction.SetDisappearing(0)) { button("Save duration").performScrollTo().performClick() }
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
            button("Add member").performClick()
            settingsNode(hasText("Inbox ID")).performTextReplacement(added)
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            action(GroupAction.AddMember(added)) { button("Add member").performScrollTo().performClick() }
            assertTrue("Settings did not add the selected member", group.members().any { it.inboxId == added })
            assertTrue(
                model.state.value.settings.members
                    .any { it.inboxId == added && it.role == "Member" },
            )
            val adminsBeforeRejectedPromotion = group.state().admins.toSet()
            model.group.beforeGroupWrite = { queued, index ->
                if (queued == GroupAction.SetAdmin(added, true) && index == 0) {
                    throw java.io.IOException("Definite promotion rejection before SDK write")
                }
            }
            try {
                action(GroupAction.SetAdmin(added, true)) { memberButton(added, "Make admin").performClick() }
            } finally {
                model.group.beforeGroupWrite = { _, _ -> }
            }
            until("Rejected promotion did not finish") { !model.group.state.value.busy }
            assertEquals(
                "group-action",
                model.group.state.value.failure
                    ?.code,
            )
            assertEquals(
                "Rejected promotion changed native admin set",
                adminsBeforeRejectedPromotion,
                group.state().admins.toSet(),
            )
            assertFalse("Rejected promotion changed native admins", group.state().admins.contains(added))
            assertFalse("Rejected promotion retained its marker", added in model.group.state.value.pendingMemberIds)
            memberButton(added, "Make admin").assertIsEnabled()
            action(GroupAction.SetAdmin(added, true)) { memberButton(added, "Make admin").performClick() }
            assertTrue("Settings did not promote the selected member", group.state().admins.contains(added))
            memberButton(added, "Remove admin").assertIsDisplayed()
            action(GroupAction.SetAdmin(added, false)) { memberButton(added, "Remove admin").performClick() }
            assertFalse("Settings did not demote the selected admin", group.state().admins.contains(added))
            memberButton(added, "Make admin").assertIsDisplayed()
            val membersBeforeRejectedRemoval = group.members().map { it.inboxId }.toSet()
            model.group.beforeGroupWrite = { queued, index ->
                if (queued == GroupAction.RemoveMember(added) && index == 0) {
                    throw java.io.IOException("Definite member rejection before SDK write")
                }
            }
            try {
                action(GroupAction.RemoveMember(added)) { memberButton(added, "Remove").performClick() }
            } finally {
                model.group.beforeGroupWrite = { _, _ -> }
            }
            until("Rejected member removal did not finish") { !model.group.state.value.busy }
            assertEquals(
                "group-action",
                model.group.state.value.failure
                    ?.code,
            )
            assertEquals(
                "Rejected removal changed native member set",
                membersBeforeRejectedRemoval,
                group.members().map { it.inboxId }.toSet(),
            )
            assertTrue("Rejected removal changed native members", group.members().any { it.inboxId == added })
            assertFalse("Rejected removal retained its marker", added in model.group.state.value.pendingMemberIds)
            memberButton(added, "Remove").assertIsEnabled()
            action(GroupAction.RemoveMember(added)) { memberButton(added, "Remove").performClick() }
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
            val membershipBeforeRejectedLeave = group.state().membershipState
            val membersBeforeRejectedLeave = group.members().map { it.inboxId }.toSet()
            model.group.beforeGroupWrite = { queued, index ->
                if (queued == GroupAction.RequestRemoval && index == 0) {
                    throw java.io.IOException("Definite leave rejection before SDK write")
                }
            }
            try {
                button("Leave group").performClick()
                action(GroupAction.RequestRemoval) {
                    compose
                        .onNode(hasText("Leave group") and hasClickAction() and hasAnyAncestor(isDialog()))
                        .performClick()
                }
            } finally {
                model.group.beforeGroupWrite = { _, _ -> }
            }
            until("Rejected leave did not finish") { !model.group.state.value.busy }
            assertEquals(
                "group-action",
                model.group.state.value.failure
                    ?.code,
            )
            assertEquals(
                "Rejected leave changed native membership",
                membershipBeforeRejectedLeave,
                group.state().membershipState,
            )
            assertEquals(
                "Rejected leave changed native member set",
                membersBeforeRejectedLeave,
                group.members().map { it.inboxId }.toSet(),
            )
            assertTrue(group.members().any { it.inboxId == owner.client.inboxId() })
            assertFalse("Rejected leave retained its marker", model.group.state.value.pendingLeave)
            assertTrue(model.state.value.settings.canRequestRemoval)
            compose
                .onNode(hasText("Leave group") and hasClickAction() and hasAnyAncestor(isDialog()))
                .assertIsEnabled()
            compose
                .onNode(hasText("Cancel") and hasClickAction() and hasAnyAncestor(isDialog()))
                .performClick()
            compose.onNodeWithText("Leave group?").assertDoesNotExist()
            button("Leave group").assertIsEnabled()
            button("Leave group").performClick()
            compose.onNodeWithText("Leave group?").assertIsDisplayed()
            assertEquals(
                "Opening confirmation must not request removal",
                membershipBeforeRejectedLeave,
                group.state().membershipState,
            )
            action(GroupAction.RequestRemoval) {
                compose
                    .onNode(hasText("Leave group") and hasClickAction() and hasAnyAncestor(isDialog()))
                    .performClick()
            }
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
            settingsNode(hasText("Leaving group. Removal is pending until membership confirms the change."))
                .assertIsDisplayed()
            button("Leaving group…").assertIsNotEnabled()
            compose.onNodeWithText("Leave group?").assertDoesNotExist()
            saveMessengerScreenshot(compose.activity, "group-settings-pending-remove")
        }
}
