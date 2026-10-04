import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger

/** A client whose files live under `root`, allowed to reach loopback storage. */
private fun fileOptions(
    backend: BackendOptions,
    root: Path,
    attachments: AttachmentOptions = AttachmentOptions(allowPrivateNetwork = true),
) = ClientOptions(
    backend = BackendSource.Options(backend),
    storage = StorageOptions(location = StorageLocation.Directory(root.toString())),
    deviceSync = false,
    attachments = attachments,
)

private fun bytesSource(text: String) = AttachmentSource.Bytes(text.toByteArray(), "note.txt", "text/plain")

private fun attachmentsDir(databasePath: String?): Path =
    Path.of(checkNotNull(databasePath)).parent.resolve("attachments")

private fun readText(path: String) = Files.readString(Path.of(path))

/** Storage and server configuration fields, including 64-bit values. */
suspend fun checkAttachmentSettings(backend: BackendOptions) {
    val offered =
        AttachmentsConfiguration(
            baseUrl = System.getenv("XMTP_S3_BASE_URL") ?: "http://127.0.0.1:9067/attachments",
            maxUploadBytes = 104_857_600uL,
            retentionSeconds = 0uL,
        )
    check(SDKClient.fetchServerConfiguration(BackendSource.Options(backend)).attachments == offered)
    val root = Files.createTempDirectory("xmtp-sdk-atch-").toRealPath()
    val large = ULong.MAX_VALUE - 1uL
    val settings = AttachmentOptions(large, large - 2uL, true)
    val client = SDKClient.create(generateLocalSigner(), fileOptions(backend, root, settings))
    check(client.serverConfiguration().attachments == offered)
    check(client.options().attachments == settings)
    check(client.attachments().offered())
    client.end()
    root.toFile().deleteRecursively()
    println("Kotlin attachments: configuration and 64-bit options")
}

/** Create, send, upload, reopen, resume, and download on another client. */
suspend fun checkAttachmentFlow(backend: BackendOptions) =
    coroutineScope {
        val root = Files.createTempDirectory("xmtp-sdk-atch-").toRealPath()
        val signer = generateLocalSigner()
        val senderOptions = fileOptions(backend, root.resolve("sender"))
        val sender = SDKClient.create(signer, senderOptions)
        val receiver = SDKClient.create(generateLocalSigner(), fileOptions(backend, root.resolve("receiver")))
        val attachments = sender.attachments()
        val events = EventQueue.open(this, sender)
        // The receiver's reader sees none of the sender's events.
        val receiverEvents = EventQueue.open(this, receiver)

        val content = "attachment bytes"
        val fromBytes = attachments.create(bytesSource(content))
        val remote = fromBytes.remoteAttachment()
        check(fromBytes.status() == PendingAttachmentStatus.Waiting)
        check(readText(fromBytes.localPath()) == content)
        check(attachments.localPath(remote) == fromBytes.localPath())
        // The SDK copies a path source at create, so moving it changes nothing.
        val source = root.resolve("photo.bin")
        Files.writeString(source, "path bytes")
        val fromPath = attachments.create(AttachmentSource.Path(source.toString(), null, "application/octet-stream"))
        Files.move(source, root.resolve("moved.bin"))
        val pathRemote = fromPath.remoteAttachment()
        check(readText(fromPath.localPath()) == "path bytes")
        check(events.drain(sender).isEmpty())

        // The record is complete before any upload, so the app sends it first.
        val dm = sender.conversations().createDm(receiver.inboxId())
        val sent = dm.sendRemoteAttachment(remote)
        // Concurrent uploads of one attachment share one transfer.
        listOf(async { fromBytes.upload() }, async { fromBytes.upload() }).awaitAll()
        check(fromBytes.status() == PendingAttachmentStatus.Complete)
        val uploaded = events.drain(sender)
        check(
            uploaded.map { it.kind } ==
                listOf(EventKind.ATTACHMENT_UPLOAD_STARTED, EventKind.ATTACHMENT_UPLOAD_COMPLETED),
        ) { "expected one shared upload, got $uploaded" }
        check(uploaded[0].copy(kind = uploaded[1].kind) == uploaded[1])
        check(uploaded[0].url == remote.url && uploaded[0].contentDigest == remote.contentDigest)
        check(receiverEvents.drain(receiver).isEmpty())
        events.end()
        sender.end()
        // Held values stay readable after end; calls fail closed.
        check(attachments.offered())
        check(fromBytes.remoteAttachment() == remote)
        checkClientClosed { fromPath.status() }
        checkClientClosed { attachments.listPending() }

        // A reopened client lists the upload it did not finish and resumes it.
        val reopened = SDKClient.build(signer.identity(), senderOptions)
        val resuming = reopened.attachments()
        val listed = resuming.listPending()
        check(listed.size == 1) { "expected one pending upload, got ${listed.size}" }
        check(listed[0].remoteAttachment().contentDigest == pathRemote.contentDigest)
        check(resuming.pending(remote).status() == PendingAttachmentStatus.Complete)
        val resumed = resuming.pending(pathRemote)
        check(resumed.status() == PendingAttachmentStatus.Waiting)
        resumed.upload()
        check(resumed.status() == PendingAttachmentStatus.Complete)
        check(resuming.listPending().isEmpty())
        check(
            thrownFailure { resuming.pending(pathRemote.copy(contentDigest = "00".repeat(32))) } ==
                failure(AttachmentFailureCause.STAGED_UNUSABLE),
        )

        // The receiver derives the path of the record it was sent without a request.
        receiver.conversations().syncAll(null)
        val message = receiver.conversations().getMessageById(sent)
        val received =
            checkNotNull(
                ((message?.content as? SDKMessageContent.Standard)?.value as? MessageContent.RemoteAttachment)?.v1,
            ) { "the sent attachment did not arrive as a remote attachment" }
        val receiving = receiver.attachments()
        val directory = attachmentsDir(receiver.storage().path())
        ServedFile(200, ByteArray(0)).use { probe ->
            val derived = receiving.localPath(received.copy(url = probe.url))
            check(probe.requests() == 0) { "path derivation sent a request" }
            check(Path.of(derived).startsWith(directory) && Path.of(derived) != directory)
        }
        val expectedPath = receiving.localPath(received)
        check(!Files.exists(Path.of(expectedPath)))

        // Download events arrive in order; a filtered reader sees only its kind.
        val deletedOnly = EventQueue.open(this, receiver, attachmentFilter(listOf(EventKind.ATTACHMENT_DELETED)))
        val downloaded = receiving.download(received)
        check(downloaded == DownloadedAttachment(expectedPath, "text/plain", "note.txt")) { "downloaded $downloaded" }
        check(readText(downloaded.path) == content)
        val pathDownload = receiving.download(pathRemote)
        check(readText(pathDownload.path) == "path bytes")
        check(
            receiving.listLocal().map { it.path }.sorted() ==
                listOf(
                    downloaded.path,
                    pathDownload.path,
                ).map { directory.relativize(Path.of(it)).toString() }.sorted(),
        )
        receiving.deleteLocal(received)
        check(receiving.listLocal().size == 1)
        check(!Files.exists(Path.of(downloaded.path)))
        val downloads = receiverEvents.drain(receiver)
        check(
            downloads.map { it.kind to it.contentDigest } ==
                listOf(
                    EventKind.ATTACHMENT_DOWNLOAD_STARTED to received.contentDigest,
                    EventKind.ATTACHMENT_DOWNLOAD_COMPLETED to received.contentDigest,
                    EventKind.ATTACHMENT_DOWNLOAD_STARTED to pathRemote.contentDigest,
                    EventKind.ATTACHMENT_DOWNLOAD_COMPLETED to pathRemote.contentDigest,
                    EventKind.ATTACHMENT_DELETED to received.contentDigest,
                ),
        ) { "download events $downloads" }
        check(downloads[0].url == received.url)
        check(downloads[0].attachmentKey != downloads[2].attachmentKey)
        check(downloads[4].copy(kind = downloads[0].kind) == downloads[0])
        check(deletedOnly.drain(receiver) == listOf(downloads[4]))
        deletedOnly.end()
        receiverEvents.end()
        reopened.end()
        receiver.end()
        root.toFile().deleteRecursively()
        println("Kotlin attachments: upload, reopen, resume, download, and delete")
    }

/** Failed uploads and downloads carry one record in errors and status. */
suspend fun checkAttachmentFailures(backend: BackendOptions) =
    coroutineScope {
        val root = Files.createTempDirectory("xmtp-sdk-atch-").toRealPath()
        val client = SDKClient.create(generateLocalSigner(), fileOptions(backend, root))
        val attachments = client.attachments()
        val events = EventQueue.open(this, client)
        val staged = attachmentsDir(client.storage().path()).resolve(".staged")

        // Missing staged data fails the upload before any request.
        val pending = attachments.create(bytesSource("staged"))
        val remote = pending.remoteAttachment()
        val ciphertext = Files.readAllBytes(staged.resolve(remote.contentDigest))
        Files.delete(staged.resolve(remote.contentDigest))
        val other = attachments.create(bytesSource("other"))
        val otherRemote = other.remoteAttachment()
        val otherCiphertext = Files.readAllBytes(staged.resolve(otherRemote.contentDigest))
        val thrown = thrownFailure { pending.upload() }
        check(thrown == failure(AttachmentFailureCause.STAGED_UNUSABLE)) { "thrown $thrown" }
        check(pending.status() == PendingAttachmentStatus.Failed(thrown))
        val failed = events.drain(client)
        check(
            failed.map { it.kind } ==
                listOf(EventKind.ATTACHMENT_UPLOAD_STARTED, EventKind.ATTACHMENT_UPLOAD_FAILED),
        ) { "upload events $failed" }
        check(
            failed[1] ==
                failed[0].copy(
                    kind = EventKind.ATTACHMENT_UPLOAD_FAILED,
                    cause = AttachmentFailureCause.STAGED_UNUSABLE,
                ),
        )
        check(failed[1].contentDigest == remote.contentDigest)
        // A source the SDK cannot read fails create.
        val missing = AttachmentSource.Path(root.resolve("missing.bin").toString(), null, "application/octet-stream")
        check(thrownFailure { attachments.create(missing) }.cause == AttachmentFailureCause.SOURCE_UNREADABLE)
        events.end()

        // The creating client holds the plaintext, so another client downloads.
        val downloader = SDKClient.create(generateLocalSigner(), fileOptions(backend, root.resolve("downloader")))
        val downloads = downloader.attachments()
        val downloadEvents = EventQueue.open(this, downloader)
        ServedFile(503, ByteArray(0)).use { unavailable ->
            val error = thrownAttachment { downloads.download(remote.copy(url = unavailable.url)) }
            check(error.v2 == failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 503u)) { "failure ${error.v2}" }
            check(error.v1.category == ErrorCategory.NETWORK)
            check(error.v1.retryable)
            check(unavailable.requests() == 1) { "the SDK does not retry" }
        }
        // Another attachment's object decrypts and decodes, so only its digest
        // differs from the record.
        ServedFile(200, otherCiphertext).use { substituted ->
            val substitute = otherRemote.copy(url = substituted.url, contentDigest = remote.contentDigest)
            check(thrownFailure { downloads.download(substitute) }.cause == AttachmentFailureCause.DIGEST_MISMATCH)
        }
        // A changed tag byte with a matching digest fails only the decryption.
        val tamperedBytes = ciphertext.copyOf()
        tamperedBytes[tamperedBytes.size - 1] = (tamperedBytes.last().toInt() xor 1).toByte()
        ServedFile(200, tamperedBytes).use { tampered ->
            val digest = sha256Hex(tamperedBytes)
            check(
                thrownFailure { downloads.download(remote.copy(url = tampered.url, contentDigest = digest)) }.cause ==
                    AttachmentFailureCause.DECRYPTION_FAILED,
            )
            check(
                thrownFailure { downloads.download(remote.copy(url = tampered.url, secret = byteArrayOf(7, 7, 7))) }
                    .cause == AttachmentFailureCause.MALFORMED,
            )
        }
        val downloadFailures = downloadEvents.drain(downloader)
        check(
            downloadFailures.filter { it.kind == EventKind.ATTACHMENT_DOWNLOAD_FAILED }.map { it.cause } ==
                listOf(
                    AttachmentFailureCause.HTTP_STATUS,
                    AttachmentFailureCause.DIGEST_MISMATCH,
                    AttachmentFailureCause.DECRYPTION_FAILED,
                ),
        ) { "download events $downloadFailures" }
        downloadEvents.end()
        downloader.end()
        client.end()
        root.toFile().deleteRecursively()
        println("Kotlin attachments: real failures carry one record")
    }

// Every transport discriminant and optional field. Rust owns the full policy table.
private val failureTable: List<AttachmentFailure> =
    listOf(
        failure(AttachmentFailureCause.NOT_OFFERED),
        failure(AttachmentFailureCause.TOO_LARGE),
        failure(AttachmentFailureCause.SOURCE_UNREADABLE),
        failure(AttachmentFailureCause.LOCAL_STORAGE),
        failure(AttachmentFailureCause.STAGED_UNUSABLE),
        failure(AttachmentFailureCause.CONNECTION_BLOCKED),
        failure(
            AttachmentFailureCause.CREDENTIAL,
            credentialKind = CredentialFailureKind.CREDENTIAL_REJECTED,
            missingScope = true,
        ),
        failure(
            AttachmentFailureCause.CREDENTIAL,
            credentialKind = CredentialFailureKind.CALLBACK_FAILED,
            retryable = true,
        ),
        failure(AttachmentFailureCause.CREDENTIAL, credentialKind = CredentialFailureKind.EXHAUSTED),
        failure(AttachmentFailureCause.CREDENTIAL, credentialKind = CredentialFailureKind.MISSING_CREDENTIAL),
        failure(AttachmentFailureCause.BACKEND_REJECTED),
        failure(AttachmentFailureCause.BACKEND_UNAVAILABLE),
        failure(AttachmentFailureCause.TARGET_REJECTED, httpStatus = 403u),
        failure(AttachmentFailureCause.NETWORK),
        failure(AttachmentFailureCause.INSECURE_URL),
        failure(AttachmentFailureCause.BLOCKED_ADDRESS),
        failure(AttachmentFailureCause.TOO_MANY_REDIRECTS),
        failure(AttachmentFailureCause.NOT_FOUND, httpStatus = 404u),
        failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 408u),
        failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 429u),
        failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 503u),
        failure(AttachmentFailureCause.HTTP_STATUS, httpStatus = 403u),
        failure(AttachmentFailureCause.MALFORMED),
        failure(AttachmentFailureCause.DIGEST_MISMATCH),
        failure(AttachmentFailureCause.DECRYPTION_FAILED),
        failure(AttachmentFailureCause.NOT_AN_ATTACHMENT),
        failure(AttachmentFailureCause.DELETED),
    )

/** Every cause and credential kind, thrown and recorded, and no resend. */
suspend fun checkAttachmentRecords(backend: BackendOptions) =
    coroutineScope {
        val root = Files.createTempDirectory("xmtp-sdk-atch-").toRealPath()
        val held = HeldTransfer.open()
        val client = SDKClient.create(generateLocalSigner(), fileOptions(backend.copy(url = held.backend), root))
        val attachments = client.attachments()
        for ((index, recorded) in failureTable.withIndex()) {
            val error = thrownAttachment { sdkConformanceAttachmentError(recorded) }
            check(error.v2 == recorded) { "thrown ${error.v2}" }
            if (recorded.cause == AttachmentFailureCause.CREDENTIAL) {
                check(error.v1.category == ErrorCategory.CALLBACK)
                check(error.v1.retryable == recorded.retryable)
            }
            val pending = attachments.create(bytesSource("record $index"))
            pending.sdkConformanceFail(recorded)
            check(pending.status() == PendingAttachmentStatus.Failed(recorded)) { "status ${pending.status()}" }
        }
        check(failureTable.map { it.cause }.toSet() == AttachmentFailureCause.entries.toSet())
        check(failureTable.mapNotNull { it.credentialKind }.toSet() == CredentialFailureKind.entries.toSet())

        // A terminal backend rejection is not sent again.
        held.command("release")
        val events = EventQueue.open(this, client)
        val rejected = attachments.create(bytesSource("rejected"))
        rejected.sdkConformanceFail(failure(AttachmentFailureCause.BACKEND_REJECTED))
        repeat(2) {
            check(thrownFailure { rejected.upload() } == failure(AttachmentFailureCause.BACKEND_REJECTED))
        }
        check(rejected.status() == PendingAttachmentStatus.Failed(failure(AttachmentFailureCause.BACKEND_REJECTED)))
        check(events.drain(client).isEmpty())
        held.checkCounts(0, 0)
        events.end()
        client.end()
        root.toFile().deleteRecursively()
        println("Kotlin attachments: every cause and credential kind in both forms")
    }

/** End waits for an operation in flight; later calls fail closed. */
suspend fun checkAttachmentEnd(backend: BackendOptions) =
    coroutineScope {
        val root = Files.createTempDirectory("xmtp-sdk-atch-").toRealPath()
        val signer = generateLocalSigner()
        val held = HeldTransfer.open()
        val options = fileOptions(backend.copy(url = held.backend), root)
        val client = SDKClient.create(signer, options)
        var reopenedForCleanup: SDKClient? = null
        try {
            val attachments = client.attachments()
            val small = attachments.create(bytesSource("small"))
            val events = EventQueue.open(this, client, attachmentFilter(listOf(EventKind.ATTACHMENT_UPLOAD_STARTED)))
            val large = attachments.create(bytesSource("held upload"))
            val upload = async { large.upload() }
            held.command("entered")
            check(events.next() is ClientEvent.AttachmentUploadStarted)
            // Cancel the real coroutine waiting on the generated binding call.
            // The detached SDK task must retain storage until the PUT settles.
            upload.cancelAndJoin()
            check(upload.isCancelled)
            val ending = async { client.end() }
            check(events.ended()) { "the event reader outlived end" }
            check(!ending.isCompleted) { "end released storage while PUT was held" }
            held.checkCounts(1, 1)
            held.command("release")
            withTimeout(10_000) { ending.await() }
            // Held values stay readable; calls fail closed.
            check(attachments.offered())
            val remote = large.remoteAttachment()
            checkClientClosed { attachments.create(bytesSource("late")) }
            checkClientClosed { attachments.localPath(remote) }
            checkClientClosed { attachments.listLocal() }
            checkClientClosed { attachments.download(remote) }
            checkClientClosed { attachments.pending(remote) }
            checkClientClosed { attachments.listPending() }
            checkClientClosed { attachments.deleteLocal(remote) }
            checkClientClosed { large.localPath() }
            checkClientClosed { large.status() }
            checkClientClosed { large.upload() }

            // The held wrappers do not keep the ended database open.
            val reopened = SDKClient.build(signer.identity(), options)
            reopenedForCleanup = reopened
            val resumed = reopened.attachments()
            check(resumed.pending(remote).status() == PendingAttachmentStatus.Complete)
            held.checkCounts(1, 1)
            // A listener sees a deletion until it stops.
            val first = CompletableDeferred<Unit>()
            val deletions = AtomicInteger()
            val listener =
                reopened.startListener(
                    EventFilter(listOf(EventKind.ATTACHMENT_DELETED), null, null, false),
                ) {
                    deletions.incrementAndGet()
                    first.complete(Unit)
                }
            val deleted = EventQueue.open(this, reopened, attachmentFilter(listOf(EventKind.ATTACHMENT_DELETED)))
            resumed.deleteLocal(remote)
            withTimeout(10_000) { first.await() }
            check(deletions.get() == 1)
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val finished = CompletableDeferred<Unit>()
            EventStartHookForTest.beforeCallback = {
                entered.complete(Unit)
                release.await()
            }
            EventStartHookForTest.afterCallback = { finished.complete(Unit) }
            try {
                resumed.deleteLocal(small.remoteAttachment())
                withTimeout(10_000) { entered.await() }
                reopened.stopListener(listener)
                release.complete(Unit)
                withTimeout(10_000) { finished.await() }
                check(deletions.get() == 1) { "a stopped listener saw a deletion" }
            } finally {
                release.complete(Unit)
                EventStartHookForTest.beforeCallback = null
                EventStartHookForTest.afterCallback = null
            }
            check(!Files.exists(Path.of(resumed.localPath(remote))))
            // The reader has both deletions, so a live listener had its turn.
            for (expected in listOf(remote, small.remoteAttachment())) {
                val next = deleted.next()
                check(
                    next is ClientEvent.AttachmentDeleted && next.attachmentDeleted.url == expected.url,
                ) { "not a deletion: $next" }
            }
            check(deletions.get() == 1) { "a stopped listener saw a deletion" }
            deleted.end()
            reopened.end()
            root.toFile().deleteRecursively()
            println("Kotlin attachments: end waits for an upload; calls fail closed")
        } finally {
            kotlinx.coroutines.withContext(kotlinx.coroutines.NonCancellable) {
                held.command("release")
                withTimeout(10_000) {
                    reopenedForCleanup?.end()
                    client.end()
                }
            }
        }
    }
