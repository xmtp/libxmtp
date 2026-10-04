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

/** Check both Context factories in one process with one shared registration. */
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
        assertNull("public options must redact the encryption key", storage.encryptionKey)
        assertEquals(configuration.storage.pool, storage.pool)
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
}
