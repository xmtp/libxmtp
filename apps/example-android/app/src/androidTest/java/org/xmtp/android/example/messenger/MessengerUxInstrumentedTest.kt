package org.xmtp.android.example.messenger

import android.graphics.Bitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

class MessengerUxInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
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
        try {
            writeProofScreenshot(instrumentation.targetContext, name, bitmap)
        } finally {
            bitmap.recycle()
        }
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.connect.inspectBackend = { backend ->
                SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = backend)))
            }
            model.connect.onBackendProbeFinished = { _, _ -> }
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
                model.connect.inspectBackend = { url ->
                    if (url == remote) {
                        queries += 1
                        error("Remote HTTP reached the SDK configuration probe")
                    }
                    SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = url)))
                }
                model.connect.onBackendProbeFinished = { _, url -> if (url == remote) finished.complete(Unit) }
                model.dispatch(MessengerAction.InspectBackend(remote))
                withTimeout(30_000) { finished.await() }
                assertEquals(0, queries)
                assertNull(model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Access token").assertDoesNotExist()
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
                model.connect.onBackendProbeFinished = { _, url ->
                    when (url) {
                        publicUrl -> completed.complete(Unit)
                        requiredUrl -> requiredCompleted.complete(Unit)
                        disabledUrl -> disabledCompleted.complete(Unit)
                    }
                }
                model.connect.inspectBackend = { url ->
                    if (url == publicUrl) {
                        entered.complete(Unit)
                        withContext(NonCancellable) { release.await() }
                        configuration.copy(auth = configuration.auth.copy(enabled = false))
                    } else {
                        configuration.copy(auth = configuration.auth.copy(enabled = true))
                    }
                }
                compose.onNodeWithText("Server address").performTextReplacement(publicUrl)
                withTimeout(30_000) { entered.await() }
                compose.onNodeWithText("Access token").assertDoesNotExist()
                compose.onNodeWithText("Allow local attachment network").assertDoesNotExist()
                capture("ux-start-no-auth")
                compose.onNodeWithText("Server address").performTextReplacement(requiredUrl)
                withTimeout(30_000) { requiredCompleted.await() }
                assertEquals(requiredUrl, model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Access token").assertExists()
                capture("ux-start-auth")
                release.complete(Unit)
                withTimeout(30_000) { completed.await() }
                val required = model.state.value.credentialsRequiredFor
                println("UX_PROOF auth=$required")
                assertEquals(requiredUrl, model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Access token").assertExists()
                model.connect.inspectBackend = { configuration.copy(auth = configuration.auth.copy(enabled = false)) }
                compose.onNodeWithText("Server address").performTextReplacement(disabledUrl)
                withTimeout(30_000) { disabledCompleted.await() }
                assertNull(model.state.value.credentialsRequiredFor)
                compose.onNodeWithText("Access token").assertDoesNotExist()
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }
}
