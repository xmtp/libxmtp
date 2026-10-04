package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.SecureRandom
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class ClientTest : BaseInstrumentedTest() {
    private suspend fun client(
        signer: Signer,
        options: ClientOptions = createClientOptions(),
    ): SDKClient = trackClient(SDKClient.create(context, signer, options))

    private fun backend() = BackendSource.Options(localApi())

    private fun availabilityKey(identity: PublicIdentity): String =
        when (identity.kind) {
            PublicIdentityKind.ETHEREUM -> "ethereum:${identity.identifier.lowercase()}"
            PublicIdentityKind.PASSKEY -> "passkey:${identity.identifier.lowercase()}"
        }

    @Test fun testCanBeCreatedWithBundle() =
        runBlocking {
            val signer = createWallet()
            val options = createClientOptions().let { it.copy(storage = it.storage.copy(label = "custom-db")) }
            val first = client(signer, options)
            val path = checkNotNull(first.storage().path())
            assertEquals("custom-db", first.options().storage.label)
            assertEquals(true, first.canMessage(listOf(signer.identity()))[availabilityKey(signer.identity())])
            val inbox = first.inboxId()
            first.end()
            val reopened = trackClient(SDKClient.build(context, signer.identity(), options))
            assertEquals(inbox, reopened.inboxId())
            assertEquals(path, reopened.storage().path())
            assertEquals(true, reopened.canMessage(listOf(signer.identity()))[availabilityKey(signer.identity())])
        }

    @Test fun testCanBeBuiltOffline() =
        runBlocking {
            val fixtures = createFixtures()
            val signer = createWallet()
            val options = createClientOptions()
            val original = client(signer, options)
            val group = original.conversations().createGroup(listOf(fixtures.alixClient.inboxId()))
            group.sendText("howdy")
            val inbox = original.inboxId()
            val groupId = group.id()
            original.end()
            val built =
                trackClient(SDKClient.build(context, signer.identity(), options.copy(allowOffline = true), inbox))
            assertEquals(inbox, built.inboxId())
            assertEquals(
                groupId,
                (checkNotNull(built.conversations().getById(groupId)) as Conversation.Group).group.id(),
            )
            val dm = fixtures.alixClient.conversations().createDm(built.inboxId())
            dm.sendText("direct")
            fixtures.boClient
                .conversations()
                .createGroup(listOf(built.inboxId()))
                .sendText("group")
            built.conversations().syncAll(null)
            assertEquals(3, built.conversations().list().size)
        }

    // verifies: META-065
    @Test fun testCreatesAClient() =
        runBlocking {
            for (inMemory in listOf(false, true)) {
                val signer = createWallet()
                val initial = createClientOptions(localApi("Testing/0.0.0"))
                val options =
                    if (inMemory) {
                        initial.copy(
                            storage = StorageOptions(StorageLocation.InMemory),
                        )
                    } else {
                        initial
                    }
                val created = client(signer, options)
                assertEquals(true, created.canMessage(listOf(signer.identity()))[availabilityKey(signer.identity())])
                assertTrue(created.installationId().isNotEmpty())
                assertEquals(signer.identity(), created.identity())
                assertEquals(inMemory, created.isInMemory())
                assertEquals("Testing/0.0.0", created.appVersion())
                assertNull(created.options().storage.encryptionKey)
                val group = created.conversations().createGroup(emptyList<InboxId>())
                val changed = CompletableDeferred<ClientEvent.ConversationMetadataChanged>()
                val listener =
                    created.startListener(
                        EventFilter(
                            listOf(EventKind.CONVERSATION_METADATA_CHANGED),
                            listOf(group.id().hexToByteArray()),
                            null,
                            false,
                        ),
                        { event ->
                            if (event is ClientEvent.ConversationMetadataChanged &&
                                event.metadataChanged.groupId.contentEquals(group.id().hexToByteArray())
                            ) {
                                changed.complete(event)
                            }
                        },
                    )
                try {
                    group.updateAppData("client-runtime-options", null)
                    assertArrayEquals(
                        group.id().hexToByteArray(),
                        withTimeout(5_000) { changed.await() }.metadataChanged.groupId,
                    )
                    assertEquals("client-runtime-options", group.state().appData)
                } finally {
                    withContext(NonCancellable) { created.stopListener(listener) }
                }
            }
        }

    @Test fun testStaticCanMessage() =
        runBlocking {
            val fixtures = createFixtures()
            val absent = createWallet().identity()
            val values = SDKClient.canMessage(listOf(fixtures.alix, absent, fixtures.bo), backend())
            assertEquals(true, values[availabilityKey(fixtures.alix)])
            assertEquals(true, values[availabilityKey(fixtures.bo)])
            assertEquals(false, values[availabilityKey(absent)])
        }

    @Test fun testStaticInboxIds() =
        runBlocking {
            val fixtures = createFixtures()
            val states =
                SDKClient.inboxStates(
                    listOf(fixtures.boClient.inboxId(), fixtures.caroClient.inboxId()),
                    backend(),
                )
            assertEquals(fixtures.bo, states.first().recoveryIdentity)
            assertEquals(fixtures.caro, states.last().recoveryIdentity)
        }

    @Test fun testCanDeleteDatabase() =
        runBlocking {
            val signer = createWallet()
            val options = createClientOptions()
            val original = client(signer, options)
            original.conversations().createGroup(emptyList<InboxId>())
            assertEquals(1, original.conversations().listGroups(null).size)
            val path = checkNotNull(original.storage().path())
            original.storage().delete()
            assertFalse(File(path).exists())
            val replacement = client(signer, options)
            assertTrue(replacement.conversations().listGroups(null).isEmpty())
        }

    // verifies: IDENT-076
    @Test fun testPreAuthenticateToInboxCallback() =
        runBlocking {
            val called = CompletableDeferred<Unit>()
            val options =
                createClientOptions().copy(
                    handlers =
                        ClientHandlers(
                            object : PreAuthenticate {
                                override suspend fun run() {
                                    called.complete(Unit)
                                }
                            },
                        ),
                )
            client(createWallet(), options)
            withTimeout(5_000) { called.await() }
        }

    @Test fun testCanDropReconnectDatabase() =
        runBlocking {
            val signer = createWallet()
            val options = createClientOptions()
            val original = client(signer, options)
            val group = original.conversations().createGroup(emptyList<InboxId>())
            group.sendText("stored")
            original.end()
            assertTrue(
                runCatching {
                    original.conversations().listGroups(
                        null,
                    )
                }.exceptionOrNull() is XmtpException.ClientClosed,
            )
            val reopened = trackClient(SDKClient.build(context, signer.identity(), options))
            reopened.storage().reconnect()
            assertEquals(
                group.id(),
                reopened
                    .conversations()
                    .listGroups(null)
                    .single()
                    .id(),
            )
        }

    @Test fun testCanGetAnInboxIdFromAddress() =
        runBlocking {
            val fixtures = createFixtures()
            assertEquals(fixtures.boClient.inboxId(), fixtures.alixClient.inboxIdFor(fixtures.bo))
        }

    @Test fun testRevokesInstallations() =
        runBlocking {
            val signer = createWallet()
            val clients = List(3) { client(signer) }
            assertEquals(
                3,
                clients
                    .last()
                    .inboxState(true)
                    .installations.size,
            )
            clients.last().revokeInstallations(signer, listOf(clients[1].installationId()))
            val state = clients.last().inboxState(true)
            assertEquals(2, state.installations.size)
            assertFalse(state.installations.any { it.id == clients[1].installationId() })
        }

    @Test fun testRevokesAllOtherInstallations() =
        runBlocking {
            val signer = createWallet()
            val clients = List(3) { client(signer) }
            clients.last().revokeAllOtherInstallations(signer)
            assertEquals(
                listOf(clients.last().installationId()),
                clients
                    .last()
                    .inboxState(true)
                    .installations
                    .map { it.id },
            )
        }

    @Test fun testsCanFindOthersInboxStates() =
        runBlocking {
            val fixtures = createFixtures()
            val states =
                fixtures.alixClient.inboxStates(
                    listOf(fixtures.boClient.inboxId(), fixtures.caroClient.inboxId()),
                    true,
                )
            assertEquals(fixtures.bo, states.first().recoveryIdentity)
            assertEquals(fixtures.caro, states.last().recoveryIdentity)
        }

    @Test fun testsCanSeeKeyPackageStatus() =
        runBlocking {
            val fixtures = createFixtures()
            val ids =
                fixtures.alixClient
                    .inboxState(true)
                    .installations
                    .map { it.id }
            val statuses = SDKClient.keyPackageStatuses(ids, backend())
            assertEquals(ids.toSet(), statuses.keys.toSet())
            for (status in statuses.values) {
                assertNull(status.validationError)
                val lifetime = checkNotNull(status.lifetime)
                assertEquals((3600 * 24 * 28 * 3 + 3600).toULong(), lifetime.notAfter - lifetime.notBefore)
            }
        }

    @Test fun testsSignatures() =
        runBlocking {
            val fixtures = createFixtures()
            val signature = fixtures.alixClient.signWithInstallationKey("Testing")
            assertTrue(fixtures.alixClient.verifySignedWithInstallationKey("Testing", signature))
            assertFalse(fixtures.alixClient.verifySignedWithInstallationKey("Not Testing", signature))
            val publicKey = fixtures.alixClient.installationIdBytes()
            assertTrue(SDKClient.verifySignedWithPublicKey("Testing", signature, publicKey))
            assertFalse(SDKClient.verifySignedWithPublicKey("Not Testing", signature, publicKey))
            assertFalse(
                SDKClient.verifySignedWithPublicKey("Testing", signature, fixtures.boClient.installationIdBytes()),
            )
            fixtures.alixClient.storage().delete()
            val replacement = client(fixtures.alixAccount)
            assertTrue(SDKClient.verifySignedWithPublicKey("Testing", signature, publicKey))
            assertNotEquals(publicKey.toHex(), replacement.installationIdBytes().toHex())
        }

    @Test fun testAddAccounts() =
        runBlocking {
            val fixtures = createFixtures()
            val second = createWallet()
            val third = createWallet()
            fixtures.alixClient.unsafeAddAccount(second, false)
            fixtures.alixClient.unsafeAddAccount(third, false)
            val state = fixtures.alixClient.inboxState(true)
            assertEquals(1, state.installations.size)
            assertEquals(setOf(fixtures.alix, second.identity(), third.identity()), state.identities.toSet())
            assertEquals(fixtures.alix, state.recoveryIdentity)
        }

    @Test fun testAddAccountsWithExistingInboxIds() =
        runBlocking {
            val fixtures = createFixtures()
            assertTrue(
                runCatching {
                    fixtures.alixClient.unsafeAddAccount(fixtures.boAccount, false)
                }.exceptionOrNull() is XmtpException,
            )
            assertNotEquals(fixtures.alixClient.inboxId(), fixtures.boClient.inboxId())
            fixtures.alixClient.unsafeAddAccount(fixtures.boAccount, true)
            assertEquals(
                2,
                fixtures.alixClient
                    .inboxState(true)
                    .identities.size,
            )
            assertEquals(fixtures.alixClient.inboxId(), fixtures.alixClient.inboxIdFor(fixtures.bo))
        }

    @Test fun testRemovingAccounts() =
        runBlocking {
            val fixtures = createFixtures()
            val second = createWallet()
            val third = createWallet()
            fixtures.alixClient.unsafeAddAccount(second, false)
            fixtures.alixClient.unsafeAddAccount(third, false)
            fixtures.alixClient.removeAccount(fixtures.alixAccount, second.identity())
            val state = fixtures.alixClient.inboxState(true)
            assertEquals(setOf(fixtures.alix, third.identity()), state.identities.toSet())
            assertEquals(fixtures.alix, state.recoveryIdentity)
            assertEquals(1, state.installations.size)
            assertTrue(
                runCatching {
                    fixtures.alixClient.removeAccount(
                        third,
                        fixtures.alix,
                    )
                }.exceptionOrNull() is XmtpException,
            )
        }

    @Test fun testErrorsIfDbEncryptionKeyIsLost() =
        runBlocking {
            val signer = createWallet()
            val options = createClientOptions()
            client(signer, options).end()
            val bad = options.copy(storage = options.storage.copy(encryptionKey = SecureRandom().generateSeed(32)))
            assertTrue(
                runCatching { SDKClient.build(context, signer.identity(), bad) }.exceptionOrNull() is XmtpException,
            )
            assertTrue(runCatching { SDKClient.create(context, signer, bad) }.exceptionOrNull() is XmtpException)
            assertEquals(
                signer.identity(),
                trackClient(SDKClient.build(context, signer.identity(), options)).identity(),
            )
        }

    @Test fun testCreatesAClientManually() =
        runBlocking {
            val signer = createWallet()
            val created = client(signer, createClientOptions().copy(registration = RegistrationOptions(auto = false)))
            assertFalse(created.isRegistered())
            val request = checkNotNull(created.unsafeCreateInboxSignatureRequest())
            request.sign(signer)
            created.unsafeApplySignatureRequest(request)
            assertTrue(created.isRegistered())
            assertEquals(true, created.canMessage(listOf(signer.identity()))[availabilityKey(signer.identity())])
            assertTrue(created.installationId().isNotEmpty())
        }

    @Test fun testCanManageAddRemoveManually() =
        runBlocking {
            val signer = createWallet()
            val other = createWallet()
            val created = client(signer)
            val add = created.unsafeAddAccountSignatureRequest(other.identity(), false)
            add.sign(other)
            created.unsafeApplySignatureRequest(add)
            assertEquals(2, created.inboxState(true).identities.size)
            val remove = created.unsafeRemoveAccountSignatureRequest(other.identity())
            remove.sign(signer)
            created.unsafeApplySignatureRequest(remove)
            assertEquals(listOf(signer.identity()), created.inboxState(true).identities)
        }

    @Test fun testCanManageRevokeManually() =
        runBlocking {
            val signer = createWallet()
            val clients = List(3) { client(signer) }
            val request = clients.last().unsafeRevokeInstallationsSignatureRequest(listOf(clients[1].installationId()))
            request.sign(signer)
            clients.last().unsafeApplySignatureRequest(request)
            assertEquals(
                2,
                clients
                    .last()
                    .inboxState(true)
                    .installations.size,
            )
            val all = checkNotNull(clients.last().unsafeRevokeAllOtherInstallationsSignatureRequest())
            all.sign(signer)
            clients.last().unsafeApplySignatureRequest(all)
            assertEquals(
                1,
                clients
                    .last()
                    .inboxState(true)
                    .installations.size,
            )
        }

    @Test fun testPersistentLogging() =
        runBlocking {
            initLogging(LoggingOptions(level = LogLevel.TRACE))
            SDKClient.clearXMTPLogs(context)
            SDKClient.activatePersistentLibXMTPLogWriter(context, LogLevel.TRACE, LogRotation.HOURLY, 3u)
            try {
                val created = client(createWallet())
                created.conversations().createGroup(emptyList<InboxId>())
                created.conversations().sync()
            } finally {
                SDKClient.deactivatePersistentLibXMTPLogWriter()
            }
            val files = SDKClient.getXMTPLogFilePaths(context)
            assertEquals(1, files.size)
            assertTrue(File(files.single()).length() > 0)
            SDKClient.clearXMTPLogs(context)
            assertTrue(SDKClient.getXMTPLogFilePaths(context).isEmpty())
        }

    @Test fun testNetworkDebugInformation() =
        runBlocking {
            val workers = WorkerOptions(intervals = WorkerKind.entries.map { WorkerInterval(it, null, null, false) })
            val created =
                client(
                    createWallet(),
                    createClientOptions(
                        localApi("stats/${UUID.randomUUID()}"),
                        deviceSyncEnabled = false,
                    ).copy(workers = workers),
                )
            created.diagnostics().clearStatistics()
            val reset = created.diagnostics().apiStatistics()
            assertEquals(ApiStats(0uL, 0uL, 0uL, 0uL, 0uL), reset)
            assertEquals(IdentityStats(0uL, 0uL), created.diagnostics().identityStatistics())
            created.conversations().sync()
            val synced = created.diagnostics().apiStatistics()
            assertEquals(0uL, synced.publish)
            assertTrue(synced.queryNewest > 0uL)
            assertTrue(synced.subscribe > 0uL)
            assertEquals(0uL, synced.subscribeStatic)
            val stream = launch { created.messages().collect {} }
            try {
                created.inboxState(true)
                val before = created.diagnostics().apiStatistics()
                assertTrue(before.query > synced.query)
                val group = created.conversations().createGroup(emptyList<InboxId>())
                val after = created.diagnostics().apiStatistics()
                assertTrue(after.publish > before.publish)
                created.conversations().sync()
                val queried = created.diagnostics().apiStatistics()
                assertTrue(queried.queryNewest > after.queryNewest)
                group.sendText("hi")
                val sent = created.diagnostics().apiStatistics()
                assertTrue(sent.publish > after.publish)
                assertTrue(sent.subscribe > 0uL)
                assertEquals(0uL, sent.subscribeStatic)
                assertTrue(created.diagnostics().aggregateStatistics().isNotEmpty())
            } finally {
                withContext(NonCancellable) { stream.cancelAndJoin() }
            }
        }

    @Test fun testCannotCreateMoreThan10Installations() =
        runBlocking {
            val signer = createWallet()
            val clients = List(10) { client(signer) }
            assertEquals(
                10,
                clients
                    .first()
                    .inboxState(true)
                    .installations.size,
            )
            assertTrue(runCatching { client(signer) }.exceptionOrNull() is XmtpException)
            val other = client(createWallet())
            val group = other.conversations().createGroup(listOf(clients.first().inboxId()))
            assertTrue(group.members().any { it.inboxId == clients.first().inboxId() })
            assertEquals(
                10,
                other
                    .inboxStates(listOf(clients.first().inboxId()), true)
                    .single()
                    .installations.size,
            )
            clients.first().revokeInstallations(signer, listOf(clients.last().installationId()))
            assertEquals(
                9,
                clients
                    .first()
                    .inboxState(true)
                    .installations.size,
            )
            client(signer)
            assertEquals(
                10,
                clients
                    .first()
                    .inboxState(true)
                    .installations.size,
            )
        }

    @Test fun testStaticRevokeOneOfFiveInstallations() =
        runBlocking {
            val signer = createWallet()
            val clients = List(5) { client(signer) }
            val removed = clients[1].installationId()
            SDKClient.revokeInstallations(signer, clients.first().inboxId(), listOf(removed), backend())
            val state = clients.last().inboxState(true)
            assertEquals(4, state.installations.size)
            assertFalse(state.installations.any { it.id == removed })
        }

    @Test fun testStaticRevokeInstallationsManually() =
        runBlocking {
            val signer = createWallet()
            val clients = List(3) { client(signer) }
            val request = clients.first().unsafeRevokeInstallationsSignatureRequest(listOf(clients[1].installationId()))
            request.sign(signer)
            clients.first().unsafeApplySignatureRequest(request)
            assertEquals(
                2,
                clients
                    .last()
                    .inboxState(true)
                    .installations.size,
            )
        }
}
