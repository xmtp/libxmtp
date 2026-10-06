package uniffi.xmtp_sdk

import android.os.Looper
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.*
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import org.xmtp.android.library.BuildConfig
import java.io.File
import java.security.SecureRandom
import java.util.Collections
import java.util.UUID

/**
 * Check both Context factories in one process with one shared registration.
 * The process observer is registered once per process, so the registration
 * test runs first (name order) and the live lifecycle test uses that observer.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class AndroidContextStartupTest {
    private val context
        get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    private fun options(label: String) =
        ClientOptions(
            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
            storage =
                StorageOptions(
                    location = StorageLocation.Default,
                    label = label,
                    encryptionKey = ByteArray(32).also { SecureRandom().nextBytes(it) },
                    pool = StoragePoolOptions(min = 2u, max = 5u),
                ),
            deviceSync = false,
        )

    private suspend fun checkStorage(
        client: SDKClient,
        configuration: ClientOptions,
    ) {
        val storage = client.options().storage
        assertEquals(StorageOptions(context).location, storage.location)
        assertEquals(configuration.storage.label, storage.label)
        assertNull("public options must redact the encryption key", storage.encryptionKey)
        assertEquals(configuration.storage.pool, storage.pool)
        // verifies: STORE-003
        // The Default location from a real Context opens under filesDir/xmtp_db.
        val root = File(context.filesDir, "xmtp_db").canonicalPath
        val path = File(checkNotNull(client.storage().path())).canonicalPath
        assertTrue("database must stay under the app storage root", path.startsWith(root + File.separator))
        val salt = File("$path.sqlcipher_salt")
        assertTrue("configured SQLCipher storage must have its salt file", salt.isFile)
        assertEquals("SQLCipher salt size", 32L, salt.length())
    }

    @Test fun contextFactoriesShareMainLifecycleRegistration() =
        runBlocking {
            val expectedObserverCount =
                withContext(Dispatchers.Main) {
                    (ProcessLifecycleOwner.get().lifecycle as LifecycleRegistry).observerCount + 1
                }
            firstContextCreateWaitsForMainLifecycleRegistration(expectedObserverCount)
            firstContextBuildWaitsForMainLifecycleRegistration(expectedObserverCount)
        }

    private suspend fun firstContextCreateWaitsForMainLifecycleRegistration(expectedObserverCount: Int) {
        withTimeout(30_000) {
            withContext(Dispatchers.Main) {
                assertSame(Looper.getMainLooper(), Looper.myLooper())
                assertTrue("process lifecycle control must be enabled", AndroidStreamLifecycle.enabled)
                val lifecycle = ProcessLifecycleOwner.get().lifecycle as LifecycleRegistry
                val configuration = options("main-create-${UUID.randomUUID()}")
                val client = SDKClient.create(context, generateLocalSigner(), configuration)
                try {
                    assertSame(Looper.getMainLooper(), Looper.myLooper())
                    assertEquals(
                        "Context create must register its process observer before return",
                        expectedObserverCount,
                        lifecycle.observerCount,
                    )
                    checkStorage(client, configuration)
                } finally {
                    withContext(NonCancellable + Dispatchers.IO) { client.storage().delete() }
                }
            }
        }
    }

    private suspend fun firstContextBuildWaitsForMainLifecycleRegistration(expectedObserverCount: Int) {
        withTimeout(30_000) {
            val signer = generateLocalSigner()
            val configuration = options("main-build-${UUID.randomUUID()}")
            val fixture =
                withContext(Dispatchers.IO) {
                    SDKClient.create(
                        signer,
                        configuration.copy(
                            storage = configuration.storage.copy(location = StorageOptions(context).location),
                        ),
                    )
                }
            val inbox: InboxId
            val path: String
            try {
                inbox = fixture.inboxId()
                path = checkNotNull(fixture.storage().path())
            } finally {
                withContext(NonCancellable + Dispatchers.IO) { fixture.end() }
            }
            withContext(Dispatchers.Main) {
                assertSame(Looper.getMainLooper(), Looper.myLooper())
                assertTrue("process lifecycle control must be enabled", AndroidStreamLifecycle.enabled)
                val lifecycle = ProcessLifecycleOwner.get().lifecycle as LifecycleRegistry
                val client = SDKClient.build(context, signer.identity(), configuration, inboxId = inbox)
                try {
                    assertSame(Looper.getMainLooper(), Looper.myLooper())
                    assertEquals(
                        "Context build must reuse the registered process observer before return",
                        expectedObserverCount,
                        lifecycle.observerCount,
                    )
                    assertEquals(inbox, client.inboxId())
                    assertEquals(path, client.storage().path())
                    checkStorage(client, configuration)
                } finally {
                    withContext(NonCancellable + Dispatchers.IO) { client.storage().delete() }
                }
            }
        }
    }

    private suspend fun dispatch(event: Lifecycle.Event) =
        withContext(Dispatchers.Main) {
            (ProcessLifecycleOwner.get().lifecycle as LifecycleRegistry).handleLifecycleEvent(event)
        }

    private suspend fun eventually(
        timeoutMs: Long,
        condition: () -> Boolean,
    ): Boolean =
        withTimeoutOrNull(timeoutMs) {
            while (!condition()) delay(50)
            true
        } ?: false

    /**
     * Real ON_STOP and ON_START events on the process lifecycle reach the
     * observer that the Context factory registered. ON_STOP suspends a live
     * stream: the reader reports RECONNECTING and a peer message does not
     * arrive. ON_START resumes it, and the same stream delivers that message.
     * The test leaves the process started, so later tests keep live streams.
     */
    @Test fun processStopSuspendsAndProcessStartResumesALiveStream() =
        runBlocking {
            val previous = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = true
            try {
                withTimeout(180_000) {
                    val options =
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        )
                    val sender = SDKClient.create(context, generateLocalSigner(), options)
                    try {
                        val receiver = SDKClient.create(context, generateLocalSigner(), options)
                        try {
                            checkLifecycleCycle(sender, receiver)
                        } finally {
                            withContext(NonCancellable) { receiver.end() }
                        }
                    } finally {
                        withContext(NonCancellable) { sender.end() }
                    }
                }
            } finally {
                // Resume on every exit, so later tests in this process keep live streams.
                withContext(NonCancellable) { dispatch(Lifecycle.Event.ON_START) }
                AndroidStreamLifecycle.enabled = previous
            }
        }

    private suspend fun checkLifecycleCycle(
        sender: SDKClient,
        receiver: SDKClient,
    ) = kotlinx.coroutines.coroutineScope {
        dispatch(Lifecycle.Event.ON_START)
        val group = sender.conversations.createGroup(listOf(receiver.inboxId()))
        receiver.conversations.sync()
        val joined = (checkNotNull(receiver.conversations.getById(group.id())) as Conversation.Group).group
        val received = Collections.synchronizedList(mutableListOf<MessageId>())
        val states = Collections.synchronizedList(mutableListOf<ConnectionState>())
        val stream =
            launch(Dispatchers.Default) {
                joined
                    .streamMessages(
                        ConversationMessageStreamOptions(onConnectionStateChange = {
                            _,
                            current,
                            ->
                            states.add(current)
                        }),
                    ).collect { received.add(it.id) }
            }
        try {
            val sentAt = System.nanoTime()
            val foreground = group.sendText("foreground")
            assertTrue("The stream did not deliver before ON_STOP", eventually(30_000) { foreground in received })
            // A slow backend cannot hide a stream that was not suspended.
            val window = maxOf(3_000L, 3 * (System.nanoTime() - sentAt) / 1_000_000)

            val stopStart = states.size
            dispatch(Lifecycle.Event.ON_STOP)
            val background: MessageId
            val deliveredWhileStopped: Boolean
            try {
                assertTrue(
                    "ON_STOP did not suspend the stream: $states",
                    eventually(30_000) {
                        synchronized(states) { states.drop(stopStart).contains(ConnectionState.RECONNECTING) }
                    },
                )
                background = group.sendText("background")
                deliveredWhileStopped = eventually(window) { background in received }
            } finally {
                // Resume before any assertion can stop the test.
                dispatch(Lifecycle.Event.ON_START)
            }
            assertFalse("A stopped process received a network message in $window ms", deliveredWhileStopped)
            assertTrue("ON_START did not resume the stream", eventually(60_000) { background in received })
        } finally {
            stream.cancelAndJoin()
        }
    }
}
