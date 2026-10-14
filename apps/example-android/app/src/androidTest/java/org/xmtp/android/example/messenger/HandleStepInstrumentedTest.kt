package org.xmtp.android.example.messenger

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestRule
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.app.AppGraph
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.feature.handles.HandleAction
import uniffi.xmtp_sdk.AndroidStreamLifecycle
import uniffi.xmtp_sdk.resumeStreams

/**
 * Choose your handle opens once for each account: it stays the first screen across restarts,
 * and a saved route does not replace it, until the user taps Skip for now.
 */
@RunWith(AndroidJUnit4::class)
class HandleStepInstrumentedTest {
    private val graphRule = TestAppGraphRule(offerHandleChoice = true)

    @get:Rule val rule: TestRule = graphRule
    private val stores = mutableListOf<ViewModelStore>()

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

    private suspend fun shell(saved: SavedStateHandle = SavedStateHandle()) {
        val store = ViewModelStore().also(stores::add)
        withContext(Dispatchers.Main) { graphRule.newShell(store, saved) }
    }

    /** A new process restores [profile] with Settings as its saved route. Returns the first route it opens. */
    private suspend fun restart(profile: String): AppRoute {
        val graph = graphRule.replaceGraph()
        graph.session.preferences.setSignedIn(true)
        shell(SavedStateHandle(mapOf("messenger.route.profile" to profile, "messenger.route.kind" to "settings")))
        until("The restart did not restore the account") {
            graph.session.active.value
                ?.key
                ?.profileId == profile && graph.destinations.state.value.route != AppRoute.Connect
        }
        return graph.destinations.state.value.route
    }

    @Test fun chooseYourHandleOpensOncePerAccount() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var profile: String? = null
            try {
                shell()
                val session = graphRule.graph.session
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL))
                profile = checkNotNull(session.active.value).key.profileId
                until("A new account did not open Choose your handle") {
                    graphRule.graph.destinations.state.value.route == AppRoute.ChooseHandle
                }
                // It is the first screen of the account. Back leaves the app, as on Chats.
                assertFalse(graphRule.graph.destinations.back())
                assertEquals(AppRoute.ChooseHandle, graphRule.graph.destinations.state.value.route)

                // Until the user skips, a restart opens the step again, not the saved route.
                assertEquals(AppRoute.ChooseHandle, restart(profile))

                val graph = graphRule.graph
                until("Skip for now did not open Chats") {
                    // The editor opens with the route, so Skip can arrive before it has an account.
                    if (graph.destinations.state.value.route == AppRoute.ChooseHandle) {
                        graph.handles.dispatch(HandleAction.Skip)
                    }
                    graph.destinations.state.value.route == AppRoute.Chats
                }
                // Skip for now is saved for the account: the next start opens the saved route.
                assertEquals(AppRoute.Settings, restart(profile))
            } finally {
                withContext(NonCancellable) {
                    withContext(Dispatchers.Main) { stores.forEach(ViewModelStore::clear) }
                    deleteAccount(graphRule.graph, profile)
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }

    private suspend fun deleteAccount(
        graph: AppGraph,
        profile: String?,
    ) {
        val session = graph.session
        if (session.active.value != null) {
            session.deleteAccount()
            return
        }
        session.preferences.profiles().firstOrNull { it.id == profile }?.let {
            session.preferences.setActive(it)
            session.deleteAccount()
        }
    }
}
