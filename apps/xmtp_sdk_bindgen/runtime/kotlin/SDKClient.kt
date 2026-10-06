package uniffi.xmtp_sdk

/**
 * The receive registry. A star-projected codec decodes to `Any`, the one place
 * where the value type is erased (Decision 3).
 */
private class CodecRegistry(
    codecs: List<ContentCodec<*>>,
) {
    private val codecs = codecs.associateBy { ContentCodecKey(it.type) }

    fun decode(
        encoded: EncodedContent,
        rawBytes: ByteArray,
    ): SDKMessageContent {
        val codec =
            codecs[ContentCodecKey(encoded.type)]
                ?: return SDKMessageContent.Unknown(
                    encoded,
                    rawBytes,
                    ErrorDetails(
                        "CodecNotFound",
                        ErrorCategory.INPUT,
                        false,
                        "content type has no registered host codec",
                    ),
                )
        return try {
            SDKMessageContent.Custom(encoded, rawBytes, codec.decode(encoded), null)
        } catch (error: Throwable) {
            SDKMessageContent.Custom(
                encoded,
                rawBytes,
                null,
                ErrorDetails(
                    "CodecDecodeFailed",
                    ErrorCategory.CALLBACK,
                    false,
                    runCatching { error.toString() }.getOrDefault("custom content codec failed"),
                ),
            )
        }
    }
}

/**
 * The host client resolves storage and owns the weak message lookup entry.
 * Generated forwarders in `ClientForwarding.kt` expose the other Client methods
 * and the Client statics.
 * The generated Client stays private to the runtime.
 */
class SDKClient private constructor(
    internal val raw: Client,
    codecs: List<ContentCodec<*>>,
) {
    private val codecs = CodecRegistry(codecs)
    internal val listenerGates = ListenerGates()

    fun storage(): Storage = raw.storage()

    internal fun decodeCustom(
        encoded: EncodedContent,
        rawBytes: ByteArray,
    ): SDKMessageContent = codecs.decode(encoded, rawBytes)

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
    }

    suspend fun end() {
        listenerGates.stopAll()
        try {
            raw.end()
        } finally {
            ClientRegistry.remove(this)
        }
    }

    val conversations: Conversations get() = raw.conversations()
}
