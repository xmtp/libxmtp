import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.lang.ref.WeakReference
import java.nio.file.Files
import java.nio.file.Path

private fun sameEncoded(
    actual: EncodedContent,
    expected: EncodedContent,
): Boolean =
    actual.type == expected.type && actual.parameters == expected.parameters &&
        actual.fallback == expected.fallback && actual.content.contentEquals(expected.content)

private fun signCommand(
    action: String,
    text: String? = null,
): String {
    val args =
        listOf(
            System.getenv("SDK_NODE_BIN"),
            System.getenv("SDK_SIGN_SCRIPT"),
            action,
        ) + listOfNotNull(text)
    val process = ProcessBuilder(args).start()
    val result =
        process.inputStream
            .bufferedReader()
            .readText()
            .trim()
    check(process.waitFor() == 0) { process.errorStream.bufferedReader().readText() }
    return result
}

private class TestSigner : Signer {
    override suspend fun identity() = PublicIdentity(signCommand("identity"), PublicIdentityKind.ETHEREUM)

    override suspend fun kind() = SignerKind.Eoa

    override suspend fun sign(request: SigningRequest): Signature {
        val hex = signCommand("sign", request.text).removePrefix("0x")
        return Signature.Ecdsa(hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray())
    }
}

private class OrderedLogSink : LogSink {
    val sequence = mutableListOf<String>()

    override fun log(record: LogRecord) {
        if (record.target == "xmtp_sdk::conformance") {
            sequence.add(record.fields["sequence"] ?: "")
        }
    }
}

private class SampleCodec : SDKContentCodec {
    override val type = ContentTypeID("example.org", "sample", 1u, 0u)

    override fun encode(value: Any) = EncodedContent(type, emptyMap(), null, (value as String).toByteArray())

    override fun decode(encoded: EncodedContent): Any = encoded.content.decodeToString()
}

private class FailingCodec : SDKContentCodec {
    override val type = SampleCodec().type

    override fun encode(value: Any) = SampleCodec().encode(value)

    override fun decode(encoded: EncodedContent): Any = throw AssertionError("codec decode failed")
}

private suspend fun releasedMessage(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxID: InboxID,
): Pair<Message, WeakReference<SDKClient>> {
    val host = SDKClient.build(identity, options, inboxID)
    val group = host.raw.conversations().createGroup(emptyList(), null)
    val id = group.sendText("weak owner", null)
    val message = group.messages(null).first { it.id == id }
    return message to WeakReference(host)
}

fun main() =
    runBlocking {
        check(sdkVersion().startsWith("1.12.0"))
        check(MessageID.fromString("a".repeat(64)).toString().length == 64)
        check(runCatching { MessageID.fromString("bad") }.exceptionOrNull() is XmtpException.InvalidArgument)
        for (id in listOf(InboxID::class, InstallationID::class, ConversationID::class, MessageID::class)) {
            // Kotlin adds a synthetic constructor so the companion can call the private one.
            val callable = id.java.constructors.filterNot { it.isSynthetic }
            check(callable.isEmpty() && id.java.methods.none { it.name == "copy" }) {
                "${id.simpleName} can be built without fromString"
            }
        }
        println("Kotlin scenario 1: load, checksums, version passed")

        val codecSamples = sdkConformanceStandardSamples()
        check(codecSamples.size == 15) { "missing standard codec samples" }
        for (sample in codecSamples) {
            val (codec, value) =
                when (val content = sample.value) {
                    is StandardContent.Text -> TextCodec() to content.v1
                    is StandardContent.Markdown -> MarkdownCodec() to content.v1
                    StandardContent.ReadReceipt -> ReadReceiptCodec() to Unit
                    is StandardContent.Reaction -> ReactionV2Codec() to content
                    is StandardContent.Attachment -> AttachmentCodec() to content.v1
                    is StandardContent.RemoteAttachment -> RemoteAttachmentCodec() to content.v1
                    is StandardContent.MultiRemoteAttachment -> MultiRemoteAttachmentCodec() to content.v1
                    is StandardContent.TransactionReference -> TransactionReferenceCodec() to content.v1
                    is StandardContent.WalletSendCalls -> WalletSendCallsCodec() to content.v1
                    is StandardContent.Actions -> ActionsCodec() to content.v1
                    is StandardContent.Intent -> IntentCodec() to content.v1
                    is StandardContent.Reply -> ReplyCodec() to content
                    is StandardContent.GroupUpdated -> GroupUpdatedCodec() to content.v1
                    is StandardContent.DeleteMessage -> DeleteMessageCodec() to content
                    is StandardContent.LeaveRequest -> LeaveRequestCodec() to content.v1
                }
            val encoded = codec.encode(value)
            check(runCatching { codec.encode(Any()) }.exceptionOrNull() is XmtpException.InvalidArgument) {
                "${codec.javaClass.simpleName} did not reject a wrong value with InvalidArgument"
            }
            check(sameEncoded(encoded, sample.expected)) { "standard codec content differs from Rust" }
            check(sameEncoded(codec.encode(codec.decode(encoded)), sample.expected))
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
                    override fun log(record: LogRecord): Unit = throw AssertionError("host failure")
                },
            )
        check(
            runCatching {
                failingSink.log(LogRecord(LogLevel.ERROR, "test", "message", emptyMap(), 0, 0uL))
            }.exceptionOrNull() is LogSinkException.Failed,
        )
        val cancellingSink =
            SDKForeign.logSink(
                object : LogSink {
                    override fun log(record: LogRecord): Unit = throw CancellationException("x")
                },
            )
        check(
            runCatching {
                cancellingSink.log(LogRecord(LogLevel.ERROR, "test", "message", emptyMap(), 0, 0uL))
            }.exceptionOrNull() is LogSinkException.Failed,
        )
        println("Kotlin P37 foreign trait wrappers passed")

        val signer = TestSigner()
        val directory = Files.createTempDirectory("xmtp-sdk-conformance-")
        val backendOptions = BackendOptions(url = checkNotNull(System.getenv("XMTP_BACKEND_URL")))
        check(ClientOptions(storage = StorageOptions(location = StorageLocation.InMemory)).backend == null)
        val options =
            ClientOptions(
                backend = BackendSource.Options(backendOptions),
                storage = StorageOptions(location = StorageLocation.Directory(directory.toString())),
                deviceSync = false,
            )
        val host = SDKClient.create(signer, options)
        val client = host.raw
        val inboxID = client.inboxID()
        val storagePath = checkNotNull(host.storage().path())
        check(Files.isRegularFile(Path.of(storagePath))) { "storage path does not name the database file" }
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
                            value.referenceInboxID,
                            value.reaction,
                            null,
                        )
                    }

                    is StandardContent.Reply -> {
                        group.sendReply(
                            value.reference,
                            value.referenceInboxID,
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
            val wire = checkNotNull(client.conversations().getMessageByID(id))
            check(sameEncoded(wire.encoded, sample.expected)) { "typed send content differs from codec" }
            typedSends++
        }
        check(typedSends == 12)
        println("Kotlin P69: typed send bytes match all 12 public codecs")
        val sentID = group.sendText("conformance message", null)
        val sent = group.messages(null).first { it.id == sentID }
        check(sent.client() === host)
        host.end()
        check(runCatching { sent.client() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(runCatching { sent.refresh() }.exceptionOrNull() is XmtpException.ClientClosed)
        check(Message(sent.data.copy(clientKey = sent.data.clientKey + 1uL)) != sent)
        val reopenedHost = SDKClient.build(signer.identity(), options, inboxID)
        val reopened = reopenedHost.raw
        check(reopened.inboxID() == inboxID)
        val defaultDirectory = Files.createTempDirectory("xmtp-sdk-default-")
        val defaultClient =
            SDKClient.build(
                signer.identity(),
                options.copy(storage = options.storage.copy(location = StorageLocation.Default)),
                inboxID,
                defaultDirectory = defaultDirectory.toString(),
            )
        check(Files.list(defaultDirectory).use { paths -> paths.anyMatch { it.fileName.toString().endsWith(".db3") } })
        defaultClient.end()
        val (orphan, weak) = releasedMessage(signer.identity(), options, inboxID)
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
        val liveID = liveGroup.sendText("durable stream", null)
        check(reader.next()?.id == liveID)
        reader.end()
        val replay = liveGroup.messageReader()
        check(replay.next()?.id == liveID)
        val pending = async { replay.next() }
        delay(50)
        pending.cancel()
        replay.end()
        runCatching { pending.await() }
        val adapterID = liveGroup.sendText("adapter stream", null)
        var delivered = false
        try {
            withTimeout(10_000) {
                reopenedHost.messages(liveGroup).collect { message ->
                    check(message.id == adapterID)
                    delivered = true
                }
            }
        } catch (_: TimeoutCancellationException) {
            check(delivered)
        }
        val protocolGroup = reopened.conversations().createGroup(emptyList(), null)
        val firstID = protocolGroup.sendText("ack on request", null)
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(1)
                .toList()
                .single()
                .id == firstID,
        )
        val reread = protocolGroup.messageReader()
        check(withTimeout(3_000) { reread.next() }?.id == firstID) {
            "adapter prefetched and acknowledged a value"
        }
        reread.end()
        val secondID = protocolGroup.sendText("second request", null)
        check(
            reopenedHost
                .messages(protocolGroup)
                .take(2)
                .toList()
                .map { it.id } == listOf(firstID, secondID),
        )
        val afterAck = protocolGroup.messageReader()
        check(afterAck.next()?.id == secondID) { "adapter did not acknowledge on next request" }
        afterAck.end()
        val openedReader = CompletableDeferred<MessageReader>()
        val releaseOpening = CompletableDeferred<Unit>()
        SDKClient.readerOpenedForTest = { opened ->
            openedReader.complete(opened)
            releaseOpening.await()
        }
        val cancelledOpening =
            async(start = CoroutineStart.UNDISPATCHED) {
                reopenedHost.messages(protocolGroup).collect {}
            }
        val lateReader = withTimeout(10_000) { openedReader.await() }
        cancelledOpening.cancel(CancellationException("cancel during reader creation"))
        releaseOpening.complete(Unit)
        withTimeout(10_000) { cancelledOpening.join() }
        SDKClient.readerOpenedForTest = null
        check(withTimeout(10_000) { lateReader.next() } == null) { "late reader was not ended" }
        val reopenedReader = protocolGroup.messageReader()
        reopenedReader.end()
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
                storage = StorageOptions(location = StorageLocation.InMemory),
            )
        val credentialHost = SDKClient.build(signer.identity(), credentialOptions, inboxID)
        check(
            credentialHost.raw
                .options()
                .backend
                .let { it as BackendSource.Options }
                .options.credential
                ?.expiresAtSeconds == largeExpiry,
        ) {
            "credential expiry lost 64-bit precision"
        }
        credentialHost.raw.setCredential(Credential(null, "Bearer renewed", largeExpiry))
        credentialHost.end()
        println("Kotlin scenario 3: credential update and 64-bit value passed")

        val snapshot = reopened.serverConfiguration()
        val fetched = fetchServerConfiguration(BackendSource.Options(backendOptions))
        val staticBackend = Backend.connect(backendOptions)
        check(SDKClient.inboxIDFor(signer.identity(), BackendSource.Connected(staticBackend)) == inboxID)
        check(
            SDKClient.canMessage(listOf(signer.identity()), BackendSource.Connected(staticBackend)).first().canMessage,
        )
        check(SDKClient.canMessage(listOf(signer.identity()), BackendSource.Options(backendOptions)).first().canMessage)
        SDKClient
            .build(
                signer.identity(),
                options.copy(
                    backend = BackendSource.Connected(staticBackend),
                    storage = StorageOptions(location = StorageLocation.InMemory),
                ),
                inboxID,
            ).end()
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
        check(!unsignedHost.raw.isRegistered())
        val request = checkNotNull(unsignedHost.raw.unsafeCreateInboxSignatureRequest())
        check(request.signatureText().isNotEmpty())
        request.sign(local)
        unsignedHost.raw.unsafeApplySignatureRequest(request)
        check(unsignedHost.raw.isRegistered())
        unsignedHost.end()
        println("Kotlin scenario 11: local signer and signature request passed")

        check(reopened.notificationState() == NotificationState.Disabled)
        check(
            runCatching {
                reopened.enableNotifications(
                    NotificationConfig(channel = NotificationChannel.Http("https://example.com", byteArrayOf(1))),
                )
            }.exceptionOrNull() is XmtpException.InvalidArgument,
        )
        println("Kotlin scenario 12: notification state and typed error passed")

        initLogging(LoggingOptions(level = LogLevel.ERROR))
        val orderedSink = OrderedLogSink()
        setLogSink(orderedSink)
        sdkConformanceEmit(32u)
        check(orderedSink.sequence == (0 until 32).map(Int::toString)) { "inline log sink changed record order" }
        for (failure in listOf<Throwable>(Error("foreign log sink failed"), Exception("foreign log sink failed"))) {
            var throwingSinkCalled = false
            val before = sdkConformanceSinkErrorCount()
            setLogSink(
                object : LogSink {
                    override fun log(record: LogRecord) {
                        throwingSinkCalled = true
                        throw failure
                    }
                },
            )
            sdkConformanceEmit(1u)
            check(throwingSinkCalled) { "foreign log sink was not called" }
            check(sdkConformanceSinkErrorCount() == before + 1uL) { "Rust did not observe the foreign sink error" }
        }
        clearLogSink()
        check(sdkVersion().startsWith("1.12.0"))
        println("Kotlin logging: ordered records and throwing foreign sink passed")

        val fresh = generateLocalSigner()
        val errorSigner =
            object : Signer {
                override suspend fun identity() = fresh.identity()

                override suspend fun kind() = fresh.kind()

                override suspend fun sign(request: SigningRequest): Signature = throw Error("signer failed")
            }
        val errorHost = SDKClient.create(errorSigner, unsignedOptions)
        check(
            withTimeout(10_000) { runCatching { errorHost.raw.register() }.exceptionOrNull() } is XmtpException.Signer,
        )
        errorHost.end()
        val unsignedErrorHost = SDKClient.create(generateLocalSigner(), unsignedOptions)
        val errorRequest = checkNotNull(unsignedErrorHost.raw.unsafeCreateInboxSignatureRequest())
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
                    SDKClient.build(
                        signer.identity(),
                        options.copy(
                            backend =
                                BackendSource.Options(
                                    BackendOptions(url = backendOptions.url, credentials = failingSource),
                                ),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                        ),
                        inboxID,
                    )
                }.exceptionOrNull()
            }
        check(
            failedCredential is XmtpException.CredentialCallbackFailed,
        ) { "credential Error became $failedCredential" }
        println("Kotlin signer Error: call failed without a hang")

        val family = reopened.conversations().createGroup(emptyList(), CreateGroupOptions(name = "family group"))
        check(family.state().name == "family group")
        check(family.creatorInboxID() == inboxID)
        check(reopened.conversations().listGroups(null).any { it.id() == family.id() })
        println("Kotlin scenario 4: group options, state, and list passed")

        val parentID = family.sendText("parent", null)
        val reactionID =
            reopened.conversations().reactToMessage(
                parentID,
                Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
                null,
            )
        val replyID = reopened.conversations().replyToMessage(parentID, encodeText("reply"), null)
        check(reopened.decodeContent(encodeText("decoded")) is MessageContent.Text)
        val familyMessages = family.messages(null)
        val parent = familyMessages.first { it.id == parentID }
        val reply = familyMessages.first { it.id == replyID }
        check(parent.replyCount == 1uL && parent.reactions.firstOrNull()?.id == reactionID)
        check(reply.inReplyTo?.id == parentID)
        val changedEnvelope =
            Message(parent.data.copy(encoded = parent.encoded.copy(parameters = mapOf("key" to "different"))))
        check(parent != changedEnvelope) { "EncodedContent parameters must affect message equality" }
        val copiedBytes =
            Message(parent.data.copy(encoded = parent.encoded.copy(content = parent.encoded.content.copyOf())))
        check(parent == copiedBytes && parent.hashCode() == copiedBytes.hashCode())
        val sameParent = checkNotNull(reopened.conversations().getMessageByID(parentID))
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
        val withCodec = SDKClient.build(signer.identity(), options, inboxID, codecs = listOf(codec))
        val withoutCodec = SDKClient.build(signer.identity(), options, inboxID)
        val customID = family.send(codec.encode("codec value"), null)
        val decoded = checkNotNull(withCodec.raw.conversations().getMessageByID(customID))
        val undecoded = checkNotNull(withoutCodec.raw.conversations().getMessageByID(customID))
        check((decoded.content as? SDKMessageContent.Custom)?.value == "codec value")
        check(undecoded.content is SDKMessageContent.Unknown)
        val customReplyID =
            withCodec.raw.conversations().replyToMessage(
                customID,
                codec.encode("reply codec value"),
                null,
            )
        val customReply = checkNotNull(withCodec.raw.conversations().getMessageByID(customReplyID))
        check((customReply.replyContent as? SDKReplyContent.Custom)?.value == "reply codec value") {
            "reply body custom codec did not run"
        }
        val failingHost = SDKClient.build(signer.identity(), options, inboxID, codecs = listOf(FailingCodec()))
        val failed = checkNotNull(failingHost.raw.conversations().getMessageByID(customID))
        check((failed.content as? SDKMessageContent.Custom)?.error is AssertionError)
        failingHost.end()
        println("Kotlin codec_scoped_to_client passed")
        withCodec.end()
        withoutCodec.end()
        println("Kotlin scenario 6: custom codec stayed with its client")

        val archive = reopened.archives().exportToBytes(ByteArray(32) { 7 }, null)
        check(archive.isNotEmpty())
        check(reopened.archives().metadataFromBytes(archive, ByteArray(32) { 7 }).backupVersion == 0u.toUShort())
        val archiveFolder = Files.createTempDirectory("xmtp-sdk-archive-")
        val archivePath = archiveFolder.resolve("snapshot.xmtp")
        try {
            reopened.archives().exportToFile(archivePath.toString(), ByteArray(32) { 7 }, null)
            check(
                reopened.archives().metadataFromFile(archivePath.toString(), ByteArray(32) { 7 }).backupVersion ==
                    0u.toUShort(),
            )
        } finally {
            Files.deleteIfExists(archivePath)
            Files.deleteIfExists(archiveFolder)
        }
        println("Kotlin scenario 9: archive bytes and file passed")

        reopenedHost.end()
    }
