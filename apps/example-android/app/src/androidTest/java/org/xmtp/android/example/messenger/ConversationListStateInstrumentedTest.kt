package org.xmtp.android.example.messenger

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.shared.MessengerAction
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

@RunWith(AndroidJUnit4::class)
class ConversationListStateInstrumentedTest {
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

    @Test fun fiftyGroupsUseOneNativeStateEachAndKeepListProjection() =
        runBlocking {
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val application = instrumentation.targetContext.applicationContext as ExampleApp
            val store = ViewModelStore()
            val model =
                ViewModelProvider(
                    store,
                    ViewModelProvider.AndroidViewModelFactory(application),
                )[MessengerViewModel::class.java]
            val initialRefresh = CompletableDeferred<Unit>()
            val releaseRefresh = CompletableDeferred<Unit>()
            var peer: SDKClient? = null
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.beforeActiveRefresh = {
                    initialRefresh.complete(Unit)
                    releaseRefresh.await()
                }
                model.session.onMessage = { _, _ -> }
                model.session.onInvalidated = {}
                val backend = BuildConfig.XMTP_BACKEND_URL
                model.session.connect(backend, "", localAttachmentNetwork(backend))
                val owner = checkNotNull(model.session.active.value)
                until("initial list refresh held") { initialRefresh.isCompleted }
                val groups =
                    (0 until 50).map { index ->
                        owner.client.conversations.createGroup(
                            emptyList(),
                            CreateGroupOptions(name = if (index == 0) "" else "List group $index"),
                        )
                    }
                val previewId = groups[1].sendText("List preview")
                until("native list preview published") {
                    owner.client.conversations
                        .getMessageById(previewId)
                        ?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                releaseRefresh.complete(Unit)
                until("initial fifty-group list ready") { model.state.value.conversations.size == 50 }
                model.beforeActiveRefresh = {}
                val calls = ConcurrentHashMap<String, AtomicInteger>()
                model.listGroupStateRead = { group ->
                    calls.computeIfAbsent(group.id()) { AtomicInteger() }.incrementAndGet()
                    group.state()
                }
                model.refreshList(owner)
                assertEquals("Each of 50 groups needs one native state call", 50, calls.values.sumOf { it.get() })
                assertEquals(groups.map { it.id() }.toSet(), calls.keys)
                assertTrue("No group needs a second native state call", calls.values.all { it.get() == 1 })
                val rows =
                    model.state.value.conversations
                        .associateBy { it.id }
                assertEquals(groups.map { it.id() }.toSet(), rows.keys)
                groups.forEachIndexed { index, group ->
                    val row = checkNotNull(rows[group.id()])
                    assertEquals(if (index == 0) "Group" else "List group $index", row.title)
                    assertEquals(if (index == 1) "List preview" else "", row.preview)
                    assertEquals("0", row.unread)
                    assertFalse(row.unknown)
                    assertEquals(group.id().hashCode(), row.pattern)
                }
                assertTrue(checkNotNull(rows[groups[1].id()]).time.isNotEmpty())
                println("LIST_STATE_PROOF stage=fifty-native-group-state-calls-unchanged-projection calls=50")

                peer =
                    SDKClient.create(
                        application,
                        localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = backend)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val second = checkNotNull(peer)
                val dm = owner.client.conversations.createDm(second.inboxId())
                dm.updateConsentState(ConsentState.ALLOWED)
                groups[49].updateConsentState(ConsentState.DENIED)
                calls.clear()
                model.refreshList(owner)
                assertEquals("The DM path adds no GroupState read", 49, calls.values.sumOf { it.get() })
                val mixed = model.state.value.conversations
                assertEquals(50, mixed.size)
                assertFalse(mixed.any { it.id == groups[49].id() })
                val dmRow = mixed.single { it.id == dm.id() }
                assertEquals(second.inboxId().take(12), dmRow.title)
                assertEquals("", dmRow.preview)
                assertEquals("0", dmRow.unread)
                assertFalse(dmRow.unknown)

                groups[49].updateConsentState(ConsentState.UNKNOWN)
                model.dispatch(MessengerAction.SelectTab(true))
                until("native unknown group projection") {
                    model.state.value.conversations
                        .singleOrNull()
                        ?.let { it.id == groups[49].id() && it.unknown } ==
                        true
                }
                assertEquals(
                    "List group 49",
                    model.state.value.conversations
                        .single()
                        .title,
                )
                println("LIST_STATE_PROOF stage=dm-and-unknown-consent-projection-unchanged")
            } finally {
                releaseRefresh.complete(Unit)
                withContext(NonCancellable) {
                    model.beforeActiveRefresh = {}
                    model.listGroupStateRead = { it.state() }
                    peer?.end()
                    if (model.session.active.value != null) model.session.deleteAccount()
                    model.session.signOut()
                    instrumentation.runOnMainSync { store.clear() }
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
}
