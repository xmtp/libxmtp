package org.xmtp.android.example.messenger

import android.content.ContentValues
import android.graphics.Bitmap
import android.provider.MediaStore
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.emoji2.emojipicker.EmojiPickerView
import androidx.emoji2.text.EmojiCompat
import androidx.lifecycle.ViewModelProvider
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import androidx.test.espresso.Espresso.onView
import androidx.test.espresso.action.ViewActions.click
import androidx.test.espresso.matcher.ViewMatchers.*
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runners.model.Statement
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

private const val narrowCase = "narrowPickerKeepsTargetsAndComposerClearAtDoubleTextScale"

class MessengerUxInstrumentedTest {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule =
        RuleChain
            .outerRule(
                TestRule { base, description ->
                    object : Statement() {
                        override fun evaluate() {
                            val narrow = description.methodName == narrowCase
                            if (narrow) {
                                shell("wm size 320x640")
                                shell("wm density 160")
                                shell("settings put system font_scale 2.0")
                            }
                            try {
                                base.evaluate()
                            } finally {
                                if (narrow) {
                                    shell("settings put system font_scale 1.0")
                                    shell("wm density reset")
                                    shell("wm size reset")
                                }
                            }
                        }
                    }
                },
            ).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: () -> Boolean,
    ) {
        assertTrue(
            "Timed out at $stage",
            withTimeoutOrNull(30_000) {
                while (!check()) delay(20)
                true
            } == true,
        )
        println("UX_PROOF stage=$stage")
    }

    private suspend fun emojiGridVisible() {
        until("emoji-grid-visible") {
            runCatching {
                onView(withContentDescription(org.hamcrest.CoreMatchers.containsString("😀"))).check { view, error ->
                    if (error != null) throw error
                    assertTrue(view.isShown)
                }
            }.isSuccess
        }
    }

    private fun capture(name: String) {
        val activity = compose.activity
        activity.runOnUiThread {
            activity
                .getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                .hideSoftInputFromWindow(activity.window.decorView.windowToken, 0)
        }
        compose.waitUntil(10_000) {
            activity.window.decorView.rootWindowInsets
                ?.isVisible(
                    android.view.WindowInsets.Type
                        .ime(),
                ) != true
        }
        compose.waitForIdle()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val bitmap = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        val resolver = instrumentation.targetContext.contentResolver
        val values =
            ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, "$name.png")
                put(MediaStore.Images.Media.MIME_TYPE, "image/png")
                put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/XmtpMessengerProof")
            }
        val uri = checkNotNull(resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values))
        checkNotNull(resolver.openOutputStream(uri)).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL))
        val owner = checkNotNull(model.session.active.value)
        until("connected") { model.state.value.inbox == owner.client.inboxId() }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.inspectBackend = { backend ->
                SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = backend)))
            }
            model.onBackendProbeFinished = { _, _ -> }
            if (model.session.active.value != null) model.session.deleteAccount()
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun remoteHttpConfigurationIsRejectedBeforeThePublicSdkProbe() =
        runBlocking {
            try {
                model.session.signOut()
                until("start") { model.state.value.screen == Screen.START }
                val remote = "http://probe.example.test"
                val finished = CompletableDeferred<Unit>()
                var queries = 0
                model.inspectBackend = {
                    queries += 1
                    error("Remote HTTP reached the SDK configuration probe")
                }
                model.onBackendProbeFinished = { _, url -> if (url == remote) finished.complete(Unit) }
                model.dispatch(MessengerAction.InspectBackend(remote))
                withTimeout(30_000) { finished.await() }
                assertEquals(0, queries)
                assertNull(model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Credential").assertDoesNotExist()
                println("TRANSPORT_PROOF stage=remote-configuration-rejected-before-sdk-probe")
            } finally {
                cleanup()
            }
        }

    @Test fun credentialsUsePublishedAuthConfigurationAndIgnoreAnEarlierUrl() =
        runBlocking<Unit> {
            val release = CompletableDeferred<Unit>()
            try {
                model.session.signOut()
                until("start") { model.state.value.screen == Screen.START }
                val configuration =
                    SDKClient.fetchServerConfiguration(
                        BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    )
                assertFalse(configuration.auth.enabled)
                val publicUrl = "https://old.example.test"
                val requiredUrl = "https://current.example.test"
                val entered = CompletableDeferred<Unit>()
                val completed = CompletableDeferred<Unit>()
                val requiredCompleted = CompletableDeferred<Unit>()
                val disabledCompleted = CompletableDeferred<Unit>()
                val disabledUrl = "https://public.example.test"
                model.onBackendProbeFinished = { _, url ->
                    when (url) {
                        publicUrl -> completed.complete(Unit)
                        requiredUrl -> requiredCompleted.complete(Unit)
                        disabledUrl -> disabledCompleted.complete(Unit)
                    }
                }
                model.inspectBackend = { url ->
                    if (url == publicUrl) {
                        entered.complete(Unit)
                        withContext(NonCancellable) { release.await() }
                        configuration.copy(auth = configuration.auth.copy(enabled = false))
                    } else {
                        configuration.copy(auth = configuration.auth.copy(enabled = true))
                    }
                }
                compose.onNodeWithText("Backend URL").performTextReplacement(publicUrl)
                withTimeout(30_000) { entered.await() }
                compose.onNodeWithText("Credential").assertDoesNotExist()
                compose.onNodeWithText("Allow local attachment network").assertDoesNotExist()
                capture("ux-start-no-auth")
                compose.onNodeWithText("Backend URL").performTextReplacement(requiredUrl)
                withTimeout(30_000) { requiredCompleted.await() }
                assertEquals(requiredUrl, model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Credential").assertExists()
                capture("ux-start-auth")
                release.complete(Unit)
                withTimeout(30_000) { completed.await() }
                val required = model.state.value.credentialsRequiredFor
                println("UX_PROOF auth=$required")
                assertEquals(requiredUrl, model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Credential").assertExists()
                model.inspectBackend = { configuration.copy(auth = configuration.auth.copy(enabled = false)) }
                compose.onNodeWithText("Backend URL").performTextReplacement(disabledUrl)
                withTimeout(30_000) { disabledCompleted.await() }
                assertNull(model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Credential").assertDoesNotExist()
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun inlineQuickAndFullPickerSendActualReactionsWithMinimumTargets() =
        runBlocking<Unit> {
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Reaction picker"),
                    )
                val id = group.sendText("Reaction target")
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until("reaction-parent-loaded") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Reaction target").performClick()
                for (emoji in listOf("👍", "❤️", "😂", "😮", "😢")) {
                    compose.onNodeWithText(emoji).assertExists()
                }
                compose.onNodeWithContentDescription("More reactions").assertExists()
                capture("ux-inline-reactions")
                compose.onNodeWithText("👍").performClick()
                until("quick-native-reaction") {
                    val parent =
                        model.state.value.messages
                            .firstOrNull { it.id == id }
                    parent?.reactions?.any { it.emoji == "👍" && it.count == 1 } == true
                }
                compose.onNodeWithText("Reaction target").performClick()
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithText("Choose reaction").assertExists()
                until("bundled-offline-font-ready") { EmojiCompat.get().loadState == EmojiCompat.LOAD_STATE_SUCCEEDED }
                onView(isAssignableFrom(EmojiPickerView::class.java)).check { view, error ->
                    if (error != null) throw error
                    val picker = view as EmojiPickerView
                    val density = picker.resources.displayMetrics.density
                    val cellWidth = (picker.width - picker.paddingLeft - picker.paddingRight) / picker.emojiGridColumns
                    assertTrue(cellWidth / density >= 48)

                    fun visit(node: android.view.View) {
                        if (node is RecyclerView) {
                            val manager = node.layoutManager as? LinearLayoutManager
                            if (manager?.orientation == RecyclerView.HORIZONTAL) {
                                for (index in 0 until node.childCount) {
                                    assertTrue(node.getChildAt(index).width / density >= 48)
                                    assertTrue(node.getChildAt(index).height / density >= 48)
                                }
                            }
                        }
                        if (node is android.view.ViewGroup) repeat(node.childCount) { visit(node.getChildAt(it)) }
                    }
                    visit(picker)
                    val columns = picker.emojiGridColumns
                    val widthDp = picker.width / density
                    println("UX_PROOF columns=$columns widthDp=$widthDp")
                }
                emojiGridVisible()
                capture("ux-full-reactions")
                onView(withContentDescription(org.hamcrest.CoreMatchers.containsString("😀"))).perform(click())
                until("full-picker-native-reaction") {
                    val parent =
                        model.state.value.messages
                            .firstOrNull { it.id == id }
                    parent?.reactions?.any { it.emoji == "😀" && it.count == 1 } == true
                }
                compose.onNodeWithText("Choose reaction").assertDoesNotExist()
            } finally {
                cleanup()
            }
        }

    @Test fun replyQuotesOnlyFirstLineAndCancelKeepsTheNormalDraft() =
        runBlocking<Unit> {
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Reply composer"),
                    )
                val body = "Quote first line\nHidden second line"
                val id = group.sendText(body)
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until("reply-parent-loaded") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Message").performTextInput("unsent draft")
                compose.onNodeWithText(body).performClick()
                compose.onNodeWithText("Reply").performClick()
                until("normal-reply-target") { model.state.value.replyTo == id }
                assertEquals("Quote first line", model.state.value.replyPreview)
                compose.onNodeWithText("Quote first line", substring = false).assertExists()
                compose.onNodeWithText("unsent draft").assertExists()
                capture("ux-reply-composer")
                compose.onNodeWithContentDescription("Cancel reply").performClick()
                assertNull(model.state.value.replyTo)
                compose.onNodeWithText("unsent draft").assertExists()
                compose.onNodeWithText("Quote first line", substring = false).assertDoesNotExist()
                println("UX_PROOF stage=reply-cancel-kept-draft")
            } finally {
                cleanup()
            }
        }

    @Test fun fullPickerStillOpensWithTheSameBackendEndpointOffline() =
        runBlocking<Unit> {
            val proxy = AppOfflineBackendProxy(BuildConfig.XMTP_BACKEND_URL)
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(proxy.url, "", true)
                val first = checkNotNull(model.session.active.value)
                until("offline-picker-connected") { model.state.value.inbox == first.client.inboxId() }
                val group =
                    first.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Offline picker"),
                    )
                val id = group.sendText("Offline reaction target")
                val chatId = group.id()
                model.session.signOut()
                proxy.close()
                withContext(Dispatchers.IO) { proxy.assertUnavailable() }
                model.session.connect(proxy.url, "", true)
                val owner = checkNotNull(model.session.active.value)
                until("offline-picker-reopened") { model.state.value.inbox == owner.client.inboxId() }
                model.dispatch(MessengerAction.OpenConversation(chatId))
                until("offline-parent-loaded") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Offline reaction target").performClick()
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithText("Choose reaction").assertExists()
                until("offline-picker-font-ready") { EmojiCompat.get().loadState == EmojiCompat.LOAD_STATE_SUCCEEDED }
                onView(withContentDescription(org.hamcrest.CoreMatchers.containsString("😀"))).check { view, error ->
                    if (error != null) throw error
                    assertTrue(view.isShown)
                }
                println("UX_PROOF stage=same-endpoint-offline-grid-visible")
                compose.onNodeWithContentDescription("Close emoji picker").performClick()
                compose.onNodeWithText("Choose reaction").assertDoesNotExist()
            } finally {
                proxy.close()
                cleanup()
            }
        }

    private fun shell(command: String): String {
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        return android.os.ParcelFileDescriptor
            .AutoCloseInputStream(automation.executeShellCommand(command))
            .bufferedReader()
            .use { it.readText() }
    }

    @Test fun narrowPickerKeepsTargetsAndComposerClearAtDoubleTextScale() =
        runBlocking<Unit> {
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Narrow picker"),
                    )
                val id = group.sendText("Narrow reaction target")
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until("narrow-parent-loaded") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Narrow reaction target").performClick()
                for (emoji in listOf("👍", "❤️", "😂", "😮", "😢")) {
                    val bounds = compose.onNodeWithText(emoji).getUnclippedBoundsInRoot()
                    assertTrue((bounds.right - bounds.left).value >= 48)
                    assertTrue((bounds.bottom - bounds.top).value >= 48)
                }
                val popup = compose.onNodeWithTag("reaction-popover").fetchSemanticsNode().boundsInWindow
                val composer = compose.onNodeWithTag("message-composer").fetchSemanticsNode().boundsInWindow
                assertTrue("Popup must end above the composer", popup.bottom <= composer.top)
                assertTrue("Popup must fit 320 dp", popup.left >= 0 && popup.right <= 320)
                capture("ux-inline-reactions-200")
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithText("Choose reaction").assertExists()
                onView(isAssignableFrom(EmojiPickerView::class.java)).check { view, error ->
                    if (error != null) throw error
                    val picker = view as EmojiPickerView
                    assertEquals(2f, picker.resources.configuration.fontScale, 0.01f)
                    val density = picker.resources.displayMetrics.density
                    val cellWidth = picker.width / picker.emojiGridColumns / density
                    assertTrue("Full picker cell $cellWidth", cellWidth >= 48)
                    val columns = picker.emojiGridColumns
                    println("UX_PROOF narrow-columns=$columns")
                }
                emojiGridVisible()
                capture("ux-full-reactions-200")
                compose.onNodeWithContentDescription("Close emoji picker").performClick()
            } finally {
                cleanup()
            }
        }
}
