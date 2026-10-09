package org.xmtp.android.example.messenger

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.text.TextLayoutResult
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runners.model.Statement
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.attachments.AttachmentDescriptor
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.metadata.FieldShape
import uniffi.xmtp_sdk.*
import java.util.UUID

/** Actual 320 dp device layout, touch scrolling and 200% text on all nine screens. */
class ScreenScaleInstrumentedTest {
    val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val captured = mutableSetOf<Screen>()

    private fun shell(command: String): String =
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand(command)
            .use { descriptor ->
                android.os.ParcelFileDescriptor
                    .AutoCloseInputStream(
                        descriptor,
                    ).bufferedReader()
                    .use { it.readText().trim() }
            }

    @get:Rule val rules: TestRule =
        RuleChain
            .outerRule(
                TestRule { base, _ ->
                    object : Statement() {
                        override fun evaluate() {
                            val scale = shell("settings get system font_scale")
                            val size =
                                shell("wm size")
                                    .lineSequence()
                                    .firstOrNull { it.startsWith("Override size:") }
                                    ?.substringAfter(':')
                                    ?.trim()
                            val density =
                                shell("wm density")
                                    .lineSequence()
                                    .firstOrNull { it.startsWith("Override density:") }
                                    ?.substringAfter(':')
                                    ?.trim()
                            shell("wm size 320x640")
                            shell("wm density 160")
                            shell("settings put system font_scale 2.0")
                            try {
                                base.evaluate()
                            } finally {
                                if (scale == "null") {
                                    shell("settings delete system font_scale")
                                } else {
                                    shell("settings put system font_scale $scale")
                                }
                                shell(if (density == null) "wm density reset" else "wm density $density")
                                shell(if (size == null) "wm size reset" else "wm size $size")
                            }
                        }
                    }
                },
            ).around(compose)

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

    private fun closeKeyboard() {
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        compose.waitForIdle()
    }

    private fun click(label: String) = hasText(label) and hasClickAction()

    /** Use physical swipes. Programmatic scroll-to cannot prove user scrolling. */
    private fun reveal(
        matcher: SemanticsMatcher,
        siblingLabel: String? = null,
    ): SemanticsNodeInteraction {
        closeKeyboard()
        val minimum = 48f * compose.activity.resources.displayMetrics.density
        repeat(24) {
            val node = compose.onAllNodes(matcher).fetchSemanticsNodes().singleOrNull()
            if (node != null) {
                val bounds = node.boundsInRoot
                if (bounds.width >= minimum - 1 && bounds.height >= minimum - 1) {
                    val control = compose.onNode(matcher)
                    if (labelsFit(control, siblingLabel)) return control
                }
            }
            val scrolling =
                compose
                    .onAllNodes(
                        SemanticsMatcher.keyIsDefined(SemanticsActions.ScrollBy),
                        useUnmergedTree = true,
                    ).fetchSemanticsNodes()
                    .maxByOrNull { it.boundsInRoot.width * it.boundsInRoot.height }
            assertNotNull("No user scroll path can reveal $matcher on ${model.state.value.screen}", scrolling)
            val scroll = checkNotNull(scrolling)
            val interaction =
                compose.onNode(
                    SemanticsMatcher("scroll frame ${scroll.id}") { it.id == scroll.id },
                    useUnmergedTree = true,
                )
            val before = scroll.config[SemanticsProperties.VerticalScrollAxisRange].value()
            val backwards = node != null && node.positionInRoot.y < scroll.boundsInRoot.top
            interaction.performTouchInput {
                val x = width - 4f
                val upper = height * 0.2f
                val lower = height * 0.8f
                swipe(Offset(x, if (backwards) upper else lower), Offset(x, if (backwards) lower else upper), 250)
            }
            compose.waitForIdle()
            val after = interaction.fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange].value()
            assertNotEquals("Touch scrolling did not move ${model.state.value.screen}", before, after)
        }
        fail("The action cannot be reached at 320 dp and 200% text: $matcher")
        error("Unreachable")
    }

    private fun labelsFit(
        control: SemanticsNodeInteraction,
        siblingLabel: String? = null,
    ): Boolean {
        val selectedNode = control.fetchSemanticsNode()
        val selected = SemanticsMatcher("selected control ${selectedNode.id}") { it.id == selectedNode.id }
        val scope = if (siblingLabel == null) selected or hasAnyAncestor(selected) else hasText(siblingLabel)
        val captions =
            compose
                .onAllNodes(
                    SemanticsMatcher.keyIsDefined(SemanticsProperties.Text) and scope,
                    useUnmergedTree = true,
                ).fetchSemanticsNodes()
                .filter { node -> node.config[SemanticsProperties.Text].any { it.text.isNotBlank() } }
        val unmergedLabels =
            captions
                .flatMap { node -> node.config[SemanticsProperties.Text].map { it.text } }
                .filter { it.isNotBlank() }
        val mergedLabels =
            selectedNode.config
                .getOrNull(SemanticsProperties.Text)
                .orEmpty()
                .map { it.text }
                .filter { it.isNotBlank() }
        val expected = if (siblingLabel != null) listOf(siblingLabel) else (mergedLabels + unmergedLabels).distinct()
        if (expected.isEmpty()) {
            val descriptions = selectedNode.config.getOrNull(SemanticsProperties.ContentDescription).orEmpty()
            assertTrue("The control has no caption or accessible icon label", descriptions.any { it.isNotBlank() })
            assertNull(
                "A toggle needs its visible sibling caption",
                selectedNode.config.getOrNull(SemanticsProperties.ToggleableState),
            )
            assertNull(
                "A text input needs its visible caption",
                selectedNode.config.getOrNull(SemanticsActions.SetText),
            )
            return true
        }
        assertTrue("Unmerged caption evidence is missing for $expected", captions.isNotEmpty())
        for (label in expected) {
            assertTrue(
                "Unmerged caption evidence is missing for '$label'",
                captions.any { node -> node.config[SemanticsProperties.Text].any { it.text == label } },
            )
        }
        var visible = true
        for (caption in captions) {
            val labels = caption.config[SemanticsProperties.Text].map { it.text }.filter { it in expected }
            assertTrue("The expected caption is empty", labels.isNotEmpty())
            if (siblingLabel != null) {
                assertEquals(
                    "The toggle label is not in the same semantic row",
                    selectedNode.parent?.id,
                    caption.parent?.id,
                )
                assertTrue(
                    "The toggle label is not beside the checkbox",
                    caption.positionInRoot.x >= selectedNode.positionInRoot.x,
                )
            }
            val layout = caption.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action
            assertNotNull("Text layout action is missing for $labels", layout)
            val results = mutableListOf<TextLayoutResult>()
            var succeeded = false
            compose.runOnIdle { succeeded = checkNotNull(layout).invoke(results) }
            assertTrue("Text layout action failed for $labels", succeeded)
            assertTrue("Text layout evidence is empty for $labels", results.isNotEmpty())
            for (label in labels) {
                assertTrue(
                    "Text layout evidence does not contain '$label'",
                    results.any { it.layoutInput.text.text == label },
                )
            }
            for (result in results) {
                assertFalse("The text label is clipped: ${result.layoutInput.text.text}", result.didOverflowWidth)
                assertFalse("The text label is clipped: ${result.layoutInput.text.text}", result.didOverflowHeight)
                if (result.size.width > caption.boundsInRoot.width + 1 ||
                    result.size.height > caption.boundsInRoot.height + 1
                ) {
                    visible = false
                }
            }
        }
        return visible
    }

    private fun control(
        matcher: SemanticsMatcher,
        siblingLabel: String? = null,
    ): SemanticsNodeInteraction {
        val result = reveal(matcher, siblingLabel)
        result.assertIsDisplayed()
        val bounds = result.fetchSemanticsNode().boundsInRoot
        val minimum = 48f * compose.activity.resources.displayMetrics.density
        assertTrue("The action is narrower than 48 dp", bounds.width >= minimum - 1)
        assertTrue("The action is shorter than 48 dp", bounds.height >= minimum - 1)
        assertTrue("The action crosses the display edge", bounds.left >= -1 && bounds.right <= 321)
        assertTrue("The action text is clipped", labelsFit(result, siblingLabel))
        return result
    }

    private fun input(
        label: String,
        value: String,
    ) {
        control(hasText(label) and hasSetTextAction()).performTextReplacement(value)
        closeKeyboard()
    }

    private fun capture(screen: Screen) {
        assertEquals(screen, model.state.value.screen)
        saveMessengerScreenshot(compose.activity, "scale-${screen.name.lowercase()}")
        captured += screen
    }

    private suspend fun groupAction(
        expected: MessengerAction,
        label: String,
    ) {
        val finished = CompletableDeferred<Unit>()
        model.onGroupActionFinished = { if (it == expected) finished.complete(Unit) }
        try {
            control(click(label)).performClick()
            withTimeout(30_000) { finished.await() }
        } finally {
            model.onGroupActionFinished = {}
        }
    }

    @Test fun allScreensRemainReachableAtDoubleTextScale() =
        runBlocking<Unit> {
            assertEquals(320, compose.activity.resources.configuration.screenWidthDp)
            assertEquals(2f, compose.activity.resources.configuration.fontScale, 0.01f)
            val backend = checkNotNull(InstrumentationRegistry.getArguments().getString("metadataBackendUrl"))
            val lifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var peer: SDKClient? = null
            var reader: Job? = null
            try {
                model.session.signOut()
                until("Start did not appear after sign out") { model.state.value.screen == Screen.START }
                input("Backend URL", backend)
                control(click("Connect"))
                capture(Screen.START)
                control(click("Connect")).performClick()
                until("The actual profile did not connect") {
                    model.session.active.value != null &&
                        model.state.value.screen == Screen.CONVERSATIONS
                }
                val owner = checkNotNull(model.session.active.value)
                peer =
                    SDKClient.create(
                        compose.activity,
                        generateLocalSigner(),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = backend)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val other = checkNotNull(peer)
                reader = launch(Dispatchers.IO) { other.conversations.streamAllMessages().collect {} }
                control(click("Allowed"))
                control(click("Unknown"))
                control(hasContentDescription("Settings"))
                capture(Screen.CONVERSATIONS)
                control(hasContentDescription("New conversation")).performClick()
                until("Create did not open") { model.state.value.screen == Screen.CREATE }
                control(click("Direct message"))
                control(click("Group")).performClick()
                input("Inbox IDs or Ethereum addresses, separated by commas", other.inboxId())
                input("Name", "Scale proof group")
                input("Description", "A real group for all screen bounds")
                control(isToggleable(), siblingLabel = "Admins only")
                capture(Screen.CREATE)
                control(click("Create")).performClick()
                until("Create did not open the real timeline") {
                    model.state.value.screen == Screen.TIMELINE &&
                        model.state.value.conversationId != null
                }
                val id = checkNotNull(model.state.value.conversationId)
                val chat = checkNotNull(owner.client.conversations.getById(id))
                val group = (chat as Conversation.Group).group
                assertEquals("Scale proof group", group.state().name)
                input("Message", "Real scaled composer text")
                control(click("Send")).performClick()
                until("The scaled composer did not publish its real text") {
                    chat.messages(publishedSelection()).any {
                        (it.standardContent() as? MessageContent.Text)?.v1 == "Real scaled composer text"
                    }
                }
                control(hasContentDescription("Conversation settings"))
                capture(Screen.TIMELINE)
                control(hasContentDescription("Conversation settings")).performClick()
                until("Conversation settings did not open") { model.state.value.screen == Screen.CONVERSATION_SETTINGS }
                input("Name", "Scaled settings name")
                input("Description", "The settings controls remain editable")
                groupAction(
                    MessengerAction.UpdateGroup("Scaled settings name", "The settings controls remain editable"),
                    "Save",
                )
                assertEquals("Scaled settings name", group.state().name)
                control(click("All members"))
                control(click("Admins only"))
                input("Disappearing messages: seconds (0 is Off)", "0")
                control(click("Save duration"))
                val memberTag = "settings-member-${other.inboxId()}"
                control(click("Make admin") and hasAnyAncestor(hasTestTag(memberTag)))
                control(click("Remove") and hasAnyAncestor(hasTestTag(memberTag)))
                input("Inbox ID", other.inboxId())
                control(click("Add member"))
                control(click("Block"))
                capture(Screen.CONVERSATION_SETTINGS)
                control(click("Group fields")).performClick()
                until("The real catalogue fields did not load") {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.id.componentId == 49160.toUShort() }
                }
                control(click("Reload fields")).performClick()
                until("The real field refresh did not finish") { !model.metadataState.value.busy }
                for (idValue in listOf(49153, 49154, 49155, 49156, 49157, 49160, 64768, 64769, 64770)) {
                    val prefix =
                        if (idValue in 49155..49157 || idValue in 64769..64770) "metadata-key-" else "metadata-value-"
                    control(hasTestTag("$prefix$idValue"))
                }
                control(hasTestTag("metadata-set-49160"))
                control(hasTestTag("metadata-set-64768"))
                control(hasTestTag("metadata-entry-value-64769"))
                control(hasTestTag("metadata-add-64769"))
                control(hasTestTag("metadata-update-64769"))
                control(hasTestTag("metadata-add-64770"))
                capture(Screen.GROUP_FIELDS)
                control(hasContentDescription("Back")).performClick()
                until("Settings did not return from group fields") {
                    model.state.value.screen ==
                        Screen.CONVERSATION_SETTINGS
                }
                control(click("My fields")).performClick()
                until("The actual user fields did not load") {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.id.componentId == 49159.toUShort() }
                }
                control(hasTestTag("metadata-value-49158"))
                control(hasTestTag("metadata-value-49159"))
                control(hasTestTag("metadata-value-64771"))
                val immutableOwn = hasAnyAncestor(hasTestTag("metadata-field-64771"))
                control(click("Set empty") and immutableOwn)
                control(click("Clear") and immutableOwn)
                control(click("Save changed fields"))
                capture(Screen.MY_FIELDS)
                control(hasContentDescription("Back")).performClick()
                until(
                    "Settings did not return from own fields",
                ) { model.state.value.screen == Screen.CONVERSATION_SETTINGS }
                control(hasContentDescription("Back")).performClick()
                until("Timeline did not return") { model.state.value.screen == Screen.TIMELINE }
                control(hasContentDescription("Back")).performClick()
                until("Conversations did not return") { model.state.value.screen == Screen.CONVERSATIONS }
                // The layout fixture creates real SDK staged files, not invented card states.
                val draftIds = mutableListOf<String>()
                var acceptedDraft: String? = null
                repeat(3) { index ->
                    val pending =
                        owner.client.attachments().create(
                            AttachmentSource.Bytes(
                                ByteArray(64) { 7 },
                                "scale-layout-draft-$index.bin",
                                "application/octet-stream",
                            ),
                        )
                    val draft = UUID.randomUUID().toString()
                    val secret = "scale-$draft"
                    val reference =
                        if (index == 2) {
                            pending.upload()
                            val accepted =
                                chat.sendRemoteAttachment(
                                    pending.remoteAttachment(),
                                    SendOptions(optimistic = true),
                                )
                            assertEquals(
                                DeliveryStatus.UNPUBLISHED,
                                owner.client.conversations
                                    .getMessageById(accepted)!!
                                    .deliveryStatus,
                            )
                            acceptedDraft = draft
                            SendDraftRef(draft, id, secret, accepted, SendPhase.ACCEPTED)
                        } else {
                            SendDraftRef(draft, id, secret)
                        }
                    model.session.secrets.write(
                        owner.key.profileId,
                        secret,
                        AttachmentDescriptor.encode(pending.remoteAttachment()),
                    )
                    model.session.preferences.saveDraft(owner.key.profileId, reference)
                    draftIds += draft
                }
                model.dispatch(MessengerAction.Refresh)
                control(hasContentDescription("Settings")).performClick()
                until("App settings did not open") { model.state.value.screen == Screen.APP_SETTINGS }
                control(click("Draft recovery"))
                control(click("Sign out"))
                control(click("Delete my account")).performClick()
                control(click("Cancel")).performClick()
                capture(Screen.APP_SETTINGS)
                control(click("Draft recovery")).performClick()
                until("Draft recovery did not open") { model.state.value.screen == Screen.DRAFTS }
                for (draft in draftIds) {
                    val card = "attachment-card-$draft"
                    control(click("Discard") and hasAnyAncestor(hasTestTag(card)))
                    if (draft == acceptedDraft) {
                        control(click("Retry publication") and hasAnyAncestor(hasTestTag(card)))
                        control(click("View chat") and hasAnyAncestor(hasTestTag(card)))
                    } else {
                        control(click("Send file") and hasAnyAncestor(hasTestTag(card)))
                    }
                }
                capture(Screen.DRAFTS)
                assertEquals("Every screen needs an observed bounds/scroll pass", Screen.entries.toSet(), captured)
            } finally {
                model.onGroupActionFinished = {}
                reader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    if (model.session.active.value != null) model.session.deleteAccount()
                }
                AndroidStreamLifecycle.enabled = lifecycle
            }
        }
}
