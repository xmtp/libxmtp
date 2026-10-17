package org.xmtp.android.library

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.SecureRandom

/** Each test owns its clients and database directories. */
abstract class BaseInstrumentedTest {
    private var previousManageStreamLifecycle = true
    private val createdClients = mutableListOf<SDKClient>()
    private val dbFolders = mutableListOf<String>()

    @get:Rule val testDbDir = TemporaryFolder()
    protected val dbEncryptionKey = SecureRandom().generateSeed(32)
    protected val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before open fun setUp() =
        runBlocking {
            previousManageStreamLifecycle = AndroidStreamLifecycle.enabled
            // Ordinary backend tests do not own a foreground Activity.
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
        }

    @After open fun tearDown() {
        try {
            runBlocking {
                withContext(NonCancellable) {
                    createdClients.forEach { it.end() }
                }
            }
            createdClients.clear()
            dbFolders.forEach { File(it).deleteRecursively() }
            dbFolders.clear()
        } finally {
            AndroidStreamLifecycle.enabled = previousManageStreamLifecycle
        }
    }

    protected fun trackClient(client: SDKClient): SDKClient = client.also { createdClients.add(it) }

    protected suspend fun createClient(
        account: Signer,
        api: BackendOptions = localApi(),
        deviceSyncEnabled: Boolean = true,
        codecs: List<ContentCodec<*>> = emptyList(),
    ): SDKClient =
        SDKClient
            .create(
                context,
                account,
                createClientOptions(api, deviceSyncEnabled = deviceSyncEnabled),
                codecs,
            ).let(::trackClient)

    protected suspend fun createFixtures(api: BackendOptions = localApi()): TestFixtures {
        val alixAccount = generateLocalSigner()
        val boAccount = generateLocalSigner()
        val caroAccount = generateLocalSigner()
        val (alixClient, boClient, caroClient) =
            coroutineScope {
                val alix = async { createClient(alixAccount, api) }
                val bo = async { createClient(boAccount, api) }
                val caro = async { createClient(caroAccount, api) }
                Triple(alix.await(), bo.await(), caro.await())
            }
        return TestFixtures(
            alixAccount,
            alixAccount.identity(),
            alixClient,
            boAccount,
            boAccount.identity(),
            boClient,
            caroAccount,
            caroAccount.identity(),
            caroClient,
        )
    }

    protected fun createClientOptions(
        api: BackendOptions = localApi(),
        dbDirectory: String? = null,
        deviceSyncEnabled: Boolean = true,
    ): ClientOptions {
        val directory = dbDirectory ?: testDbDir.newFolder().absolutePath
        dbFolders.add(directory)
        return ClientOptions(
            backend = BackendSource.Options(api),
            storage = StorageOptions(StorageLocation.Directory(directory), encryptionKey = dbEncryptionKey),
            deviceSync = deviceSyncEnabled,
        )
    }

    protected suspend fun createWallet(): Signer = generateLocalSigner()
}

data class TestFixtures(
    val alixAccount: Signer,
    val alix: PublicIdentity,
    val alixClient: SDKClient,
    val boAccount: Signer,
    val bo: PublicIdentity,
    val boClient: SDKClient,
    val caroAccount: Signer,
    val caro: PublicIdentity,
    val caroClient: SDKClient,
)
