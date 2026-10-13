package org.xmtp.android.example.messenger

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.rules.TestRule
import org.junit.runner.Description
import org.junit.runners.model.Statement
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.messenger.app.AppGraph
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.data.xmtp.XmtpQueries
import org.xmtp.android.example.messenger.notifications.PushTransport
import org.xmtp.android.example.messenger.session.SessionObservers
import uniffi.xmtp_sdk.ClientEvent
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.SDKClient

/**
 * Install an owned production graph before the Activity creates its controller. Tests start
 * from Chats, so Choose your handle is off unless [offerHandleChoice] turns it on.
 */
internal class TestAppGraphRule(
    private val notificationTransport: PushTransport? = null,
    private val conversationQueries: ConversationQueries = XmtpQueries(),
    private val offerHandleChoice: Boolean = false,
) : TestRule {
    val observers = TestGraphObservers()
    lateinit var graph: AppGraph
        private set

    fun newShell(store: ViewModelStore): MessengerViewModel =
        ViewModelProvider(
            store,
            object : ViewModelProvider.Factory {
                @Suppress("UNCHECKED_CAST")
                override fun <T : ViewModel> create(modelClass: Class<T>): T {
                    check(modelClass == MessengerViewModel::class.java)
                    return MessengerViewModel(graph, SavedStateHandle()) as T
                }
            },
        )[MessengerViewModel::class.java]

    suspend fun replaceGraph(): AppGraph {
        graph.session.signOut()
        graph.close()
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as ExampleApp
        graph =
            if (notificationTransport == null) {
                AppGraph(
                    app,
                    sessionObservers = observers.fixed(),
                    conversationQueries = conversationQueries,
                    offerHandleChoice = offerHandleChoice,
                )
            } else {
                AppGraph(
                    app,
                    sessionObservers = observers.fixed(),
                    notificationTransport = notificationTransport,
                    conversationQueries = conversationQueries,
                    offerHandleChoice = offerHandleChoice,
                )
            }
        app.graph = graph
        return graph
    }

    override fun apply(
        base: Statement,
        description: Description,
    ) = object : Statement() {
        override fun evaluate() =
            runBlocking {
                val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as ExampleApp
                val previous = app.graph
                previous.session.signOut()
                graph =
                    if (notificationTransport == null) {
                        AppGraph(
                            app,
                            sessionObservers = observers.fixed(),
                            conversationQueries = conversationQueries,
                            offerHandleChoice = offerHandleChoice,
                        )
                    } else {
                        AppGraph(
                            app,
                            sessionObservers = observers.fixed(),
                            notificationTransport = notificationTransport,
                            conversationQueries = conversationQueries,
                            offerHandleChoice = offerHandleChoice,
                        )
                    }
                app.graph = graph
                try {
                    base.evaluate()
                } finally {
                    withContext(NonCancellable) {
                        try {
                            graph.session.signOut()
                        } finally {
                            graph.close()
                            app.graph = previous
                        }
                    }
                }
            }
    }
}

/** Latches surround the real handler. They never supply its projected result. */
internal class TestGraphObservers {
    var beforeHandle: suspend (ActiveSession, Message) -> Unit = { _, _ -> }
    var afterHandle: suspend (ActiveSession, Message) -> Unit = { _, _ -> }
    var beforeInvalidate: suspend (ActiveSession) -> Unit = {}
    var beforeEvent: suspend (ActiveSession, ClientEvent) -> Unit = { _, _ -> }
    var onSessionInvalidated: () -> Unit = {}
    var afterAdmissionChanged: (ActiveSession) -> Unit = {}
    var beforeUnregister: suspend (SDKClient) -> Unit = {}
    var beforeEnd: suspend (ActiveSession) -> Unit = {}
    var beforeProfileRemoval: suspend (String) -> Unit = {}
    var beforeStoredNotificationStop: suspend (ActiveSession) -> Unit = {}

    fun pauseAutomaticProjection() {
        beforeHandle = { _, _ -> awaitCancellation() }
        beforeInvalidate = { awaitCancellation() }
    }

    fun fixed() =
        SessionObservers(
            beforeHandle = { owner, message -> beforeHandle(owner, message) },
            afterHandle = { owner, message -> afterHandle(owner, message) },
            beforeInvalidate = { owner -> beforeInvalidate(owner) },
            beforeEvent = { owner, event -> beforeEvent(owner, event) },
            onSessionInvalidated = { onSessionInvalidated() },
            afterAdmissionChanged = { owner -> afterAdmissionChanged(owner) },
            beforeUnregister = { client -> beforeUnregister(client) },
            beforeEnd = { owner -> beforeEnd(owner) },
            beforeProfileRemoval = { profile -> beforeProfileRemoval(profile) },
            beforeStoredNotificationStop = { owner -> beforeStoredNotificationStop(owner) },
        )
}
