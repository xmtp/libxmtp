package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*
import java.io.File

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

    // Each hand-written backend wrapper in SDKClient's companion object runs here, except
    // fetchServerConfiguration and inboxIdFor (BackendAuthTest) and
    // verifySignedWithPublicKey (testsSignatures).
    @Test fun testStaticBackendCalls() =
        runBlocking {
            val fixtures = createFixtures()
            val absent = createWallet().identity()
            val values = SDKClient.canMessage(listOf(fixtures.alix, absent, fixtures.bo), backend())
            assertEquals(true, values[availabilityKey(fixtures.alix)])
            assertEquals(true, values[availabilityKey(fixtures.bo)])
            assertEquals(false, values[availabilityKey(absent)])

            val states = SDKClient.inboxStates(listOf(fixtures.boClient.inboxId()), backend())
            assertEquals(fixtures.bo, states.single().recoveryIdentity)

            val installation = fixtures.alixClient.installationId()
            val statuses = SDKClient.keyPackageStatuses(listOf(installation), backend())
            assertNull(checkNotNull(statuses[installation]).validationError)

            // These wrappers move the backend argument, and the inbox and
            // address are both strings, so only a live read catches a swap.
            val alixInbox = fixtures.alixClient.inboxId()
            assertTrue(SDKClient.isAddressAuthorized(fixtures.alix.identifier, alixInbox, backend()))
            assertFalse(SDKClient.isAddressAuthorized(absent.identifier, alixInbox, backend()))
            assertTrue(SDKClient.isInstallationAuthorized(installation, alixInbox, backend()))
            assertFalse(SDKClient.isInstallationAuthorized("00".repeat(32), alixInbox, backend()))

            val group = fixtures.alixClient.conversations().createGroup(emptyList<InboxId>())
            group.sendText("newest")
            val metadata = SDKClient.newestMessageMetadata(listOf(group.id()), backend())
            assertTrue(checkNotNull(metadata[group.id()]).sequenceId > 0u)

            val signer = createWallet()
            val kept = client(signer)
            val removed = client(signer).installationId()
            SDKClient.revokeInstallations(signer, kept.inboxId(), listOf(removed), backend())
            assertEquals(listOf(kept.installationId()), kept.inboxState(true).installations.map { it.id })
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
}
