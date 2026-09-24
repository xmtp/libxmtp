package uniffi.xmtp_sdk

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext

/** The host client resolves storage and owns the weak message lookup entry. */
class SDKClient private constructor(
    val raw: Client,
) {
    companion object {
        private fun resolved(
            options: ClientOptions,
            defaultDirectory: String?,
        ): ClientOptions {
            if (options.storage.location !is StorageLocation.Default) return options
            val directory = requireNotNull(defaultDirectory) { "Default storage needs a host directory" }
            return options.copy(storage = options.storage.copy(location = StorageLocation.Directory(directory)))
        }

        suspend fun create(
            signer: Signer,
            options: ClientOptions,
            defaultDirectory: String? = null,
        ): SDKClient =
            SDKClient(Client.create(signer, resolved(options, defaultDirectory))).also {
                ClientRegistry.register(it)
            }

        suspend fun build(
            identity: PublicIdentity,
            options: ClientOptions,
            inboxID: InboxID? = null,
            defaultDirectory: String? = null,
        ): SDKClient =
            SDKClient(
                Client.build(identity, resolved(options, defaultDirectory), inboxID),
            ).also { ClientRegistry.register(it) }

        suspend fun fetchServerConfiguration(options: BackendOptions): ServerConfiguration =
            uniffi.xmtp_sdk.fetchServerConfiguration(BackendSource.Options(options))

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            backend: Backend,
        ): List<CanMessageEntry> = canMessageWithBackend(BackendSource.Connected(backend), identities)

        suspend fun inboxIDFor(
            identity: PublicIdentity,
            backend: Backend,
        ): InboxID = inboxIDForWithBackend(BackendSource.Connected(backend), identity)

        suspend fun inboxStates(
            ids: List<InboxID>,
            backend: Backend,
        ): List<InboxState> = inboxStatesWithBackend(BackendSource.Connected(backend), ids)

        suspend fun keyPackageStatuses(
            ids: List<InstallationID>,
            backend: Backend,
        ): List<KeyPackageStatusEntry> = keyPackageStatusesWithBackend(BackendSource.Connected(backend), ids)

        suspend fun newestMessageMetadata(
            ids: List<ConversationID>,
            backend: Backend,
        ): List<MessageMetadataEntry> = newestMessageMetadataWithBackend(BackendSource.Connected(backend), ids)

        suspend fun revokeInstallations(
            signer: Signer,
            inboxID: InboxID,
            ids: List<InstallationID>,
            backend: Backend,
        ) = revokeInstallationsWithBackend(BackendSource.Connected(backend), signer, inboxID, ids)

        suspend fun isAddressAuthorized(
            address: String,
            inboxID: InboxID,
            backend: Backend,
        ): Boolean = isAddressAuthorizedWithBackend(BackendSource.Connected(backend), inboxID, address)

        suspend fun isInstallationAuthorized(
            installationID: InstallationID,
            inboxID: InboxID,
            backend: Backend,
        ): Boolean = isInstallationAuthorizedWithBackend(BackendSource.Connected(backend), inboxID, installationID)

        suspend fun verifySignedWithPublicKey(
            text: String,
            signature: ByteArray,
            publicKey: ByteArray,
        ): Boolean = uniffi.xmtp_sdk.verifySignedWithPublicKey(text, signature, publicKey)
    }

    suspend fun end() {
        try {
            raw.end()
        } finally {
            ClientRegistry.remove(this)
        }
    }

    /** A value is acknowledged only when the next collection request starts. */
    fun messages(group: Group): Flow<Message> =
        flow {
            val owner = this@SDKClient
            val opening = CoroutineScope(Dispatchers.Default).async { group.messageReader() }
            var reader: MessageReader? = null
            try {
                reader = opening.await()
                while (true) {
                    owner.raw.clientKey()
                    val value = reader.next() ?: break
                    emit(value)
                }
            } finally {
                withContext(NonCancellable) {
                    runCatching { (reader ?: opening.await()).end() }
                }
            }
        }
}
