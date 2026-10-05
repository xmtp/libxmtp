import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.lang.ref.WeakReference
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

fun main() =
    runBlocking {
        if (System.getenv("SDK_CALLBACK_LIFETIME") == "1") {
            callbackLifetime(BackendOptions(url = System.getenv("XMTP_BACKEND_URL")))
            return@runBlocking
        }
        System.getenv("SDK_RETAINED_CASE")?.let { name ->
            runRetainedConformanceCase(name, BackendOptions(url = System.getenv("XMTP_BACKEND_URL")))
            return@runBlocking
        }
        check(sdkVersion().startsWith("1.12.0"))
        val messageId: MessageId = "a".repeat(64)
        check(messageId.length == 64)
        println("Kotlin scenario 1: load, checksums, version passed")
        checkPureInboxIdCalculation()
        checkNativeConfigurationRecordProjection()

        // Client-free standard codecs encode the client's bytes and round trip.
        // verifies: CTYPE-007, CTYPE-026
        val codecSamples = sdkConformanceStandardSamples()
        check(codecSamples.size == 15) { "missing standard codec samples" }
        checkCodecRecordValues()
        for (sample in codecSamples) {
            val expected = sample.expected
            val matches =
                when (val content = sample.value) {
                    is StandardContent.Text -> {
                        matchesRust(TextCodec(), content.v1, expected)
                    }

                    is StandardContent.Markdown -> {
                        matchesRust(MarkdownCodec(), content.v1, expected)
                    }

                    StandardContent.ReadReceipt -> {
                        matchesRust(ReadReceiptCodec(), Unit, expected)
                    }

                    is StandardContent.Reaction -> {
                        matchesRust(
                            ReactionV2Codec(),
                            ReactionV2Content(content.reference, content.referenceInboxId, content.reaction),
                            expected,
                        )
                    }

                    is StandardContent.Attachment -> {
                        matchesRust(AttachmentCodec(), content.v1, expected)
                    }

                    is StandardContent.RemoteAttachment -> {
                        matchesRust(RemoteAttachmentCodec(), content.v1, expected)
                    }

                    is StandardContent.MultiRemoteAttachment -> {
                        matchesRust(MultiRemoteAttachmentCodec(), content.v1, expected)
                    }

                    is StandardContent.TransactionReference -> {
                        matchesRust(TransactionReferenceCodec(), content.v1, expected)
                    }

                    is StandardContent.WalletSendCalls -> {
                        matchesRust(WalletSendCallsCodec(), content.v1, expected)
                    }

                    is StandardContent.Actions -> {
                        matchesRust(ActionsCodec(), content.v1, expected)
                    }

                    is StandardContent.Intent -> {
                        matchesRust(IntentCodec(), content.v1, expected)
                    }

                    is StandardContent.Reply -> {
                        matchesRust(
                            ReplyCodec(),
                            ReplyContent(content.reference, content.referenceInboxId, content.content),
                            expected,
                        )
                    }

                    is StandardContent.GroupUpdated -> {
                        matchesRust(GroupUpdatedCodec(), content.v1, expected)
                    }

                    is StandardContent.DeleteMessage -> {
                        matchesRust(DeleteMessageCodec(), DeleteMessageContent(content.messageId), expected)
                    }

                    is StandardContent.LeaveRequest -> {
                        matchesRust(LeaveRequestCodec(), content.v1, expected)
                    }
                }
            check(matches) { "standard codec content differs from Rust" }
        }
        println("Kotlin P69: all 15 standard codecs match Rust bytes")

        val failingSigner =
            SDKForeign.signer(
                object : Signer {
                    override suspend fun identity(): PublicIdentity = throw AssertionError("host failure")

                    override suspend fun kind(): SignerKind = throw AssertionError("host failure")

                    override suspend fun sign(request: SigningRequest): Signature = throw AssertionError("host failure")
                },
            )
        check(runCatching { withTimeout(5_000) { failingSigner.kind() } }.exceptionOrNull() is SignerException.Failed)
        val failingCredentials =
            SDKForeign.credentials(
                object : CredentialSource {
                    override suspend fun credential(): Credential = throw AssertionError("host failure")
                },
            )
        check(
            runCatching { withTimeout(5_000) { failingCredentials.credential() } }.exceptionOrNull()
                is CredentialException.Failed,
        )
        val cancelled = CancellationException("real cancellation")
        val cancelledSigner =
            SDKForeign.signer(
                object : Signer {
                    override suspend fun identity(): PublicIdentity = throw cancelled

                    override suspend fun kind(): SignerKind = throw cancelled

                    override suspend fun sign(request: SigningRequest): Signature = throw cancelled
                },
            )
        check(runCatching { cancelledSigner.kind() }.exceptionOrNull() === cancelled)
        val failingSink =
            SDKForeign.logSink(
                object : LogSink {
                    override suspend fun log(record: LogRecord): Unit = throw AssertionError("host failure")
                },
            )
        check(
            runCatching {
                failingSink.log(LogRecord(LogLevel.ERROR, "test", "message", emptyMap(), Timestamp(0), 0uL))
            }.exceptionOrNull() is LogSinkException.Failed,
        )
        val cancellingSink =
            SDKForeign.logSink(
                object : LogSink {
                    override suspend fun log(record: LogRecord): Unit = throw cancelled
                },
            )
        check(
            runCatching {
                cancellingSink.log(LogRecord(LogLevel.ERROR, "test", "message", emptyMap(), Timestamp(0), 0uL))
            }.exceptionOrNull() === cancelled,
        )
        println("Kotlin P37 foreign trait wrappers passed")

        val signer = TestSigner()
        val androidFiles = Files.createTempDirectory("xmtp-sdk-android-files-").toFile()
        val androidContext =
            object : android.content.Context() {
                override val filesDir = androidFiles
            }
        val androidStorage = StorageOptions(androidContext, label = "phone")
        check(androidStorage.location == StorageLocation.Directory(androidFiles.resolve("xmtp_db").absolutePath))
        check(androidStorage.label == "phone")
        val backendOptions = BackendOptions(url = checkNotNull(System.getenv("XMTP_BACKEND_URL")))
        checkCredentialCallbacksStayDistinct()
        checkCredentialDisplayRedactsToken()
        checkConfigurationDiscoveryDoesNotCallCredentials(backendOptions)
        checkStoragePoolOptionsCrossTheNativeBoundary(backendOptions)
        checkStorageReconnectAndRebuildKeepHistory(backendOptions)
        checkConfigurationRefreshKeepsTheHeldSnapshot(backendOptions)
        checkReaderCollectorBoundarySurvivesDatabaseReopen(backendOptions)
        checkEndedClientCannotHandOffReaderValues(backendOptions)
        check(ClientOptions(storage = StorageOptions(location = StorageLocation.InMemory)).backend == null)
        val options =
            ClientOptions(
                backend = BackendSource.Options(backendOptions),
                storage = androidStorage,
                deviceSync = false,
            )
        loggingConformance(options)
        checkReaderCursor(signer, backendOptions)
        checkHistorySnapshots(backendOptions)
        checkRestoredPeer(backendOptions)
        checkIdentityRoutes(backendOptions)
        checkStorageLayout(backendOptions)
        checkAttachmentSettings(backendOptions)
        checkAttachmentFlow(backendOptions)
        checkAttachmentFailures(backendOptions)
        checkAttachmentRecords(backendOptions)
        checkAttachmentEnd(backendOptions)
        val host = SDKClient.create(signer, options)
        checkReaderReadFailuresEndExactlyOnce(host)
        checkReaderCollectorCloseReasons(host)
        val client = host
        // Uppercase hex decodes, so only ID validation rejects it.
        val invalidId = runCatching { client.conversations().getMessageById("AB".repeat(32)) }.exceptionOrNull()
        check(invalidId is XmtpException.InvalidArgument)
        check(invalidId.v1.code == "InvalidArgument")
        check(invalidId.v1.category == ErrorCategory.INPUT)
        check(!invalidId.v1.retryable)
        val inboxId = client.inboxId()
        val storagePath = checkNotNull(host.storage().path())
        check(Files.isRegularFile(Path.of(storagePath))) { "storage path does not name the database file" }
        val deployment = deploymentComponent(client.serverConfiguration().identifier)
        check(storagePath == androidFiles.resolve("xmtp_db/phone/$deployment/$inboxId/xmtp.db3").absolutePath)
        val group = client.conversations().createGroup(emptyList(), null)
        var typedSends = 0
        for (sample in codecSamples) {
            val id =
                when (val value = sample.value) {
                    is StandardContent.Text -> {
                        group.sendText(value.v1, null)
                    }

                    is StandardContent.Markdown -> {
                        group.sendMarkdown(value.v1, null)
                    }

                    is StandardContent.Reaction -> {
                        group.sendReaction(
                            value.reference,
                            value.referenceInboxId,
                            value.reaction,
                            null,
                        )
                    }

                    is StandardContent.Reply -> {
                        group.sendReply(
                            value.reference,
                            value.referenceInboxId,
                            value.content,
                            null,
                        )
                    }

                    StandardContent.ReadReceipt -> {
                        group.sendReadReceipt(null)
                    }

                    is StandardContent.Attachment -> {
                        group.sendAttachment(value.v1, null)
                    }

                    is StandardContent.RemoteAttachment -> {
                        group.sendRemoteAttachment(value.v1, null)
                    }

                    is StandardContent.MultiRemoteAttachment -> {
                        group.sendMultiRemoteAttachment(value.v1, null)
                    }

                    is StandardContent.TransactionReference -> {
                        group.sendTransactionReference(value.v1, null)
                    }

                    is StandardContent.WalletSendCalls -> {
                        group.sendWalletSendCalls(value.v1, null)
                    }

                    is StandardContent.Actions -> {
                        group.sendActions(value.v1, null)
                    }

                    is StandardContent.Intent -> {
                        group.sendIntent(value.v1, null)
                    }

                    else -> {
                        continue
                    }
                }
            val wire = checkNotNull(client.conversations().getMessageById(id))
            check(sameEncoded(wire.encoded, sample.expected)) { "typed send content differs from codec" }
            typedSends++
        }
        check(typedSends == 12)
        println("Kotlin P69: typed send bytes match all 12 public codecs")
        val sentId = group.sendText("conformance message", null)
        val sent = group.messages(null).first { it.id == sentId }
        check(sent.client() === host)
        host.end()
        check(runCatching { sent.client() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(runCatching { sent.refresh() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(Message(sent.data.copy(clientKey = sent.data.clientKey + 1uL)) != sent)
        val reopenedHost = SDKClient.build(signer.identity(), options, inboxId)
        val reopened = reopenedHost
        check(reopened.inboxId() == inboxId)
        check(
            runCatching {
                SDKClient.build(
                    signer.identity(),
                    options.copy(storage = options.storage.copy(location = StorageLocation.Default)),
                    inboxId,
                )
            }.exceptionOrNull() is XmtpException.StorageLocationRequired,
        )
        val defaultDirectory = Files.createTempDirectory("xmtp-sdk-default-")
        check(
            runCatching {
                SDKClient.build(
                    signer.identity(),
                    options.copy(storage = options.storage.copy(location = StorageLocation.Default)),
                    inboxId,
                    defaultDirectory = defaultDirectory.toString(),
                )
            }.exceptionOrNull() is XmtpException.IdentityNotFound,
        )
        check(Files.walk(defaultDirectory).use { paths -> paths.noneMatch { it.fileName.toString().endsWith(".db3") } })
        val (orphan, weak) = releasedMessage(signer.identity(), options, inboxId)
        // The run task uses SerialGC with explicit GC enabled, so System.gc() runs a full collection.
        repeat(50) {
            if (weak.get() == null) return@repeat
            System.gc()
            delay(50)
        }
        check(weak.get() == null) { "the registry kept the host client alive" }
        check(runCatching { orphan.client() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(runCatching { orphan.refresh() }.exceptionOrNull() is XmtpException.ClientClosed)
        println("Kotlin client_closed_after_end_and_release passed")
        println("Kotlin scenario 2: create, reopen, end passed")

        val liveGroup = reopened.conversations().createGroup(emptyList(), null)
        val reader = liveGroup.messageReader()
        val liveId = liveGroup.sendText("durable stream", null)
        check(reader.next()?.id == liveId)
        reader.end()
        val replay = liveGroup.messageReader()
        check(replay.next()?.id == liveId)
        val pending = async { replay.next() }
        delay(50)
        pending.cancel()
        replay.end()
        runCatching { pending.await() }
        val adapterId = liveGroup.sendText("adapter stream", null)
        var delivered = false
        try {
            withTimeout(10_000) {
                reopenedHost.messages(liveGroup).collect { message ->
                    check(message.id == adapterId)
                    delivered = true
                }
            }
        } catch (_: TimeoutCancellationException) {
            check(delivered)
        }
        val protocolGroup = reopened.conversations().createGroup(emptyList(), null)
        val firstId = protocolGroup.sendText("ack on request", null)
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(1)
                .toList()
                .single()
                .id == firstId,
        )
        val reread = protocolGroup.messageReader()
        check(withTimeout(3_000) { reread.next() }?.id == firstId) {
            "adapter prefetched and acknowledged a value"
        }
        reread.end()
        val secondId = protocolGroup.sendText("second request", null)
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(2)
                .toList()
                .map { it.id } == listOf(firstId, secondId),
        )
        val afterAck = protocolGroup.messageReader()
        check(afterAck.next()?.id == secondId) { "adapter did not acknowledge on next request" }
        afterAck.end()
        val breakGroup = reopened.conversations().createGroup(emptyList(), null)
        val breakId = breakGroup.sendText("close after take", null)
        val breakReasons = mutableListOf<SDKStreamCloseReason>()
        val retainedFlow = reopenedHost.messages(breakGroup, onClose = { breakReasons.add(it) })
        check(
            retainedFlow
                .take(1)
                .toList()
                .single()
                .id == breakId,
        )
        check(breakReasons == listOf(SDKStreamCloseReason.Closed)) { "take did not close the stored flow" }
        val breakReplay = breakGroup.messageReader()
        check(withTimeout(3_000) { breakReplay.next() }?.id == breakId) {
            "take acknowledged the last message"
        }
        breakReplay.end()
        val firstReasons = mutableListOf<SDKStreamCloseReason>()
        val retainedFirst = reopenedHost.messages(breakGroup, onClose = { firstReasons.add(it) })
        check(retainedFirst.first().id == breakId)
        check(firstReasons == listOf(SDKStreamCloseReason.Closed)) { "first did not close the stored flow" }
        val callbackReasons = mutableListOf<SDKStreamCloseReason>()
        val closeCallbackFlow =
            reopenedHost.messages(
                breakGroup,
                onClose = {
                    callbackReasons.add(it)
                    throw IllegalStateException("close callback failed")
                },
            )
        check(
            withTimeout(3_000) {
                closeCallbackFlow
                    .take(1)
                    .toList()
                    .single()
                    .id
            } == breakId,
        ) {
            "close callback error escaped message collection"
        }
        check(callbackReasons == listOf(SDKStreamCloseReason.Closed))
        val thrownReasons = mutableListOf<SDKStreamCloseReason>()
        val retainedThrown = reopenedHost.messages(breakGroup, onClose = { thrownReasons.add(it) })
        try {
            retainedThrown.collect { throw IllegalStateException("collector stopped") }
            error("collector exception did not leave the flow")
        } catch (error: IllegalStateException) {
            check(error.message == "collector stopped")
        }
        check(thrownReasons == listOf(SDKStreamCloseReason.Closed)) {
            "collector exception did not close the stored flow"
        }
        val thrownReplay = breakGroup.messageReader()
        check(withTimeout(3_000) { thrownReplay.next() }?.id == breakId) {
            "collector exception acknowledged the last message"
        }
        thrownReplay.end()
        val stateGroup = reopened.conversations().createGroup(emptyList(), null)
        val stateId = stateGroup.sendText("throwing state callback", null)
        val uncaughtStateError = AtomicReference<Throwable?>()
        val previousHandler = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { _, error -> uncaughtStateError.compareAndSet(null, error) }
        try {
            val stateFlow =
                reopenedHost.messages(
                    stateGroup,
                    onConnectionStateChange = { _, _ -> throw IllegalStateException("state callback failed") },
                )
            check(
                withTimeout(3_000) {
                    stateFlow
                        .take(1)
                        .toList()
                        .single()
                        .id
                } == stateId,
            )
            delay(100)
            check(uncaughtStateError.get() == null) { "state callback crashed its coroutine" }
        } finally {
            Thread.setDefaultUncaughtExceptionHandler(previousHandler)
        }
        val openedReader = CompletableDeferred<MessageReader>()
        val releaseOpening = CompletableDeferred<Unit>()
        SDKClient.readerOpenedForTest = { opened ->
            openedReader.complete(opened)
            releaseOpening.await()
        }
        val openingClosed = CompletableDeferred<SDKStreamCloseReason>()
        val cancelledOpening =
            async(start = CoroutineStart.UNDISPATCHED) {
                reopenedHost.messages(protocolGroup, onClose = { openingClosed.complete(it) }).collect {}
            }
        val lateReader = withTimeout(10_000) { openedReader.await() }
        cancelledOpening.cancel(CancellationException("cancel during reader creation"))
        delay(100)
        check(!openingClosed.isCompleted) { "close callback ran before the late reader ended" }
        releaseOpening.complete(Unit)
        withTimeout(3_000) { cancelledOpening.join() }
        check(withTimeout(3_000) { openingClosed.await() } == SDKStreamCloseReason.Closed) {
            "late reader did not report a closed stream"
        }
        SDKClient.readerOpenedForTest = null
        withTimeout(10_000) {
            while (lateReader.connectionState() != ConnectionState.CLOSED) delay(10)
        }
        check(withTimeout(10_000) { lateReader.next() } == null) { "late reader was not ended" }
        val reopenedReader = protocolGroup.messageReader()
        reopenedReader.end()
        var conversationClose: SDKStreamCloseReason? = null
        val conversationValues =
            async {
                reopenedHost
                    .conversationStream(onClose = { conversationClose = it })
                    .take(1)
                    .toList()
            }
        delay(100)
        reopened.conversations().createGroup(emptyList(), null)
        check(withTimeout(15_000) { conversationValues.await() }.size == 1)
        check(conversationClose == SDKStreamCloseReason.Closed)
        val throwingConversationReasons = mutableListOf<SDKStreamCloseReason>()
        val throwingConversationValues =
            async {
                reopenedHost
                    .conversationStream(
                        onClose = {
                            throwingConversationReasons.add(it)
                            throw IllegalStateException("conversation close callback failed")
                        },
                    ).take(1)
                    .toList()
            }
        delay(100)
        reopened.conversations().createGroup(emptyList(), null)
        check(withTimeout(15_000) { throwingConversationValues.await() }.size == 1)
        check(throwingConversationReasons == listOf(SDKStreamCloseReason.Closed))
        val monitorCalls = AtomicInteger()
        val monitorClosed = CompletableDeferred<Unit>()
        val fakeMonitor =
            async {
                readerFlow<Unit, Unit>(
                    owner = reopenedHost,
                    open = { Unit },
                    next = { awaitCancellation() },
                    end = {},
                    connectionState = { ConnectionState.CONNECTING },
                    connectionStateChanged = { _, _ ->
                        monitorCalls.incrementAndGet()
                        ConnectionState.CLOSED
                    },
                    onClose = null,
                    onConnectionStateChange = { _, current ->
                        if (current == ConnectionState.CLOSED) monitorClosed.complete(Unit)
                    },
                ).collect {}
            }
        try {
            withTimeout(5_000) { monitorClosed.await() }
            val callsAtClosed = monitorCalls.get()
            delay(100)
            check(monitorCalls.get() == callsAtClosed) { "state monitor kept reading after Closed" }
        } finally {
            fakeMonitor.cancelAndJoin()
        }
        // verifies: PROC-044
        // A reader opened on a connected connection reports Connected first.
        val firstState = CompletableDeferred<Pair<ConnectionState?, ConnectionState>>()
        val connectedMonitor =
            async {
                readerFlow<Unit, Unit>(
                    owner = reopenedHost,
                    open = { Unit },
                    next = { awaitCancellation() },
                    end = {},
                    connectionState = { ConnectionState.CONNECTED },
                    connectionStateChanged = { _, _ -> awaitCancellation() },
                    onClose = null,
                    onConnectionStateChange = { previous, current -> firstState.complete(previous to current) },
                ).collect {}
            }
        try {
            val first = withTimeout(5_000) { firstState.await() }
            check(first == (null to ConnectionState.CONNECTED)) { "connected reader first reported $first" }
        } finally {
            connectedMonitor.cancelAndJoin()
        }
        println("Kotlin scenario 7: durable stream and idle cancellation passed")

        val largeExpiry = 9_007_199_254_740_993L
        val credentialOptions =
            options.copy(
                backend =
                    BackendSource.Options(
                        BackendOptions(
                            url = backendOptions.url,
                            credential = Credential(null, "Bearer initial", largeExpiry),
                        ),
                    ),
                storage = options.storage,
                workers = WorkerOptions(defaultIntervalNs = largeExpiry.toULong()),
            )
        val credentialHost = SDKClient.build(signer.identity(), credentialOptions, inboxId)
        val savedOptions = credentialHost.options()
        check(savedOptions.workers?.defaultIntervalNs == largeExpiry.toULong()) {
            "worker interval lost 64-bit precision"
        }
        // The options never return the backend token or the database key.
        val savedBackend = (savedOptions.backend as BackendSource.Options).options
        check(
            savedBackend.credential == null &&
                savedBackend.credentials == null &&
                savedOptions.storage.encryptionKey == null,
        ) {
            "client options exposed a secret"
        }
        credentialHost.setCredential(Credential(null, "Bearer renewed", largeExpiry))
        credentialHost.end()
        println("Kotlin scenario 3: credential update and 64-bit value passed")

        val snapshot = reopened.serverConfiguration()
        val fetched = fetchServerConfiguration(BackendSource.Options(backendOptions))
        val staticBackend = Backend.connect(backendOptions)
        check(SDKClient.inboxIdFor(signer.identity(), BackendSource.Connected(staticBackend)) == inboxId)
        check(
            SDKClient.canMessage(
                listOf(signer.identity()),
                BackendSource.Connected(staticBackend),
            )["ethereum:${signer.identity().identifier}"] ==
                true,
        )
        check(
            SDKClient.canMessage(
                listOf(signer.identity()),
                BackendSource.Options(backendOptions),
            )["ethereum:${signer.identity().identifier}"] ==
                true,
        )
        val sameText = "1111111111111111111111111111111111111111"
        val mixedIdentities =
            listOf(
                PublicIdentity(sameText, PublicIdentityKind.ETHEREUM),
                PublicIdentity(sameText, PublicIdentityKind.PASSKEY),
                signer.identity(),
            )
        val registeredKey = "ethereum:${signer.identity().identifier}"

        fun checkMixedCanMessage(result: Map<String, Boolean>) {
            check(result.size == 3)
            check(result["ethereum:$sameText"] == false)
            check(result["passkey:$sameText"] == false)
            check(result[registeredKey] == true)
        }
        checkMixedCanMessage(reopened.canMessage(mixedIdentities))
        checkMixedCanMessage(SDKClient.canMessage(mixedIdentities, BackendSource.Connected(staticBackend)))
        checkMixedCanMessage(SDKClient.canMessage(mixedIdentities, BackendSource.Options(backendOptions)))
        check(
            runCatching {
                SDKClient.build(
                    signer.identity(),
                    options.copy(
                        backend = BackendSource.Connected(staticBackend),
                        storage = StorageOptions(location = StorageLocation.InMemory),
                    ),
                    inboxId,
                )
            }.exceptionOrNull() is XmtpException.IdentityNotFound,
        )
        check(fetched.identifier == snapshot.identifier)
        check(reopened.refreshServerConfiguration().identifier == snapshot.identifier)
        check(
            runCatching { fetchServerConfiguration(BackendSource.Options(BackendOptions(url = "http://127.0.0.1:1"))) }
                .exceptionOrNull() is XmtpException.ConfigurationUnavailable,
        )
        println("Kotlin scenario 10: configuration and typed error passed")

        val local = generateLocalSigner()
        val unsignedOptions =
            options.copy(
                storage = StorageOptions(location = StorageLocation.InMemory),
                registration = RegistrationOptions(auto = false),
            )
        val unsignedHost = SDKClient.create(local, unsignedOptions)
        check(!unsignedHost.isRegistered())
        val request = checkNotNull(unsignedHost.unsafeCreateInboxSignatureRequest())
        check(request.signatureText().isNotEmpty())
        request.sign(local)
        unsignedHost.unsafeApplySignatureRequest(request)
        check(unsignedHost.isRegistered())
        unsignedHost.end()
        println("Kotlin scenario 11: local signer and signature request passed")
        metadataFields(options)
        println("Kotlin metadata fields and profiles passed")

        // verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
        val preAuthCalls = java.util.Collections.synchronizedList(mutableListOf<String>())
        val preAuthenticated =
            SDKClient.create(
                RecordingSigner(generateLocalSigner(), preAuthCalls),
                unsignedOptions.copy(handlers = ClientHandlers(RecordingPreAuthenticate(preAuthCalls, fail = false))),
            )
        check(preAuthCalls.isEmpty())
        preAuthenticated.register()
        check(preAuthCalls == listOf("pre-authenticate", "sign")) { "$preAuthCalls" }
        preAuthCalls.clear()
        preAuthenticated.register()
        check(preAuthCalls.isEmpty()) { "$preAuthCalls" }
        preAuthenticated.end()
        check(
            runCatching {
                SDKClient.create(
                    RecordingSigner(generateLocalSigner(), preAuthCalls),
                    unsignedOptions.copy(
                        registration = RegistrationOptions(auto = true),
                        handlers = ClientHandlers(RecordingPreAuthenticate(preAuthCalls, fail = true)),
                    ),
                )
            }.exceptionOrNull() is XmtpException.CallbackFailed,
        )
        check(preAuthCalls == listOf("pre-authenticate")) { "$preAuthCalls" }
        println("Kotlin host preAuthenticate runs before the signer")

        check(reopened.notificationState() == NotificationState.Disabled)
        check(
            runCatching {
                reopened.enableNotifications(
                    NotificationConfig(channel = NotificationChannel.Http("https://example.com", byteArrayOf(1))),
                )
            }.exceptionOrNull() is XmtpException.InvalidArgument,
        )
        println("Kotlin scenario 12: notification state and typed error passed")

        val fresh = generateLocalSigner()
        val errorSigner =
            object : Signer {
                override suspend fun identity() = fresh.identity()

                override suspend fun kind() = fresh.kind()

                override suspend fun sign(request: SigningRequest): Signature = throw Error("signer failed")
            }
        val errorHost = SDKClient.create(errorSigner, unsignedOptions)
        check(
            withTimeout(10_000) { runCatching { errorHost.register() }.exceptionOrNull() } is XmtpException.Signer,
        )
        errorHost.end()
        val unsignedErrorHost = SDKClient.create(generateLocalSigner(), unsignedOptions)
        val errorRequest = checkNotNull(unsignedErrorHost.unsafeCreateInboxSignatureRequest())
        check(
            withTimeout(10_000) { runCatching { errorRequest.sign(errorSigner) }.exceptionOrNull() }
                is XmtpException.Signer,
        )
        unsignedErrorHost.end()
        val failingSource =
            object : CredentialSource {
                override suspend fun credential(): Credential = throw Error("credential source failed")
            }
        val failedCredential =
            withTimeout(10_000) {
                runCatching {
                    SDKClient.create(
                        signer,
                        options.copy(
                            backend =
                                BackendSource.Options(
                                    BackendOptions(url = backendOptions.url, credentials = failingSource),
                                ),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                        ),
                    )
                }.exceptionOrNull()
            }
        check(
            failedCredential is XmtpException.CredentialCallbackFailed,
        ) { "credential Error became $failedCredential" }
        println("Kotlin signer Error: call failed without a hang")

        val family = reopened.conversations().createGroup(emptyList(), CreateGroupOptions(name = "family group"))
        check(family.state().name == "family group")
        check(family.creatorInboxId() == inboxId)
        check(reopened.conversations().listGroups(null).any { it.id() == family.id() })
        println("Kotlin scenario 4: group options, state, and list passed")

        val parentId = family.sendText("parent", null)
        val reactionId =
            reopened.conversations().reactToMessage(
                parentId,
                Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
                null,
            )
        val replyId = reopened.conversations().replyToMessage(parentId, encodeText("reply"), null)
        check(reopened.decodeContent(encodeText("decoded")) is MessageContent.Text)
        val familyMessages = family.messages(null)
        val parent = familyMessages.first { it.id == parentId }
        val reply = familyMessages.first { it.id == replyId }
        check(parent.replyCount == 1uL && parent.reactions.firstOrNull()?.id == reactionId)
        check(reply.inReplyTo?.id == parentId)
        val reactionMessage = checkNotNull(reopened.conversations().getMessageById(reactionId))
        val reactionContent =
            (reactionMessage.content as? SDKMessageContent.Standard)?.value as? MessageContent.Reaction
        check(
            reactionContent?.reference == parentId && reactionContent.referenceInboxId == inboxId &&
                reactionContent.reaction.content == "👍",
        ) { "reaction content lost its target" }
        check(
            reactionMessage !=
                Message(
                    reactionMessage.data.copy(content = reactionContent.copy(reference = reactionId)),
                ),
        ) { "reaction target did not affect message equality" }
        val changedEnvelope =
            Message(
                parent.data.copy(
                    encoded = checkNotNull(parent.encoded).copy(parameters = mapOf("key" to "different")),
                ),
            )
        check(parent != changedEnvelope) { "EncodedContent parameters must affect message equality" }
        val copiedBytes =
            Message(
                parent.data.copy(
                    encoded =
                        checkNotNull(
                            parent.encoded,
                        ).copy(content = checkNotNull(parent.encoded).content.copyOf()),
                ),
            )
        check(parent == copiedBytes && parent.hashCode() == copiedBytes.hashCode())
        val sameParent = checkNotNull(reopened.conversations().getMessageById(parentId))
        check(parent == sameParent) { "message_copies_compare_equal failed" }
        check(parent != Message(parent.data.copy(reactions = emptyList()))) {
            "reaction_change_compares_unequal failed"
        }
        val replyParent = checkNotNull(reply.data.inReplyTo)
        check(reply != Message(reply.data.copy(inReplyTo = replyParent.copy(content = MessageBody.Text("changed"))))) {
            "reply_parent_change_compares_unequal failed"
        }
        val changedStatus =
            parent.data.copy(
                deliveryStatus =
                    if (parent.deliveryStatus ==
                        DeliveryStatus.FAILED
                    ) {
                        DeliveryStatus.PUBLISHED
                    } else {
                        DeliveryStatus.FAILED
                    },
            )
        check(parent != Message(changedStatus)) { "status_change_compares_unequal failed" }
        val encodedCopyA = encodeText("value equality")
        val encodedCopyB = encodeText("value equality")
        check(encodedCopyA == encodedCopyB && encodedCopyA.hashCode() == encodedCopyB.hashCode()) {
            "generated byte record equality failed"
        }
        val forwarded: Conversation = Conversation.Group(family)
        check(forwarded.id() == family.id() && forwarded.lastMessage()?.id == family.lastMessage()?.id)
        println("Kotlin message_copies_compare_equal and status_change_compares_unequal passed")
        println("Kotlin scenario 5: message records, reaction, and reply passed")

        val codec = SampleCodec()
        val withCodec = SDKClient.build(signer.identity(), options, inboxId, codecs = listOf(codec))
        val withoutCodec = SDKClient.build(signer.identity(), options, inboxId)
        val slashType = ContentTypeId("example.org", "a/b", 1u, 0u)
        val slashCodec =
            object : ContentCodec<String> {
                override val type = slashType

                override fun encode(value: String) = EncodedContent(type, emptyMap(), null, value.toByteArray())

                override fun decode(encoded: EncodedContent) = "wrong codec"
            }
        val slashHost = SDKClient.build(signer.identity(), options, inboxId, codecs = listOf(slashCodec))
        val colliding = EncodedContent(ContentTypeId("example.org/a", "b", 1u, 0u), emptyMap(), null, byteArrayOf(1))
        check(slashHost.decodeCustom(colliding, byteArrayOf()) is SDKMessageContent.Unknown) {
            "codec key collision selected the wrong codec"
        }
        slashHost.end()
        val customId = family.send(codec.encode("codec value"), null)
        val decoded = checkNotNull(withCodec.conversations().getMessageById(customId))
        val undecoded = checkNotNull(withoutCodec.conversations().getMessageById(customId))
        check((decoded.content as? SDKMessageContent.Custom)?.value == "codec value")
        check(undecoded.content is SDKMessageContent.Unknown)
        val customReplyId =
            withCodec.conversations().replyToMessage(
                customId,
                codec.encode("reply codec value"),
                null,
            )
        val customReply = checkNotNull(withCodec.conversations().getMessageById(customReplyId))
        check((customReply.replyContent as? SDKReplyContent.Custom)?.value == "reply codec value") {
            "reply body custom codec did not run"
        }
        val failingHost = SDKClient.build(signer.identity(), options, inboxId, codecs = listOf(FailingCodec()))
        val failed = checkNotNull(failingHost.conversations().getMessageById(customId))
        check(
            (failed.content as? SDKMessageContent.Custom)?.error?.let {
                it.code == "CodecDecodeFailed" &&
                    it.category == ErrorCategory.CALLBACK &&
                    !it.retryable &&
                    it.message.contains("codec decode failed")
            } ==
                true,
        )
        val failedReply = checkNotNull(failingHost.conversations().getMessageById(customReplyId))
        checkRetainedContent(failed, failedReply)
        println("Kotlin retained_content_details passed")
        // verifies: PROC-045
        val failedStreamGroup = failingHost.conversations().createGroup(emptyList(), null)
        val badStreamId = failedStreamGroup.send(codec.encode("stream codec error"), null)
        val nextStreamId = failedStreamGroup.sendText("after codec error", null)
        val codecItems = withTimeout(10_000) { failingHost.messages(failedStreamGroup).take(2).toList() }
        check(codecItems.map { it.id } == listOf(badStreamId, nextStreamId))
        val badItem =
            codecItems[0].content as? SDKMessageContent.Custom ?: error("failed custom stream content missing")
        check(
            badItem.rawBytes.isNotEmpty() && badItem.error?.code == "CodecDecodeFailed" &&
                badItem.error.category == ErrorCategory.CALLBACK,
        )
        check((codecItems[1].content as? SDKMessageContent.Standard)?.value == MessageContent.Text("after codec error"))
        println("Kotlin codec_failure_keeps_stream_open passed")
        checkHostileCodecStream(signer.identity(), options, inboxId)
        failingHost.end()
        println("Kotlin codec_scoped_to_client passed")
        val typedParent = customCodecPolicyAndIsolation(family, withoutCodec)
        println("Kotlin custom_codec_policy_and_isolation passed")
        codecPolicyFailureNeverPublishes(family, typedParent)
        println("Kotlin codec_policy_failure_never_publishes passed")
        withCodec.end()
        withoutCodec.end()
        println("Kotlin scenario 6: custom codec stayed with its client")

        checkArchiveBytesAndFile(reopened)

        // verifies: EVENT-014
        // verifies: EVENT-050
        // verifies: EVENT-052
        // verifies: EVENT-054
        val eventFilter =
            EventFilter(
                kinds = listOf(EventKind.CONVERSATION_JOINED),
                groupIds = null,
                contentTypes = null,
                referencesOwnMessages = false,
            )
        val eventReader = reopenedHost.events(eventFilter)
        val received = CompletableDeferred<Unit>()
        val listenerId = reopenedHost.startListener(eventFilter) { received.complete(Unit) }
        reopened.conversations().createGroup(emptyList(), null)
        withTimeout(10_000) { eventReader.first() }
        withTimeout(10_000) { received.await() }
        reopenedHost.stopListener(listenerId)
        println("Kotlin scenario 8: event reader and listener passed")

        // verifies: EVENT-053
        val startEntered = CompletableDeferred<Unit>()
        val releaseStart = CompletableDeferred<Unit>()
        EventStartHookForTest.beforeCallback = {
            startEntered.complete(Unit)
            releaseStart.await()
        }
        val lateCalls = AtomicInteger()
        val delayedId = reopenedHost.startListener(eventFilter) { lateCalls.incrementAndGet() }
        reopened.conversations().createGroup(emptyList(), null)
        withTimeout(10_000) { startEntered.await() }
        withTimeout(10_000) { reopenedHost.stopListener(delayedId) }
        releaseStart.complete(Unit)
        EventStartHookForTest.beforeCallback = null
        delay(100)
        check(lateCalls.get() == 0) { "callback started after stop returned" }
        println("Kotlin delayed listener stop passed")

        // verifies: EVENT-052
        val stoppedFromCallback = CompletableDeferred<Unit>()
        var reentrantId: ListenerId? = null
        reentrantId =
            reopenedHost.startListener(eventFilter) {
                reopenedHost.stopListener(requireNotNull(reentrantId))
                stoppedFromCallback.complete(Unit)
            }
        reopened.conversations().createGroup(emptyList(), null)
        withTimeout(10_000) { stoppedFromCallback.await() }
        println("Kotlin stop_from_inside_listener passed")

        val endedFromCallback = CompletableDeferred<Unit>()
        reopenedHost.startListener(eventFilter) {
            reopenedHost.end()
            endedFromCallback.complete(Unit)
        }
        runCatching { reopened.conversations().createGroup(emptyList(), null) }
        withTimeout(10_000) { endedFromCallback.await() }
        println("Kotlin end_from_inside_listener passed")

        reopenedHost.end()
    }
