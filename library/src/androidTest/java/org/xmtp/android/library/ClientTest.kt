package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder
import org.xmtp.android.library.messages.generate
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import uniffi.xmtpv3.GenericException
import java.security.SecureRandom
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class ClientTest {
    @Test
    fun testTakesAWallet() {
        val fakeWallet = PrivateKeyBuilder()
        runBlocking { Client().create(account = fakeWallet) }
    }

    @Test
    fun testHasPrivateKeyBundleV1() {
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking { Client().create(account = fakeWallet) }
        assertEquals(1, client.privateKeyBundleV1.preKeysList?.size)
        val preKey = client.privateKeyBundleV1.preKeysList?.get(0)
        assert(preKey?.publicKey?.hasSignature() ?: false)
    }

    @Test
    fun testSerialization() {
        val wallet = PrivateKeyBuilder()
        val v1 =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = wallet)
        val encodedData = PrivateKeyBundleV1Builder.encodeData(v1)
        val v1Copy = PrivateKeyBundleV1Builder.fromEncodedData(encodedData)
        val client = runBlocking { Client().buildFrom(v1Copy) }
        assertEquals(
            wallet.address,
            client.address,
        )
    }

    @Test
    fun testCanBeCreatedWithBundle() {
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking { Client().create(account = fakeWallet) }
        val bundle = client.privateKeyBundle
        val clientFromV1Bundle = runBlocking { Client().buildFromBundle(bundle) }
        assertEquals(client.address, clientFromV1Bundle.address)
        assertEquals(
            client.privateKeyBundleV1.identityKey,
            clientFromV1Bundle.privateKeyBundleV1.identityKey,
        )
        assertEquals(
            client.privateKeyBundleV1.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1.preKeysList,
        )
    }

    @Test
    fun testCanBeCreatedWithV1Bundle() {
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking { Client().create(account = fakeWallet) }
        val bundleV1 = client.v1keys
        val clientFromV1Bundle = runBlocking { Client().buildFromV1Bundle(bundleV1) }
        assertEquals(client.address, clientFromV1Bundle.address)
        assertEquals(
            client.privateKeyBundleV1.identityKey,
            clientFromV1Bundle.privateKeyBundleV1.identityKey,
        )
        assertEquals(
            client.privateKeyBundleV1.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1.preKeysList,
        )
    }

    @Test
    fun testV3CanBeCreatedWithBundle() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            enableV3 = true,
            appContext = context,
            dbEncryptionKey = key
        )
        val client = runBlocking {
            Client().create(account = fakeWallet, options = options)
        }

        runBlocking {
            client.canMessageV3(listOf(client.address))[client.address]?.let { assert(it) }
        }

        val bundle = client.privateKeyBundle
        val clientFromV1Bundle = runBlocking {
            Client().buildFromBundle(bundle, options = options)
        }
        assertEquals(client.address, clientFromV1Bundle.address)
        assertEquals(
            client.privateKeyBundleV1.identityKey,
            clientFromV1Bundle.privateKeyBundleV1.identityKey,
        )

        runBlocking {
            clientFromV1Bundle.canMessageV3(listOf(client.address))[client.address]?.let { assert(it) }
        }

        assertEquals(
            client.address,
            clientFromV1Bundle.address
        )
    }

    @Test
    fun testCreatesAV3Client() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            enableV3 = true,
            appContext = context,
            dbEncryptionKey = key
        )
        val inboxId = runBlocking { Client.getOrCreateInboxId(options, fakeWallet.address) }
        val client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = options
            )
        }
        runBlocking {
            client.canMessageV3(listOf(client.address))[client.address]?.let { assert(it) }
        }
        assert(client.installationId.isNotEmpty())
        assertEquals(inboxId, client.inboxId)
    }

    @Test
    fun testCanDeleteDatabase() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val fakeWallet2 = PrivateKeyBuilder()
        var client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val client2 = runBlocking {
            Client().create(
                account = fakeWallet2,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        runBlocking {
            client.conversations.newGroup(listOf(client2.address))
            client.conversations.syncGroups()
            assertEquals(client.conversations.listGroups().size, 1)
        }

        assert(client.dbPath.isNotEmpty())
        client.deleteLocalDatabase()

        client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        runBlocking {
            client.conversations.syncGroups()
            assertEquals(client.conversations.listGroups().size, 0)
        }
    }

    @Test
    fun testCreatesAV3DevClient() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        runBlocking {
            client.canMessageV3(listOf(client.address))[client.address]?.let { assert(it) }
        }
    }

    @Test
    fun testCreatesAV3ProductionClient() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.PRODUCTION, true),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        runBlocking {
            client.canMessageV3(listOf(client.address))[client.address]?.let { assert(it) }
        }
    }

    @Test
    fun testDoesNotCreateAV3Client() {
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking { Client().create(account = fakeWallet) }
        assertThrows("Error no V3 client initialized", XMTPException::class.java) {
            runBlocking {
                client.canMessageV3(listOf(client.address))[client.address]?.let { assert(!it) }
            }
        }
    }

    @Test
    fun testCanMessage() {
        val fixtures = fixtures()
        val notOnNetwork = PrivateKeyBuilder()
        val canMessage = runBlocking { fixtures.aliceClient.canMessage(fixtures.bobClient.address) }
        val cannotMessage = runBlocking { fixtures.aliceClient.canMessage(notOnNetwork.address) }
        assert(canMessage)
        assert(!cannotMessage)
    }

    @Test
    fun testPublicCanMessage() {
        val aliceWallet = PrivateKeyBuilder()
        val notOnNetwork = PrivateKeyBuilder()
        val opts = ClientOptions(ClientOptions.Api(XMTPEnvironment.LOCAL, false))
        val aliceClient = runBlocking {
            Client().create(aliceWallet, opts)
        }
        runBlocking { aliceClient.ensureUserContactPublished() }

        val canMessage = runBlocking { Client.canMessage(aliceWallet.address, opts) }
        val cannotMessage = runBlocking { Client.canMessage(notOnNetwork.address, opts) }

        assert(canMessage)
        assert(!cannotMessage)
    }

    @Test
    fun testPreEnableIdentityCallback() {
        val fakeWallet = PrivateKeyBuilder()
        val expectation = CompletableFuture<Unit>()

        val preEnableIdentityCallback: suspend () -> Unit = {
            expectation.complete(Unit)
        }

        val opts = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            preEnableIdentityCallback = preEnableIdentityCallback
        )

        try {
            runBlocking {
                Client().create(account = fakeWallet, options = opts)
            }
            expectation.get(5, TimeUnit.SECONDS)
        } catch (e: Exception) {
            fail("Error: $e")
        }
    }

    @Test
    fun testPreCreateIdentityCallback() {
        val fakeWallet = PrivateKeyBuilder()
        val expectation = CompletableFuture<Unit>()

        val preCreateIdentityCallback: suspend () -> Unit = {
            expectation.complete(Unit)
        }

        val opts = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            preCreateIdentityCallback = preCreateIdentityCallback
        )

        try {
            runBlocking { Client().create(account = fakeWallet, options = opts) }
            expectation.get(5, TimeUnit.SECONDS)
        } catch (e: Exception) {
            fail("Error: $e")
        }
    }

    @Test
    fun testPreAuthenticateToInboxCallback() {
        val fakeWallet = PrivateKeyBuilder()
        val expectation = CompletableFuture<Unit>()
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext

        val preAuthenticateToInboxCallback: suspend () -> Unit = {
            expectation.complete(Unit)
        }

        val opts = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            preAuthenticateToInboxCallback = preAuthenticateToInboxCallback,
            enableV3 = true,
            appContext = context,
            dbEncryptionKey = key
        )

        try {
            runBlocking { Client().create(account = fakeWallet, options = opts) }
            expectation.get(5, TimeUnit.SECONDS)
        } catch (e: Exception) {
            fail("Error: $e")
        }
    }

    @Test
    fun testCanDropReconnectDatabase() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val fakeWallet2 = PrivateKeyBuilder()
        val boClient = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val alixClient = runBlocking {
            Client().create(
                account = fakeWallet2,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.address))
            boClient.conversations.syncGroups()
        }

        runBlocking {
            assertEquals(boClient.conversations.listGroups().size, 1)
        }

        boClient.dropLocalDatabaseConnection()

        assertThrows(
            "Client error: storage error: Pool needs to  reconnect before use",
            GenericException::class.java
        ) { runBlocking { boClient.conversations.listGroups() } }

        runBlocking { boClient.reconnectLocalDatabase() }

        runBlocking {
            assertEquals(boClient.conversations.listGroups().size, 1)
        }
    }

    @Test
    fun testCanGetAnInboxIdFromAddress() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val boWallet = PrivateKeyBuilder()
        val alixClient = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val boClient = runBlocking {
            Client().create(
                account = boWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val boInboxId = runBlocking {
            alixClient.inboxIdFromAddress(boClient.address)
        }
        assertEquals(boClient.inboxId, boInboxId)
    }
}
