package uniffi.xmtp_sdk

import android.os.Looper
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.BuildConfig
import java.io.File
import java.security.SecureRandom
import java.util.UUID

/** Run each first factory case in a fresh Android process. */
@RunWith(AndroidJUnit4::class)
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
        assertArrayEquals(configuration.storage.encryptionKey, storage.encryptionKey)
        assertEquals(configuration.storage.pool, storage.pool)
        val root = File(context.filesDir, "xmtp_db/${configuration.storage.label}").canonicalPath
        assertTrue(checkNotNull(client.storage().path()).startsWith(root + File.separator))
    }

    @Test fun firstContextCreateWaitsForMainLifecycleRegistration() =
        runBlocking {
            withTimeout(30_000) {
                withContext(Dispatchers.Main) {
                    assertSame(Looper.getMainLooper(), Looper.myLooper())
                    assertTrue("process lifecycle control must be enabled", AndroidStreamLifecycle.enabled)
                    val lifecycle = ProcessLifecycleOwner.get().lifecycle as LifecycleRegistry
                    val before = lifecycle.observerCount
                    val configuration = options("main-create-${UUID.randomUUID()}")
                    val client = SDKClient.create(context, generateLocalSigner(), configuration)
                    try {
                        assertSame(Looper.getMainLooper(), Looper.myLooper())
                        assertEquals(
                            "Context create must register its process observer before return",
                            before + 1,
                            lifecycle.observerCount,
                        )
                        checkStorage(client, configuration)
                    } finally {
                        withContext(NonCancellable + Dispatchers.IO) { client.storage().delete() }
                    }
                }
            }
        }

    @Test fun firstContextBuildWaitsForMainLifecycleRegistration() =
        runBlocking {
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
                    val before = lifecycle.observerCount
                    val client = SDKClient.build(context, signer.identity(), configuration, inboxId = inbox)
                    try {
                        assertSame(Looper.getMainLooper(), Looper.myLooper())
                        assertEquals(
                            "Context build must register its process observer before return",
                            before + 1,
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
}
