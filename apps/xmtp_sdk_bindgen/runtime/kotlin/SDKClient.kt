package uniffi.xmtp_sdk

import kotlinx.coroutines.flow.Flow

/**
 * The receive registry. A star-projected codec decodes to `Any`, the one place
 * where the value type is erased (Decision 3).
 */
private class CodecRegistry(
    codecs: List<ContentCodec<*>>,
) {
    private val codecs = codecs.associateBy { ContentCodecKey(it.type) }

    fun decode(encoded: EncodedContent): SDKMessageContent {
        val codec = codecs[ContentCodecKey(encoded.type)] ?: return SDKMessageContent.Unknown(encoded)
        return try {
            SDKMessageContent.Custom(encoded, codec.decode(encoded), null)
        } catch (error: Throwable) {
            SDKMessageContent.Custom(encoded, null, error)
        }
    }
}

/**
 * The host client resolves storage and owns the weak message lookup entry.
 * Generated forwarders in `ClientForwarding.kt` expose the other Client methods.
 * The generated Client stays private to the runtime.
 */
class SDKClient private constructor(
    internal val raw: Client,
    codecs: List<ContentCodec<*>>,
) {
    private val codecs = CodecRegistry(codecs)
    internal val listenerGates = ListenerGates()

    fun storage(): Storage = raw.storage()

    fun decodeCustom(encoded: EncodedContent): SDKMessageContent = codecs.decode(encoded)

    companion object {
        private fun resolved(
            options: ClientOptions,
            defaultDirectory: String?,
        ): ClientOptions {
            val location =
                if (options.storage.location is StorageLocation.Default) {
                    val directory =
                        defaultDirectory ?: throw XmtpException.StorageLocationRequired(
                            ErrorDetails(
                                "StorageLocationRequired",
                                ErrorCategory.STORAGE,
                                false,
                                "Default storage needs an Android context directory",
                            ),
                        )
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
            codecs: List<ContentCodec<*>> = emptyList(),
        ): SDKClient =
            SDKClient(Client.create(SDKForeign.signer(signer), resolved(options, defaultDirectory)), codecs).also {
                ClientRegistry.register(it)
            }

        suspend fun build(
            identity: PublicIdentity,
            options: ClientOptions,
            inboxId: InboxId? = null,
            defaultDirectory: String? = null,
            codecs: List<ContentCodec<*>> = emptyList(),
        ): SDKClient =
            SDKClient(
                Client.build(identity, resolved(options, defaultDirectory), inboxId),
                codecs,
            ).also { ClientRegistry.register(it) }

        suspend fun fetchServerConfiguration(backend: BackendSource): ServerConfiguration =
            uniffi.xmtp_sdk.fetchServerConfiguration(SDKForeign.backend(backend))

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            backend: BackendSource,
        ): Map<String, Boolean> = canMessageWithBackend(SDKForeign.backend(backend), identities)

        suspend fun inboxIdFor(
            identity: PublicIdentity,
            backend: BackendSource,
        ): InboxId = inboxIdForWithBackend(SDKForeign.backend(backend), identity)

        suspend fun inboxStates(
            ids: List<InboxId>,
            backend: BackendSource,
        ): List<InboxState> = inboxStatesWithBackend(SDKForeign.backend(backend), ids)

        suspend fun keyPackageStatuses(
            ids: List<InstallationId>,
            backend: BackendSource,
        ): Map<String, KeyPackageStatus> = keyPackageStatusesWithBackend(SDKForeign.backend(backend), ids)

        suspend fun newestMessageMetadata(
            ids: List<ConversationId>,
            backend: BackendSource,
        ): Map<String, MessageMetadataEntry> = newestMessageMetadataWithBackend(SDKForeign.backend(backend), ids)

        suspend fun revokeInstallations(
            signer: Signer,
            inboxId: InboxId,
            ids: List<InstallationId>,
            backend: BackendSource,
        ) = revokeInstallationsWithBackend(SDKForeign.backend(backend), SDKForeign.signer(signer), inboxId, ids)

        suspend fun isAddressAuthorized(
            address: String,
            inboxId: InboxId,
            backend: BackendSource,
        ): Boolean = isAddressAuthorizedWithBackend(SDKForeign.backend(backend), inboxId, address)

        suspend fun isInstallationAuthorized(
            installationId: InstallationId,
            inboxId: InboxId,
            backend: BackendSource,
        ): Boolean = isInstallationAuthorizedWithBackend(SDKForeign.backend(backend), inboxId, installationId)

        suspend fun verifySignedWithPublicKey(
            text: String,
            signature: ByteArray,
            publicKey: ByteArray,
        ): Boolean = uniffi.xmtp_sdk.verifySignedWithPublicKey(text, signature, publicKey)
    }

    suspend fun end() {
        listenerGates.stopAll()
        try {
            raw.end()
        } finally {
            ClientRegistry.remove(this)
        }
    }

    /** A value is acknowledged only when the next collection request starts. */
    fun messages(
        group: Group,
        options: ConversationMessageReaderOptions? = null,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Message> =
        messageFlow(
            this,
            open = { group.messageReader(options) },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )

    fun messages(
        dm: Dm,
        options: ConversationMessageReaderOptions? = null,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Message> =
        messageFlow(
            this,
            open = { dm.messageReader(options) },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )

    fun messages(
        options: MessageReaderOptions? = null,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Message> =
        messageFlow(
            this,
            open = { raw.conversations().messageReader(options) },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )

    fun conversationStream(
        kind: ConversationKind? = null,
        consentStates: List<ConsentState>? = null,
        onClose: ((SDKStreamCloseReason) -> Unit)? = null,
        onConnectionStateChange: ((ConnectionState?, ConnectionState) -> Unit)? = null,
    ): Flow<Conversation> =
        conversationFlow(
            this,
            open = { raw.conversations().conversationReader(ConversationReaderOptions(kind, consentStates)) },
            onClose = onClose,
            onConnectionStateChange = onConnectionStateChange,
        )
}
