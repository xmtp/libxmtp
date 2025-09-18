package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.Client.Companion.ffiApplySignatureRequest
import org.xmtp.android.library.Client.Companion.ffiRevokeInstallations
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.FfiLogLevel
import uniffi.xmtpv3.FfiLogRotation
import uniffi.xmtpv3.GenericException
import java.io.File
import java.security.SecureRandom
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class ClientTest {
    @Test
    fun testCanBeCreatedWithBundle() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            appContext = context,
            dbEncryptionKey = key
        )
        val client = runBlocking {
            Client.create(account = fakeWallet, options = options)
        }

        val clientIdentity = fakeWallet.publicIdentity
        runBlocking {
            client.canMessage(listOf(clientIdentity))[clientIdentity.identifier]?.let { assert(it) }
        }

        val fromBundle = runBlocking {
            Client.build(clientIdentity, options = options)
        }
        assertEquals(client.inboxId, fromBundle.inboxId)

        runBlocking {
            fromBundle.canMessage(listOf(clientIdentity))[clientIdentity.identifier]?.let {
                assert(
                    it
                )
            }
        }
    }

    @Test
    fun testCanBeBuiltOffline() {
        val fixtures = fixtures()
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val wallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            appContext = context,
            dbEncryptionKey = key
        )
        val client = runBlocking {
            Client.create(account = wallet, options = options)
        }

        client.debugInformation.clearAllStatistics()
        println(client.debugInformation.aggregateStatistics)
        val builtClient = runBlocking {
            Client.build(client.publicIdentity, options = options, client.inboxId)
        }
        println(client.debugInformation.aggregateStatistics)
        assertEquals(client.inboxId, builtClient.inboxId)

        val convos = runBlocking {
            val group = builtClient.conversations.newGroup(listOf(fixtures.alixClient.inboxId))
            group.send("howdy")
            val alixDm = fixtures.alixClient.conversations.newConversation(builtClient.inboxId)
            alixDm.send("howdy")
            val boGroup =
                fixtures.boClient.conversations.newGroupWithIdentities(listOf(builtClient.publicIdentity))
            boGroup.send("howdy")
            builtClient.conversations.syncAllConversations()
            builtClient.conversations.list()
        }

        assertEquals(convos.size, 3)
    }

    @Test
    fun testCreatesAClient() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false, "Testing/0.0.0"),
            appContext = context,
            dbEncryptionKey = key
        )
        val clientIdentity = fakeWallet.publicIdentity

        val inboxId = runBlocking { Client.getOrCreateInboxId(options.api, clientIdentity) }
        val client = runBlocking {
            Client.create(
                account = fakeWallet,
                options = options
            )
        }
        runBlocking {
            client.canMessage(listOf(clientIdentity))[clientIdentity.identifier]?.let { assert(it) }
        }
        assert(client.installationId.isNotEmpty())
        assertEquals(inboxId, client.inboxId)
        assertEquals(fakeWallet.publicIdentity.identifier, client.publicIdentity.identifier)
    }

    @Test
    fun testStaticCanMessage() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fixtures = fixtures()
        val notOnNetwork = PrivateKeyBuilder()
        val alixPublicIdentity = PublicIdentity(IdentityKind.ETHEREUM, fixtures.alix.walletAddress)
        val boPublicIdentity = PublicIdentity(IdentityKind.ETHEREUM, fixtures.bo.walletAddress)
        val notOnNetworkPublicIdentity =
            PublicIdentity(IdentityKind.ETHEREUM, notOnNetwork.getPrivateKey().walletAddress)

        val canMessageList = runBlocking {
            Client.canMessage(
                listOf(
                    alixPublicIdentity,
                    notOnNetworkPublicIdentity,
                    boPublicIdentity
                ),
                ClientOptions.Api(XMTPEnvironment.LOCAL, false)
            )
        }

        val expectedResults = mapOf(
            alixPublicIdentity to true,
            notOnNetworkPublicIdentity to false,
            boPublicIdentity to true
        )

        expectedResults.forEach { (id, expected) ->
            assertEquals(expected, canMessageList[id.identifier])
        }
    }

    @Test
    fun testStaticInboxIds() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fixtures = fixtures()
        val states = runBlocking {
            Client.inboxStatesForInboxIds(
                listOf(fixtures.boClient.inboxId, fixtures.caroClient.inboxId),
                ClientOptions.Api(XMTPEnvironment.LOCAL, false)
            )
        }
        assertEquals(
            states.first().recoveryPublicIdentity.identifier,
            fixtures.boAccount.publicIdentity.identifier
        )
        assertEquals(
            states.last().recoveryPublicIdentity.identifier,
            fixtures.caroAccount.publicIdentity.identifier
        )
    }

    @Test
    fun testCanDeleteDatabase() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val fakeWallet2 = PrivateKeyBuilder()
        var client = runBlocking {
            Client.create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val client2 = runBlocking {
            Client.create(
                account = fakeWallet2,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        runBlocking {
            client.conversations.newGroup(listOf(client2.inboxId))
            client.conversations.sync()
            assertEquals(client.conversations.listGroups().size, 1)
        }

        assert(client.dbPath.isNotEmpty())
        client.deleteLocalDatabase()

        client = runBlocking {
            Client.create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        runBlocking {
            client.conversations.sync()
            assertEquals(client.conversations.listGroups().size, 0)
        }
    }

    @Test
    fun testCreatesADevClient() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking {
            Client.create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val clientIdentity = fakeWallet.publicIdentity
        runBlocking {
            client.canMessage(listOf(clientIdentity))[clientIdentity.identifier]?.let { assert(it) }
        }
    }

    @Test
    fun testCreatesAProductionClient() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val client = runBlocking {
            Client.create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.PRODUCTION, true),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val clientIdentity = fakeWallet.publicIdentity
        runBlocking {
            client.canMessage(listOf(clientIdentity))[clientIdentity.identifier]?.let { assert(it) }
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
            appContext = context,
            dbEncryptionKey = key
        )

        try {
            runBlocking { Client.create(account = fakeWallet, options = opts) }
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
            Client.create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val alixClient = runBlocking {
            Client.create(
                account = fakeWallet2,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
            boClient.conversations.sync()
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
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val boClient = runBlocking {
            Client.create(
                account = boWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val boInboxId = runBlocking {
            alixClient.inboxIdFromIdentity(
                PublicIdentity(
                    IdentityKind.ETHEREUM,
                    boWallet.getPrivateKey().walletAddress
                )
            )
        }
        assertEquals(boClient.inboxId, boInboxId)
    }

    @Test
    fun testRevokesInstallations() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        val alixClient2 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

        val alixClient3 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = File(context.filesDir.absolutePath, "xmtp_db3").toPath()
                        .toString()
                )
            )
        }

        var state = runBlocking { alixClient3.inboxState(true) }
        assertEquals(state.installations.size, 3)

        runBlocking {
            alixClient3.revokeInstallations(alixWallet, listOf(alixClient2.installationId))
        }

        state = runBlocking { alixClient3.inboxState(true) }
        assertEquals(state.installations.size, 2)
    }

    @Test
    fun testRevokesAllOtherInstallations() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        runBlocking {
            val alixClient = Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )

            val alixClient2 = Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

        val alixClient3 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = File(context.filesDir.absolutePath, "xmtp_db3").toPath()
                        .toString()
                )
            )
        }

        var state = runBlocking { alixClient3.inboxState(true) }
        assertEquals(state.installations.size, 3)
        assert(state.installations.first().createdAt != null)

        runBlocking {
            alixClient3.revokeAllOtherInstallations(alixWallet)
        }

        state = runBlocking { alixClient3.inboxState(true) }
        assertEquals(state.installations.size, 1)
    }

    @Test
    fun testsCanFindOthersInboxStates() {
        val fixtures = fixtures()
        val states = runBlocking {
            fixtures.alixClient.inboxStatesForInboxIds(
                true,
                listOf(fixtures.boClient.inboxId, fixtures.caroClient.inboxId)
            )
        }
        assertEquals(
            states.first().recoveryPublicIdentity.identifier,
            fixtures.bo.walletAddress
        )
        assertEquals(
            states.last().recoveryPublicIdentity.identifier,
            fixtures.caro.walletAddress
        )
    }

    @Test
    fun testsCanSeeKeyPackageStatus() {
        val fixtures = fixtures()
        runBlocking { Client.connectToApiBackend(ClientOptions.Api(XMTPEnvironment.LOCAL, true)) }
        val inboxState = runBlocking {
            Client.inboxStatesForInboxIds(
                listOf(fixtures.alixClient.inboxId),
                ClientOptions.Api(XMTPEnvironment.LOCAL, true)
            ).first()
        }
        val installationIds = inboxState.installations.map { it.installationId }
        val keyPackageStatus = runBlocking {
            Client.keyPackageStatusesForInstallationIds(
                installationIds,
                ClientOptions.Api(XMTPEnvironment.LOCAL, true)
            )
        }
        for (installationId: String in keyPackageStatus.keys) {
            val thisKPStatus = keyPackageStatus.get(installationId)!!
            val notBeforeDate = thisKPStatus.lifetime?.notBefore?.let {
                java.time.Instant.ofEpochSecond(it.toLong()).toString()
            } ?: "null"
            val notAfterDate = thisKPStatus.lifetime?.notAfter?.let {
                java.time.Instant.ofEpochSecond(it.toLong()).toString()
            } ?: "null"
            println("inst: " + installationId + " - valid from: " + notBeforeDate + " to: " + notAfterDate)
            println("error code: " + thisKPStatus.validationError)
            val notBefore = thisKPStatus.lifetime?.notBefore
            val notAfter = thisKPStatus.lifetime?.notAfter
            if (notBefore != null && notAfter != null) {
                assertEquals((3600 * 24 * 28 * 3 + 3600).toULong(), notAfter - notBefore)
            }
        }
    }

//    @Test
//    fun testsCanSeeInvalidKeyPackageStatusOnDev() {
//        runBlocking {
//            Client.connectToApiBackend(
//                ClientOptions.Api(
//                    XMTPEnvironment.DEV,
//                    true
//                )
//            )
//        }
//        val inboxState = runBlocking {
//            Client.inboxStatesForInboxIds(
//                listOf("f87420435131ea1b911ad66fbe4b626b107f81955da023d049f8aef6636b8e1b"),
//                ClientOptions.Api(XMTPEnvironment.DEV, true)
//            ).first()
//        }
//        val installationIds = inboxState.installations.map { it.installationId }
//        val keyPackageStatus = runBlocking {
//            Client.keyPackageStatusesForInstallationIds(
//                installationIds,
//                ClientOptions.Api(XMTPEnvironment.DEV, true)
//            )
//        }
//        for (installationId: String in keyPackageStatus.keys) {
//            val thisKPStatus = keyPackageStatus.get(installationId)!!
//            val notBeforeDate = thisKPStatus.lifetime?.notBefore?.let {
//                java.time.Instant.ofEpochSecond(it.toLong()).toString()
//            } ?: "null"
//            val notAfterDate = thisKPStatus.lifetime?.notAfter?.let {
//                java.time.Instant.ofEpochSecond(it.toLong()).toString()
//            } ?: "null"
//            println("inst: " + installationId + " - valid from: " + notBeforeDate + " to: " + notAfterDate)
//            println("error code: " + thisKPStatus.validationError)
//        }
//    }

    @Test
    fun testsSignatures() {
        val fixtures = fixtures()
        val signature = fixtures.alixClient.signWithInstallationKey("Testing")
        assertEquals(fixtures.alixClient.verifySignature("Testing", signature), true)
        assertEquals(fixtures.alixClient.verifySignature("Not Testing", signature), false)

        val alixInstallationId = fixtures.alixClient.installationId
        assertEquals(
            fixtures.alixClient.verifySignatureWithInstallationId(
                "Testing",
                signature,
                alixInstallationId
            ),
            true
        )
        assertEquals(
            fixtures.alixClient.verifySignatureWithInstallationId(
                "Not Testing",
                signature,
                alixInstallationId
            ),
            false
        )
        assertEquals(
            fixtures.alixClient.verifySignatureWithInstallationId(
                "Testing",
                signature,
                fixtures.boClient.installationId
            ),
            false
        )
        assertEquals(
            fixtures.boClient.verifySignatureWithInstallationId(
                "Testing",
                signature,
                alixInstallationId
            ),
            true
        )
        fixtures.alixClient.deleteLocalDatabase()

        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixClient2 = runBlocking {
            Client.create(
                account = fixtures.alixAccount,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        assertEquals(
            alixClient2.verifySignatureWithInstallationId(
                "Testing",
                signature,
                alixInstallationId
            ),
            true
        )
        assertEquals(
            alixClient2.verifySignatureWithInstallationId(
                "Testing2",
                signature,
                alixInstallationId
            ),
            false
        )
    }

    @OptIn(DelicateApi::class)
    @Test
    fun testAddAccounts() {
        val fixtures = fixtures()
        val alix2Wallet = PrivateKeyBuilder()
        val alix3Wallet = PrivateKeyBuilder()
        runBlocking { fixtures.alixClient.addAccount(alix2Wallet) }
        runBlocking { fixtures.alixClient.addAccount(alix3Wallet) }

        val state = runBlocking { fixtures.alixClient.inboxState(true) }
        assertEquals(state.installations.size, 1)
        assertEquals(state.identities.size, 3)
        assertEquals(
            state.recoveryPublicIdentity.identifier,
            fixtures.alixAccount.publicIdentity.identifier
        )
        assertEquals(
            state.identities.map { it.identifier }.sorted(),
            listOf(
                alix2Wallet.publicIdentity.identifier,
                alix3Wallet.publicIdentity.identifier,
                fixtures.alix.walletAddress
            ).sorted()
        )
    }

    @OptIn(DelicateApi::class)
    @Test
    fun testAddAccountsWithExistingInboxIds() {
        val fixtures = fixtures()

        assertThrows(
            "This wallet is already associated with inbox ${fixtures.boClient.inboxId}",
            XMTPException::class.java
        ) {
            runBlocking { fixtures.alixClient.addAccount(fixtures.boAccount) }
        }

        assert(fixtures.boClient.inboxId != fixtures.alixClient.inboxId)
        runBlocking { fixtures.alixClient.addAccount(fixtures.boAccount, true) }

        val state = runBlocking { fixtures.alixClient.inboxState(true) }
        assertEquals(state.identities.size, 2)

        val inboxId =
            runBlocking {
                fixtures.alixClient.inboxIdFromIdentity(
                    PublicIdentity(
                        IdentityKind.ETHEREUM,
                        fixtures.bo.walletAddress
                    )
                )
            }
        assertEquals(inboxId, fixtures.alixClient.inboxId)
    }

    @OptIn(DelicateApi::class)
    @Test
    fun testRemovingAccounts() {
        val fixtures = fixtures()
        val alix2Wallet = PrivateKeyBuilder()
        val alix3Wallet = PrivateKeyBuilder()
        runBlocking { fixtures.alixClient.addAccount(alix2Wallet) }
        runBlocking { fixtures.alixClient.addAccount(alix3Wallet) }

        var state = runBlocking { fixtures.alixClient.inboxState(true) }
        assertEquals(state.identities.size, 3)
        assertEquals(
            state.recoveryPublicIdentity.identifier,
            fixtures.alixAccount.publicIdentity.identifier
        )

        runBlocking {
            fixtures.alixClient.removeAccount(
                fixtures.alixAccount,
                PublicIdentity(IdentityKind.ETHEREUM, alix2Wallet.getPrivateKey().walletAddress)
            )
        }
        state = runBlocking { fixtures.alixClient.inboxState(true) }
        assertEquals(state.identities.size, 2)
        assertEquals(
            state.recoveryPublicIdentity.identifier,
            fixtures.alix.walletAddress
        )
        assertEquals(
            state.identities.map { it.identifier }.sorted(),
            listOf(
                alix3Wallet.getPrivateKey().walletAddress,
                fixtures.alixAccount.publicIdentity.identifier
            ).sorted()
        )
        assertEquals(state.installations.size, 1)

        // Cannot remove the recovery address
        assertThrows(
            "Client error: Unknown Signer",
            GenericException::class.java
        ) {
            runBlocking {
                fixtures.alixClient.removeAccount(
                    alix3Wallet,
                    fixtures.alixAccount.publicIdentity
                )
            }
        }
    }

    @Test
    fun testErrorsIfDbEncryptionKeyIsLost() {
        val key = SecureRandom().generateSeed(32)
        val badKey = SecureRandom().generateSeed(32)

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        assertThrows(
            "Error creating V3 client: Storage error: PRAGMA key or salt has incorrect value",
            XMTPException::class.java
        ) {
            runBlocking {
                Client.build(
                    publicIdentity = PublicIdentity(
                        IdentityKind.ETHEREUM,
                        alixWallet.getPrivateKey().walletAddress
                    ),
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = badKey,
                    )
                )
            }
        }

        assertThrows(
            "Error creating V3 client: Storage error: PRAGMA key or salt has incorrect value",
            XMTPException::class.java
        ) {
            runBlocking {
                Client.create(
                    account = alixWallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = badKey,
                    )
                )
            }
        }
    }

    @Test
    fun testCreatesAClientManually() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            appContext = context,
            dbEncryptionKey = key
        )
        val inboxId = runBlocking {
            Client.getOrCreateInboxId(
                options.api,
                fakeWallet.publicIdentity
            )
        }
        val client = runBlocking {
            Client.ffiCreateClient(fakeWallet.publicIdentity, options)
        }
        runBlocking {
            val sigRequest = client.ffiSignatureRequest()
            sigRequest?.let { signatureRequest ->
                signatureRequest.addEcdsaSignature(fakeWallet.sign(signatureRequest.signatureText()).rawData)
                client.ffiRegisterIdentity(signatureRequest)
            }
        }
        runBlocking {
            client.canMessage(listOf(fakeWallet.publicIdentity))[fakeWallet.publicIdentity.identifier]?.let {
                assert(
                    it
                )
            }
        }
        assert(client.installationId.isNotEmpty())
        assertEquals(inboxId, client.inboxId)
    }

    @Test
    fun testCanManageAddRemoveManually() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val boWallet = PrivateKeyBuilder()

        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            appContext = context,
            dbEncryptionKey = key
        )

        val alix = Client.create(alixWallet, options)

        var inboxState = alix.inboxState(true)
        assertEquals(1, inboxState.identities.size)

        val sigRequest = alix.ffiAddIdentity(boWallet.publicIdentity)
        val signedMessage = boWallet.sign(sigRequest.signatureText()).rawData

        sigRequest.addEcdsaSignature(signedMessage)
        alix.ffiApplySignatureRequest(sigRequest)

        inboxState = alix.inboxState(true)
        assertEquals(2, inboxState.identities.size)

        val sigRequest2 = alix.ffiRevokeIdentity(boWallet.publicIdentity)
        val signedMessage2 = alixWallet.sign(sigRequest2.signatureText()).rawData

        sigRequest2.addEcdsaSignature(signedMessage2)
        alix.ffiApplySignatureRequest(sigRequest2)

        inboxState = alix.inboxState(true)
        assertEquals(1, inboxState.identities.size)
    }

    @Test
    fun testCanManageRevokeManually() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val alix = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = context,
                dbEncryptionKey = key
            )
        )

        val alix2 = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = context,
                dbEncryptionKey = key,
                dbDirectory = context.filesDir.absolutePath.toString()
            )
        )
        val alix3 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = File(context.filesDir.absolutePath, "xmtp_db3").toPath()
                        .toString()
                )
            )
        }

        var inboxState = alix3.inboxState(true)
        assertEquals(inboxState.installations.size, 3)

        val sigText = alix.ffiRevokeInstallations(listOf(alix2.installationId.hexToByteArray()))
        val signedMessage = alixWallet.sign(sigText.signatureText()).rawData

        sigText.addEcdsaSignature(signedMessage)
        alix.ffiApplySignatureRequest(sigText)

        inboxState = alix.inboxState(true)
        assertEquals(2, inboxState.installations.size)

        val sigText2 = alix.ffiRevokeAllOtherInstallations()
        val signedMessage2 = alixWallet.sign(sigText2.signatureText()).rawData

        sigText2.addEcdsaSignature(signedMessage2)
        alix.ffiApplySignatureRequest(sigText2)

        inboxState = alix.inboxState(true)
        assertEquals(1, inboxState.installations.size)
    }

    @Test
    fun testPersistentLogging() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        Client.clearXMTPLogs(context)
        val fakeWallet = PrivateKeyBuilder()

        // Create a specific log directory for this test
        val logDirectory = File(context.filesDir, "xmtp_test_logs")
        if (logDirectory.exists()) {
            logDirectory.deleteRecursively()
        }
        logDirectory.mkdirs()

        try {
            // Activate persistent logging with a small number of log files
            Client.activatePersistentLibXMTPLogWriter(
                context,
                FfiLogLevel.TRACE,
                FfiLogRotation.HOURLY,
                3
            )

            // Log the actual log directory path
            val actualLogDir = File(context.filesDir, "xmtp_logs")
            println("Log directory path: ${actualLogDir.absolutePath}")

            // Create a client
            val client = runBlocking {
                Client.create(
                    account = fakeWallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = key
                    )
                )
            }

            // Create a group with only the client as a member
            runBlocking {
                client.conversations.newGroup(emptyList())
                client.conversations.sync()
            }

            // Verify the group was created
            val groups = runBlocking { client.conversations.listGroups() }
            assertEquals(1, groups.size)

            // Deactivate logging
            Client.deactivatePersistentLibXMTPLogWriter()

            // Print log files content to console
            val logFiles = File(context.filesDir, "xmtp_logs").listFiles()
            println("Found ${logFiles?.size ?: 0} log files:")

            logFiles?.forEach { file ->
                println("\n--- Log file: ${file.absolutePath} (${file.length()} bytes) ---")
                try {
                    val content = file.readText()
                    // Print first 1000 chars to avoid overwhelming the console
                    println(content.take(1000) + (if (content.length > 1000) "...(truncated)" else ""))
                } catch (e: Exception) {
                    println("Error reading log file: ${e.message}")
                }
            }
        } finally {
            // Make sure logging is deactivated
            Client.deactivatePersistentLibXMTPLogWriter()
        }
        val logFiles = Client.getXMTPLogFilePaths(context)
        assertEquals(logFiles.size, 1)
        println(logFiles.get(0))
        Client.clearXMTPLogs(context)
        val logFiles2 = Client.getXMTPLogFilePaths(context)
        assertEquals(logFiles2.size, 0)
    }

    @Test
    fun testNetworkDebugInformation() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val alix = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = context,
                dbEncryptionKey = key
            )
        )
        alix.debugInformation.clearAllStatistics()

        val job = CoroutineScope(Dispatchers.IO).launch {
            alix.conversations.streamAllMessages().collect { }
        }
        val group = alix.conversations.newGroup(emptyList())
        group.send("hi")

        delay(4000)

        val aggregateStats2 = alix.debugInformation.aggregateStatistics
        println("Aggregate Stats Create:\n$aggregateStats2")

        val apiStats2 = alix.debugInformation.apiStatistics
        assertEquals(0, apiStats2.fetchKeyPackage)
        assertEquals(6, apiStats2.sendGroupMessages)
        assertEquals(0, apiStats2.sendWelcomeMessages)
        assertEquals(1, apiStats2.queryWelcomeMessages)
        assertEquals(1, apiStats2.subscribeWelcomes)

        val identityStats2 = alix.debugInformation.identityStatistics
        assertEquals(0, identityStats2.publishIdentityUpdate)
        assertEquals(0, identityStats2.getIdentityUpdatesV2)
        assertEquals(0, identityStats2.getInboxIds)
        assertEquals(0, identityStats2.verifySmartContractWalletSignature)
        job.cancel()
    }

    @Test
    fun testUploadArchiveDebugInformation() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val alix = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = context,
                dbEncryptionKey = key
            )
        )
        val uploadKey = alix.debugInformation.uploadDebugInformation()
        assert(uploadKey.isNotEmpty())
    }

    @Test
    fun testCannotCreateMoreThan10Installations() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val encryptionKey = SecureRandom().generateSeed(32)
        val wallet = PrivateKeyBuilder()

        val clients = mutableListOf<Client>()

        repeat(10) { i ->
            val client = runBlocking {
                Client.create(
                    account = wallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = encryptionKey,
                        dbDirectory = File(context.filesDir, "xmtp_db_$i").absolutePath
                    )
                )
            }
            clients.add(client)
        }

        val state = runBlocking { clients.first().inboxState(true) }
        assertEquals(10, state.installations.size)

        // Attempt to create a 6th installation, should fail
        assertThrows(
            "Error creating V3 client: Client builder error: Cannot register a new installation because the InboxID ${clients[0].inboxId} has already registered 10/10 installations. Please revoke existing installations first.",
            XMTPException::class.java
        ) {
            runBlocking {
                Client.create(
                    account = wallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = encryptionKey,
                        dbDirectory = File(context.filesDir, "xmtp_db_10").absolutePath
                    )
                )
            }
        }

        val boWallet = PrivateKeyBuilder()
        val boClient = runBlocking {
            Client.create(
                account = boWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = SecureRandom().generateSeed(32),
                    dbDirectory = File(context.filesDir, "xmtp_bo").absolutePath
                )
            )
        }

        val group = runBlocking {
            boClient.conversations.newGroup(listOf(clients[2].inboxId))
        }

        val members = runBlocking { group.members() }
        val alixMember = members.find { it.inboxId == clients.first().inboxId }
        assertNotNull(alixMember)
        val inboxState =
            runBlocking { boClient.inboxStatesForInboxIds(true, listOf(alixMember!!.inboxId)) }
        assertEquals(10, inboxState.first().installations.size)

        runBlocking {
            clients.first().revokeInstallations(wallet, listOf(clients[9].installationId))
        }

        val stateAfterRevoke = runBlocking { clients.first().inboxState(true) }
        assertEquals(9, stateAfterRevoke.installations.size)

        runBlocking {
            Client.create(
                account = wallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = encryptionKey,
                    dbDirectory = File(context.filesDir, "xmtp_db_11").absolutePath
                )
            )
        }
        val updatedState = runBlocking { clients.first().inboxState(true) }
        assertEquals(10, updatedState.installations.size)
    }

    @Test
    fun testStaticRevokeOneOfFiveInstallations() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val wallet = PrivateKeyBuilder()
        val encryptionKey = SecureRandom().generateSeed(32)

        val clients = mutableListOf<Client>()
        repeat(5) { i ->
            val client = runBlocking {
                Client.create(
                    account = wallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = context,
                        dbEncryptionKey = encryptionKey,
                        dbDirectory = File(context.filesDir, "xmtp_db_$i").absolutePath
                    )
                )
            }
            clients.add(client)
        }

        var state = runBlocking { clients.last().inboxState(true) }
        assertEquals(5, state.installations.size)

        val toRevokeId = clients[1].installationId
        runBlocking {
            Client.revokeInstallations(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                wallet,
                clients.first().inboxId,
                listOf(toRevokeId)
            )
        }

        state = runBlocking { clients.last().inboxState(true) }
        assertEquals(4, state.installations.size)
        val remainingIds = state.installations.map { it.installationId }
        assertFalse(remainingIds.contains(toRevokeId))
    }

    @Test
    fun testStaticRevokeInstallationsManually() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()
        val apiOptions = ClientOptions.Api(XMTPEnvironment.LOCAL, false)
        val alix = Client.create(
            account = alixWallet,
            options = ClientOptions(
                apiOptions,
                appContext = context,
                dbEncryptionKey = key
            )
        )

        val alix2 = Client.create(
            account = alixWallet,
            options = ClientOptions(
                apiOptions,
                appContext = context,
                dbEncryptionKey = key,
                dbDirectory = context.filesDir.absolutePath.toString()
            )
        )
        val alix3 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    apiOptions,
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = File(context.filesDir.absolutePath, "xmtp_db3").toPath()
                        .toString()
                )
            )
        }

        var inboxState = alix3.inboxState(true)
        assertEquals(inboxState.installations.size, 3)

        val sigText = ffiRevokeInstallations(
            apiOptions,
            alixWallet.publicIdentity,
            alix.inboxId,
            listOf(alix2.installationId)
        )
        val signedMessage = alixWallet.sign(sigText.signatureText()).rawData

        sigText.addEcdsaSignature(signedMessage)
        ffiApplySignatureRequest(apiOptions, sigText)

        inboxState = alix.inboxState(true)
        assertEquals(2, inboxState.installations.size)
    }
}
