package uniffi.xmtp_sdk

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.BuildConfig
import java.io.File
import java.net.URI
import java.util.Date
import java.util.UUID

/** These tests load the AAR's JNI library on the Android target. */
@RunWith(AndroidJUnit4::class)
class AndroidPackageTest {
    private var previousLifecycle = true

    @Before fun setup() =
        runBlocking {
            previousLifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
        }

    @After fun teardown() {
        AndroidStreamLifecycle.enabled = previousLifecycle
    }

    @Test fun processLifecycleControlIsOnByDefault() {
        assertTrue(previousLifecycle)
    }

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun options(label: String) =
        ClientOptions(
            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
            storage = StorageOptions(location = StorageLocation.Default, label = label),
            deviceSync = false,
            attachments = AttachmentOptions(allowPrivateNetwork = true),
        )

    private suspend fun <T> clients(block: suspend (SDKClient, SDKClient) -> T): T {
        val suffix = UUID.randomUUID().toString()
        val first = SDKClient.create(context, generateLocalSigner(), options("first-$suffix"))
        try {
            val second = SDKClient.create(context, generateLocalSigner(), options("second-$suffix"))
            try {
                return block(first, second)
            } finally {
                withContext(NonCancellable) { second.storage().delete() }
            }
        } finally {
            withContext(NonCancellable) { first.storage().delete() }
        }
    }

    @Test fun generatedTimestampsLoadAndConvertWithDesugaring() {
        assertEquals(BuildConfig.XMTP_SDK_VERSION, sdkVersion())
        val positive = Timestamp(1_000_000_001L).date
        assertEquals(1L, positive.epochSecond)
        assertEquals(1, positive.nano)
        assertEquals(1_000L, Date.from(positive).time)
        val negative = Timestamp(-1L).date
        assertEquals(-1L, negative.epochSecond)
        assertEquals(999_999_999, negative.nano)
        assertEquals(-1L, Date.from(negative).time)
    }

    @Test fun packageLoadsAndContextStorageKeepsMetadataOffline() =
        runBlocking {
            assertEquals(BuildConfig.XMTP_SDK_VERSION, sdkVersion())
            assertTrue(
                context.assets
                    .open("sdk-contract.json")
                    .bufferedReader()
                    .use { it.readText() }
                    .contains("contract"),
            )
            OfflineBackendProxy(BuildConfig.XMTP_BACKEND_URL).use { proxy ->
                val signer = generateLocalSigner()
                val configuration =
                    options("reopen-${UUID.randomUUID()}").copy(
                        backend = BackendSource.Options(BackendOptions(url = proxy.url)),
                    )
                val client = SDKClient.create(context, signer, configuration)
                val (inbox, path, id) =
                    try {
                        val inbox = client.inboxId()
                        val path = checkNotNull(client.storage().path())
                        val group =
                            client.conversations.createGroup(
                                emptyList<InboxId>(),
                                CreateGroupOptions(name = "Android metadata"),
                            )
                        assertTrue(
                            File(
                                path,
                            ).canonicalPath.startsWith(
                                File(context.filesDir, "xmtp_db").canonicalPath + File.separator,
                            ),
                        )
                        group.sendText("Stored on Android")
                        println(
                            "Offline create: backend=${proxy.url}; label=${configuration.storage.label}; " +
                                "path=$path; inbox=$inbox",
                        )
                        Triple(inbox, path, group.id())
                    } finally {
                        withContext(NonCancellable) { client.end() }
                    }
                proxy.close()
                proxy.assertUnavailable()
                val offline =
                    SDKClient.build(
                        context,
                        signer.identity(),
                        configuration.copy(
                            allowOffline = true,
                        ),
                        inboxId = inbox,
                    )
                try {
                    assertEquals(inbox, offline.inboxId())
                    assertEquals(path, offline.storage().path())
                    println(
                        "Offline reopen: backend=${proxy.url}; label=${configuration.storage.label}; " +
                            "path=${offline.storage().path()}; inbox=${offline.inboxId()}",
                    )
                    val restored = checkNotNull(offline.conversations.getById(id)) as Conversation.Group
                    assertEquals("Android metadata", restored.group.state().name)
                    assertTrue(
                        restored.group.messages().any {
                            (it.content as? SDKMessageContent.Standard)?.value ==
                                MessageContent.Text("Stored on Android")
                        },
                    )
                } finally {
                    withContext(NonCancellable) { offline.storage().delete() }
                }
            }
        }

    @Test fun contextFactoryPreservesExplicitAndInMemoryStorage() =
        runBlocking {
            val directory = File(context.filesDir, "explicit-${UUID.randomUUID()}")
            val database = File(directory, "selected.db3").absolutePath
            val selected =
                listOf(
                    StorageLocation.Explicit(database, File(directory, "attachments").absolutePath),
                    StorageLocation.InMemory,
                )
            for (location in selected) {
                val configuration =
                    options("selected-${UUID.randomUUID()}").copy(
                        storage = StorageOptions(location = location),
                        registration = RegistrationOptions(auto = false),
                    )
                val client = SDKClient.create(context, generateLocalSigner(), configuration)
                try {
                    assertEquals(if (location is StorageLocation.Explicit) database else null, client.storage().path())
                    assertFalse(client.isRegistered())
                } finally {
                    withContext(NonCancellable) {
                        if (location is StorageLocation.InMemory) client.end() else client.storage().delete()
                    }
                }
            }
        }

    @Test fun contextStorageLabelsKeepSeparateHistories() =
        runBlocking {
            val signer = generateLocalSigner()
            val suffix = UUID.randomUUID()
            val firstOptions = options("profile-first-$suffix")
            val secondOptions = options("profile-second-$suffix")
            val first = SDKClient.create(context, signer, firstOptions)
            val (groupId, firstPath) =
                try {
                    val group = first.conversations.createGroup(emptyList<InboxId>())
                    group.sendText("first profile")
                    group.id() to first.storage().path()
                } finally {
                    withContext(NonCancellable) { first.end() }
                }
            val second = SDKClient.create(context, signer, secondOptions)
            try {
                assertNotEquals(firstPath, second.storage().path())
                assertNull(second.conversations.getById(groupId))
            } finally {
                withContext(NonCancellable) { second.storage().delete() }
            }
            val reopened = SDKClient.build(context, signer.identity(), firstOptions)
            try {
                assertEquals(firstPath, reopened.storage().path())
                val group = (checkNotNull(reopened.conversations.getById(groupId)) as Conversation.Group).group
                assertTrue(
                    group.messages().any {
                        (it.content as? SDKMessageContent.Standard)?.value == MessageContent.Text("first profile")
                    },
                )
            } finally {
                withContext(NonCancellable) { reopened.storage().delete() }
            }
        }

    @Test fun attachmentTransfersThroughReversedLoopbackPort() =
        runBlocking {
            withTimeout(120_000) {
                clients { sender, receiver ->
                    val offered = checkNotNull(sender.serverConfiguration().attachments)
                    assertEquals("Attachment fixture must use adb reverse", "127.0.0.1", URI(offered.baseUrl).host)
                    val content = "Android attachment ${UUID.randomUUID()}".toByteArray()
                    val pending =
                        sender.attachments().create(
                            AttachmentSource.Bytes(content, "android.txt", "text/plain"),
                        )
                    val remote = pending.remoteAttachment()
                    val dm = sender.conversations.createDm(receiver.inboxId())
                    val id = dm.sendRemoteAttachment(remote)
                    pending.upload()
                    receiver.conversations.syncAll(null)
                    val message = checkNotNull(receiver.conversations.getMessageById(id))
                    val received =
                        ((message.content as SDKMessageContent.Standard).value as MessageContent.RemoteAttachment)
                            .v1
                    val downloaded = receiver.attachments().download(received)
                    assertArrayEquals(content, File(downloaded.path).readBytes())
                    assertEquals("android.txt", downloaded.filename)
                }
            }
        }

    @Test fun cancelledCollectorReplaysTheUnacknowledgedMessage() =
        runBlocking {
            withTimeout(60_000) {
                clients { sender, receiver ->
                    val group = sender.conversations.createGroup(listOf(receiver.inboxId()))
                    receiver.conversations.syncAll(null)
                    val received = checkNotNull(receiver.conversations.getById(group.id())) as Conversation.Group
                    val delivered = CompletableDeferred<Message>()
                    val collector =
                        launch {
                            received.group.streamMessages().collect { message ->
                                if ((message.content as? SDKMessageContent.Standard)?.value ==
                                    MessageContent.Text("held")
                                ) {
                                    delivered.complete(message)
                                    awaitCancellation()
                                }
                            }
                        }
                    val id = group.sendText("held")
                    val first = delivered.await()
                    collector.cancelAndJoin()
                    val replay = received.group.streamMessages().first { it.id == id }
                    assertEquals(first.id, replay.id)
                    assertEquals(first.deliveryCursor, replay.deliveryCursor)
                }
            }
        }

    @Test fun signerAndCredentialThrowablesReturnTypedFailures() =
        runBlocking {
            withTimeout(30_000) {
                val fresh = generateLocalSigner()
                val throwing =
                    object : Signer {
                        override suspend fun identity() = fresh.identity()

                        override suspend fun kind() = fresh.kind()

                        override suspend fun sign(request: SigningRequest): Signature =
                            throw LinkageError("private signer detail")
                    }
                val unsigned =
                    SDKClient.create(
                        context,
                        throwing,
                        options("callback-${UUID.randomUUID()}").copy(
                            registration = RegistrationOptions(auto = false),
                        ),
                    )
                try {
                    try {
                        unsigned.register()
                        fail("Expected signer failure")
                    } catch (
                        error: XmtpException.Signer,
                    ) {
                        assertFalse(error.message.orEmpty().contains("private signer detail"))
                    }
                } finally {
                    withContext(NonCancellable) { unsigned.storage().delete() }
                }
                val source =
                    object : CredentialSource {
                        override suspend fun credential(): Credential = throw LinkageError("private credential detail")
                    }
                try {
                    val unexpected =
                        SDKClient.create(
                            context,
                            fresh,
                            options("credential-${UUID.randomUUID()}").copy(
                                backend =
                                    BackendSource.Options(
                                        BackendOptions(url = BuildConfig.XMTP_BACKEND_URL, credentials = source),
                                    ),
                            ),
                        )
                    withContext(NonCancellable) { unexpected.storage().delete() }
                    fail("Expected credential failure")
                } catch (error: XmtpException.CredentialCallbackFailed) {
                    assertFalse(error.message.orEmpty().contains("private credential detail"))
                }
            }
        }

    @Test fun persistentLogHelpersListFilesAndClearThem() =
        runBlocking {
            initLogging(LoggingOptions(level = LogLevel.DEBUG))
            SDKClient.activatePersistentLibXMTPLogWriter(context, LogLevel.DEBUG, LogRotation.MINUTELY, 3u)
            SDKClient.deactivatePersistentLibXMTPLogWriter()
            val directory = File(context.filesDir, "xmtp_logs")
            val marker = File(directory, "android-helper-test.log").also { it.writeText("log helper marker") }
            assertTrue(SDKClient.getXMTPLogFilePaths(context).contains(marker.absolutePath))
            assertTrue(SDKClient.clearXMTPLogs(context) >= 1)
            assertFalse(marker.exists())
        }

    // The Rust file writer, not a test marker, must produce the file the
    // Android helpers list and clear.
    @Test fun persistentLogWriterWritesAFileTheHelpersListAndClear() =
        runBlocking {
            initLogging(LoggingOptions(level = LogLevel.TRACE))
            SDKClient.clearXMTPLogs(context)
            SDKClient.activatePersistentLibXMTPLogWriter(context, LogLevel.TRACE, LogRotation.HOURLY, 3u)
            try {
                val client = SDKClient.create(context, generateLocalSigner(), options("log-${UUID.randomUUID()}"))
                try {
                    client.conversations.createGroup(emptyList<InboxId>())
                    client.conversations.sync()
                } finally {
                    withContext(NonCancellable) { client.storage().delete() }
                }
            } finally {
                SDKClient.deactivatePersistentLibXMTPLogWriter()
            }
            val files = SDKClient.getXMTPLogFilePaths(context)
            assertEquals(1, files.size)
            assertTrue(File(files.single()).length() > 0)
            SDKClient.clearXMTPLogs(context)
            assertTrue(SDKClient.getXMTPLogFilePaths(context).isEmpty())
        }

    // verifies: PROC-036
    @Test fun hmacKeyDiagnosticsKeepDataAndHideBytes() {
        val sentinel = byteArrayOf(19, -42, 67, 11)
        val value = HmacKey(sentinel, 42L)
        assertArrayEquals(sentinel, value.key)
        assertEquals(42L, value.epoch)
        val forms = listOf(value.toString(), listOf(value).toString(), mapOf("scope" to listOf(value)).toString())
        println("HMAC test-sentinel diagnostics: $forms")
        val leaked = forms.count { it.contains("19, -42, 67, 11") }
        assertEquals("HMAC diagnostics expose test bytes", 0, leaked)
    }
}
