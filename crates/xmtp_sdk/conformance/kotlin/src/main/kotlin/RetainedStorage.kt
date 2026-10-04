import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.nio.file.Files

// Reconnect keeps a live store. An ended client must be built again.
internal suspend fun checkStorageReconnectAndRebuildKeepHistory(backend: BackendOptions) {
    withTimeout(30_000) {
        val directory = Files.createTempDirectory("kotlin-retained-storage-")
        val database = directory.resolve("client.db3").toString()
        val options =
            ClientOptions(
                backend = BackendSource.Options(backend),
                storage = StorageOptions(location = StorageLocation.Explicit(database, "$database-attachments")),
                deviceSync = false,
            )
        val signer = generateLocalSigner()
        val identity = signer.identity()
        var owner: SDKClient? = null
        try {
            val first = SDKClient.create(signer, options).also { owner = it }
            val inbox = first.inboxId()
            val group = first.conversations().createGroup(emptyList())
            val groupId = group.id()
            val messageId = group.sendText("history before reconnect")
            val storage = first.storage()
            check(storage.path() == database)
            storage.reconnect()
            check(first.inboxId() == inbox && storage.path() == database)
            check(group.messages().any { it.id == messageId }) { "live reconnect lost stored history" }

            withContext(NonCancellable) { first.end() }
            val failure = runCatching { storage.reconnect() }.exceptionOrNull()
            check(failure is XmtpException.ClientClosed) { "reconnect accepted an ended client: $failure" }

            val rebuilt = SDKClient.build(identity, options, inbox).also { owner = it }
            check(rebuilt.inboxId() == inbox && rebuilt.storage().path() == database)
            val restored = checkNotNull(rebuilt.conversations().getById(groupId)) as Conversation.Group
            check(restored.group.messages().any { it.id == messageId }) { "rebuild lost stored history" }
        } finally {
            withContext(NonCancellable) { owner?.end() }
            directory.toFile().deleteRecursively()
        }
    }
    println("Kotlin retained storage: live reconnect keeps history, ended reconnect fails, rebuild keeps history")
}
