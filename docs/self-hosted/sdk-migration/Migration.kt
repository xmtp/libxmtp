import android.content.Context
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*
import java.io.File

// End the old SDK client before calling this function.
suspend fun exerciseMigration(
    context: Context,
    existingIdentity: PublicIdentity,
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
    val first = SDKClient.build(context, existingIdentity, explicit)
    var firstFailure: Throwable? = null
    val (identity, inboxId, openedPath) =
        try {
            Triple(first.identity(), first.inboxId(), first.storage().path())
        } catch (error: Throwable) {
            firstFailure = error
            throw error
        } finally {
            endMigrationClient(first, firstFailure)
        }
    currentCoroutineContext().ensureActive()
    check(File(checkNotNull(openedPath)).absoluteFile.normalize().path == expectedPath) {
        "The SDK opened another database"
    }

    val reopened = SDKClient.build(context, identity, explicit.copy(allowOffline = true), inboxId = inboxId)
    var reopenedFailure: Throwable? = null
    try {
        check(reopened.inboxId() == inboxId) { "The inbox changed" }
        check(File(checkNotNull(reopened.storage().path())).absoluteFile.normalize().path == expectedPath) {
            "The reopened database path changed"
        }
    } catch (error: Throwable) {
        reopenedFailure = error
        throw error
    } finally {
        endMigrationClient(reopened, reopenedFailure)
    }
    currentCoroutineContext().ensureActive()
}

// Both factories resolve default storage from the Android app context.
suspend fun exerciseAndroidDefault(
    context: Context,
    existingIdentity: PublicIdentity,
    existingInboxId: String,
    options: ClientOptions,
) {
    val androidOptions =
        options.copy(
            storage = options.storage.copy(location = StorageLocation.Default),
        )
    val first = SDKClient.build(context, existingIdentity, androidOptions, inboxId = existingInboxId)
    var firstFailure: Throwable? = null
    val (identity, inboxId, openedPath) =
        try {
            Triple(first.identity(), first.inboxId(), first.storage().path())
        } catch (error: Throwable) {
            firstFailure = error
            throw error
        } finally {
            endMigrationClient(first, firstFailure)
        }
    currentCoroutineContext().ensureActive()
    val reopened = SDKClient.build(context, identity, androidOptions.copy(allowOffline = true), inboxId = inboxId)
    var reopenedFailure: Throwable? = null
    try {
        check(reopened.inboxId() == inboxId) { "The inbox changed" }
        check(reopened.storage().path() == openedPath) { "The reopened database path changed" }
    } catch (error: Throwable) {
        reopenedFailure = error
        throw error
    } finally {
        endMigrationClient(reopened, reopenedFailure)
    }
    currentCoroutineContext().ensureActive()
}

private suspend fun endMigrationClient(
    client: SDKClient,
    primaryError: Throwable? = null,
) {
    try {
        withContext(NonCancellable) { client.end() }
    } catch (error: Throwable) {
        if (primaryError == null) throw error
        if (error !== primaryError) primaryError.addSuppressed(error)
    }
}
