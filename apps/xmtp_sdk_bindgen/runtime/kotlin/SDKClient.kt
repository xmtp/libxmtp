package uniffi.xmtp_sdk

import kotlinx.coroutines.flow.Flow

private class CodecRegistry(
    codecs: List<SDKContentCodec>,
) {
    private val codecs = codecs.associateBy { it.key }

    fun decode(encoded: EncodedContent): SDKMessageContent {
        val codec = codecs[SDKContentCodecKey(encoded.type)] ?: return SDKMessageContent.Unknown(encoded)
        return try {
            SDKMessageContent.Custom(encoded, codec.decode(encoded), null)
        } catch (error: Throwable) {
            SDKMessageContent.Custom(encoded, null, error)
        }
    }
}

/** The host client resolves storage and owns the weak message lookup entry. */
class SDKClient private constructor(
    val raw: Client,
    codecs: List<SDKContentCodec>,
) {
    private val codecs = CodecRegistry(codecs)

    fun storage(): Storage = raw.storage()

    fun decodeCustom(encoded: EncodedContent): SDKMessageContent = codecs.decode(encoded)

    companion object {
        private fun resolved(
            options: ClientOptions,
            defaultDirectory: String?,
        ): ClientOptions {
            val location =
                if (options.storage.location is StorageLocation.Default) {
                    val directory = requireNotNull(defaultDirectory) { "Default storage needs a host directory" }
                    StorageLocation.Directory(directory)
                } else {
                    options.storage.location
                }
            return options.copy(
                storage = options.storage.copy(location = location),
                backend = options.backend?.let(SDKForeign::backend),
            )
        }

        suspend fun create(
            signer: Signer,
            options: ClientOptions,
            defaultDirectory: String? = null,
            codecs: List<SDKContentCodec> = emptyList(),
        ): SDKClient =
            SDKClient(Client.create(SDKForeign.signer(signer), resolved(options, defaultDirectory)), codecs).also {
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
            uniffi.xmtp_sdk.fetchServerConfiguration(SDKForeign.backend(backend))

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            backend: BackendSource,
        ): List<CanMessageEntry> = canMessageWithBackend(SDKForeign.backend(backend), identities)

        suspend fun inboxIDFor(
            identity: PublicIdentity,
            backend: BackendSource,
        ): InboxID = inboxIDForWithBackend(SDKForeign.backend(backend), identity)

        suspend fun inboxStates(
            ids: List<InboxID>,
            backend: BackendSource,
        ): List<InboxState> = inboxStatesWithBackend(SDKForeign.backend(backend), ids)

        suspend fun keyPackageStatuses(
            ids: List<InstallationID>,
            backend: BackendSource,
        ): List<KeyPackageStatusEntry> = keyPackageStatusesWithBackend(SDKForeign.backend(backend), ids)

        suspend fun newestMessageMetadata(
            ids: List<ConversationID>,
            backend: BackendSource,
        ): List<MessageMetadataEntry> = newestMessageMetadataWithBackend(SDKForeign.backend(backend), ids)

        suspend fun revokeInstallations(
            signer: Signer,
            inboxID: InboxID,
            ids: List<InstallationID>,
            backend: BackendSource,
        ) = revokeInstallationsWithBackend(SDKForeign.backend(backend), SDKForeign.signer(signer), inboxID, ids)

        suspend fun isAddressAuthorized(
            address: String,
            inboxID: InboxID,
            backend: BackendSource,
        ): Boolean = isAddressAuthorizedWithBackend(SDKForeign.backend(backend), inboxID, address)

        suspend fun isInstallationAuthorized(
            installationID: InstallationID,
            inboxID: InboxID,
            backend: BackendSource,
        ): Boolean = isInstallationAuthorizedWithBackend(SDKForeign.backend(backend), inboxID, installationID)

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
    fun messages(
        group: Group,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Message> =
        messageFlow(
            this,
            open = { group.messageReader() },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )

    fun conversations(
        kind: ConversationKind? = null,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Conversation> =
        conversationFlow(
            this,
            open = { raw.conversations().conversationReader(kind) },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )
}
