import android.content.Context
import uniffi.xmtp_sdk.*
import java.io.File

// End the old SDK client before calling this function.
suspend fun exerciseMigration(
    signer: Signer,
    options: ClientOptions,
    dbPath: String,
    attachmentsDir: String,
) {
    val expectedPath = File(dbPath).absoluteFile.normalize().path
    val explicit =
        options.copy(
            storage =
                options.storage.copy(
                    location = StorageLocation.Explicit(dbPath, attachmentsDir),
                ),
        )
    val first = SDKClient.create(signer, explicit)
    val identity = first.identity()
    val inboxId = first.inboxId()
    val openedPath = first.storage().path()
    first.end()
    check(File(checkNotNull(openedPath)).absoluteFile.normalize().path == expectedPath) {
        "The SDK opened another database"
    }

    val reopened = SDKClient.build(identity, explicit.copy(allowOffline = true))
    try {
        check(reopened.inboxId() == inboxId) { "The inbox changed" }
        check(File(checkNotNull(reopened.storage().path())).absoluteFile.normalize().path == expectedPath) {
            "The reopened database path changed"
        }
    } finally {
        reopened.end()
    }
}

// Resolve the default root from the Android app context for both factories.
suspend fun exerciseAndroidDefault(
    context: Context,
    signer: Signer,
    options: ClientOptions,
) {
    val resolvedStorage = StorageOptions(context, label = options.storage.label)
    val androidOptions =
        options.copy(
            storage = options.storage.copy(location = resolvedStorage.location),
        )
    val first = SDKClient.create(signer, androidOptions)
    val identity = first.identity()
    val inboxId = first.inboxId()
    val openedPath = first.storage().path()
    first.end()
    val reopened = SDKClient.build(identity, androidOptions.copy(allowOffline = true), inboxId = inboxId)
    try {
        check(reopened.inboxId() == inboxId) { "The inbox changed" }
        check(reopened.storage().path() == openedPath) { "The reopened database path changed" }
    } finally {
        reopened.end()
    }
}
