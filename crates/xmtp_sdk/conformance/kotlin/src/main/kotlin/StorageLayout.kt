import uniffi.xmtp_sdk.*
import java.nio.file.Files

private fun isStorageLocation(error: Throwable?): Boolean =
    error is XmtpException.StorageLocation && error.v1.category == ErrorCategory.STORAGE && !error.v1.retryable

private fun directoryOptions(
    url: String,
    directory: String,
    label: String? = null,
) = ClientOptions(
    backend = BackendSource.Options(BackendOptions(url = url)),
    storage = StorageOptions(location = StorageLocation.Directory(directory), label = label),
    deviceSync = false,
)

/** Storage locations open the layouts they name, offline from their record. */
suspend fun checkStorageLayout(backend: BackendOptions) {
    val root = Files.createTempDirectory("xmtp-sdk-layout-").toRealPath()
    // A labelled directory holds the store at its deployment path.
    var relay = CountingRelay(backend.url)
    val signer = generateLocalSigner()
    val identity = signer.identity()
    val online = SDKClient.create(signer, directoryOptions(relay.url, root.toString(), "phone"))
    val inboxId = online.inboxId()
    val expected =
        root
            .resolve("phone")
            .resolve(deploymentComponent(online.serverConfiguration().identifier))
            .resolve(inboxId)
            .resolve("xmtp.db3")
    check(online.storage().path() == expected.toString())
    check(Files.isRegularFile(expected))
    online.end()

    relay.refuse()
    val offline =
        SDKClient.build(
            identity,
            directoryOptions(relay.url, root.toString(), "phone").copy(allowOffline = true),
            inboxId,
        )
    check(relay.connections() == 0) { "offline build sent a request" }
    check(offline.inboxId() == inboxId)
    check(offline.storage().path() == expected.toString())
    offline.end()
    // The unlabelled root records no deployment, so an offline first start
    // fails before any request.
    val unrecorded =
        runCatching {
            SDKClient.build(identity, directoryOptions(relay.url, root.toString()).copy(allowOffline = true), inboxId)
        }.exceptionOrNull()
    check(isStorageLocation(unrecorded)) { "expected a storage location error, got $unrecorded" }
    check(relay.connections() == 0) { "offline first start sent a request" }
    println("Kotlin storage layout: a labelled directory reopens offline")

    // Unsafe labels fail before any path or request.
    val unsafeRoot = root.resolve("unsafe")
    for (label in listOf(".", "..", "bad/name", "bad\\name", "bad:name", "a\u0000b")) {
        val error =
            runCatching {
                SDKClient.create(signer, directoryOptions(relay.url, unsafeRoot.toString(), label))
            }.exceptionOrNull()
        check(isStorageLocation(error)) { "label $label: expected a storage location error, got $error" }
    }
    check(relay.connections() == 0) { "an unsafe label sent a request" }
    check(!Files.exists(unsafeRoot))
    relay.close()
    println("Kotlin storage layout: unsafe labels fail before any path or request")

    // An explicit location opens the file the app chose.
    relay = CountingRelay(backend.url)
    val explicitSigner = generateLocalSigner()
    val dbPath = root.resolve("chosen.sqlite").toString()
    val explicit =
        ClientOptions(
            backend = BackendSource.Options(BackendOptions(url = relay.url)),
            storage =
                StorageOptions(
                    location = StorageLocation.Explicit(dbPath, root.resolve("files").toString()),
                ),
            deviceSync = false,
        )
    val chosen = SDKClient.create(explicitSigner, explicit)
    val chosenInbox = chosen.inboxId()
    check(chosen.storage().path() == dbPath)
    chosen.end()
    relay.refuse()
    val reopened = SDKClient.build(explicitSigner.identity(), explicit.copy(allowOffline = true))
    check(relay.connections() == 0) { "offline reopen sent a request" }
    check(reopened.inboxId() == chosenInbox)
    check(reopened.storage().path() == dbPath)
    reopened.end()
    relay.close()
    root.toFile().deleteRecursively()
    println("Kotlin storage layout: an explicit location reopens offline")
}
