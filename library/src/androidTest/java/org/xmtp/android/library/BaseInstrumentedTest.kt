package org.xmtp.android.library

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.io.File
import java.security.SecureRandom

/**
 * Base class for instrumented tests that provides automatic resource management
 * and cleanup to prevent memory leaks.
 *
 * This class handles:
 * - Automatic client cleanup after each test
 * - Unique database directories for test isolation
 * - Memory management and garbage collection
 * - Database file cleanup
 */
abstract class BaseInstrumentedTest {
    private val createdClients = mutableListOf<Client>()
    private val dbFolders = mutableListOf<String>()

    @get:Rule
    private val testDbDir = TemporaryFolder()
    protected val dbEncryptionKey: ByteArray = SecureRandom().generateSeed(32)

    protected val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    open fun setUp() {
        testDbDir.create()
    }

    @After
    open fun tearDown() {
        // Clean up all clients
        runBlocking {
            createdClients.forEach { client ->
                try {
                    client.dropLocalDatabaseConnection()
                } catch (e: Exception) {
                    // Log but don't fail the test cleanup
                    println("Warning: Failed to delete database for client: ${e.message}")
                }
            }
        }

        // Clear the client list
        createdClients.clear()
        dbFolders.forEach {
            try {
                File(it).deleteRecursively()
            } catch (e: Exception) {
            }
        }
        dbFolders.clear()
        // Force garbage collection to help with native memory cleanup
        System.gc()
    }

    /**
     * Creates a client with automatic cleanup tracking.
     * This is the primary method for creating clients in tests.
     */
    protected suspend fun createClient(
        account: SigningKey,
        api: ClientOptions.Api = ClientOptions.Api(XMTPEnvironment.LOCAL, false),
        deviceSyncEnabled: Boolean = true,
    ): Client {
        val options = createClientOptions(api, deviceSyncEnabled = deviceSyncEnabled)
        val client = Client.create(account = account, options = options)
        createdClients.add(client)
        return client
    }

    /**
     * Creates a standard fixtures setup with automatic cleanup.
     * Returns the 5 standard test clients: alix, bo, caro, davon, eri.
     */
    protected suspend fun createFixtures(api: ClientOptions.Api = ClientOptions.Api(XMTPEnvironment.LOCAL, false)): TestFixtures {
        //  Create accounts
        val alixAccount = PrivateKeyBuilder()
        val boAccount = PrivateKeyBuilder()
        val caroAccount = PrivateKeyBuilder()

        // Create clients concurrently
        val (alixClient, boClient, caroClient) =
            coroutineScope {
                val alixDeferred = async { createClient(alixAccount, api) }
                val boDeferred = async { createClient(boAccount, api) }
                val caroDeferred = async { createClient(caroAccount, api) }
                Triple(alixDeferred.await(), boDeferred.await(), caroDeferred.await())
            }

        return TestFixtures(
            alixAccount = alixAccount,
            alix = alixAccount.getPrivateKey(),
            alixClient = alixClient,
            boAccount = boAccount,
            bo = boAccount.getPrivateKey(),
            boClient = boClient,
            caroAccount = caroAccount,
            caro = caroAccount.getPrivateKey(),
            caroClient = caroClient,
        )
    }

    private fun randomSubfolder(): String {
        val clientDbDir = testDbDir.newFolder()
        clientDbDir.mkdirs()

        return clientDbDir.absolutePath
    }

    /**
     * Creates client options with unique database directory for this test.
     */
    protected fun createClientOptions(
        api: ClientOptions.Api,
        dbDirectory: String? = null,
        deviceSyncEnabled: Boolean,
    ): ClientOptions {
        val finalDbDirectory = dbDirectory ?: randomSubfolder()
        dbFolders.add(finalDbDirectory)

        return ClientOptions(
            api = api,
            dbEncryptionKey = dbEncryptionKey,
            appContext = context,
            dbDirectory = finalDbDirectory,
            deviceSyncEnabled = deviceSyncEnabled,
        )
    }

    /**
     * Helper method to create a test wallet.
     */
    protected fun createWallet(): PrivateKeyBuilder = PrivateKeyBuilder()
}

/**
 * Data class representing the standard test fixtures.
 */
data class TestFixtures(
    val alixAccount: PrivateKeyBuilder,
    val alix: org.xmtp.android.library.messages.PrivateKey,
    val alixClient: Client,
    val boAccount: PrivateKeyBuilder,
    val bo: org.xmtp.android.library.messages.PrivateKey,
    val boClient: Client,
    val caroAccount: PrivateKeyBuilder,
    val caro: org.xmtp.android.library.messages.PrivateKey,
    val caroClient: Client,
)
