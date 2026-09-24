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
    codecs: List<SDKContentCodec>,
) {
    private val codecs = codecs.associateBy { it.key }

    fun decodeCustom(encoded: EncodedContent): SDKMessageContent {
        val codec = codecs[SDKContentCodecKey(encoded.type)] ?: return SDKMessageContent.Unknown(encoded)
        return try {
            SDKMessageContent.Custom(encoded, codec.decode(encoded), null)
        } catch (error: Throwable) {
            SDKMessageContent.Custom(encoded, null, error)
        }
    }

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
            codecs: List<SDKContentCodec> = emptyList(),
        ): SDKClient =
            SDKClient(Client.create(signer, resolved(options, defaultDirectory)), codecs).also {
                ClientRegistry.register(it)
            }

        suspend fun build(
            identity: PublicIdentity,
            options: ClientOptions,
            inboxID: InboxID? = null,
            defaultDirectory: String? = null,
            codecs: List<SDKContentCodec> = emptyList(),
        ): SDKClient =
            SDKClient(
                Client.build(identity, resolved(options, defaultDirectory), inboxID),
                codecs,
            ).also { ClientRegistry.register(it) }

        suspend fun fetchServerConfiguration(backend: BackendSource): ServerConfiguration =
            uniffi.xmtp_sdk.fetchServerConfiguration(backend)

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            backend: BackendSource,
        ): List<CanMessageEntry> = canMessageWithBackend(backend, identities)

        suspend fun inboxIDFor(
            identity: PublicIdentity,
            backend: BackendSource,
        ): InboxID = inboxIDForWithBackend(backend, identity)

        suspend fun inboxStates(
            ids: List<InboxID>,
            backend: BackendSource,
        ): List<InboxState> = inboxStatesWithBackend(backend, ids)

        suspend fun keyPackageStatuses(
            ids: List<InstallationID>,
            backend: BackendSource,
        ): List<KeyPackageStatusEntry> = keyPackageStatusesWithBackend(backend, ids)

        suspend fun newestMessageMetadata(
            ids: List<ConversationID>,
            backend: BackendSource,
        ): List<MessageMetadataEntry> = newestMessageMetadataWithBackend(backend, ids)

        suspend fun revokeInstallations(
            signer: Signer,
            inboxID: InboxID,
            ids: List<InstallationID>,
            backend: BackendSource,
        ) = revokeInstallationsWithBackend(backend, signer, inboxID, ids)

        suspend fun isAddressAuthorized(
            address: String,
            inboxID: InboxID,
            backend: BackendSource,
        ): Boolean = isAddressAuthorizedWithBackend(backend, inboxID, address)

        suspend fun isInstallationAuthorized(
            installationID: InstallationID,
            inboxID: InboxID,
            backend: BackendSource,
        ): Boolean = isInstallationAuthorizedWithBackend(backend, inboxID, installationID)

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
