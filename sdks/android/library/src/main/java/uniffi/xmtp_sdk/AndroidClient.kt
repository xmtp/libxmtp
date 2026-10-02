package uniffi.xmtp_sdk

import android.content.Context

private fun androidOptions(
    context: Context,
    options: ClientOptions,
): ClientOptions =
    if (options.storage.location is StorageLocation.Default) {
        options.copy(storage = options.storage.copy(location = StorageOptions(context).location))
    } else {
        options
    }

/** Create a client with Android default storage and process lifecycle control. */
suspend fun SDKClient.Companion.create(
    context: Context,
    signer: Signer,
    options: ClientOptions,
    codecs: List<ContentCodec<*>> = emptyList(),
): SDKClient {
    AndroidStreamLifecycle.awaitReady()
    return create(signer, androidOptions(context.applicationContext, options), codecs = codecs)
}

/** Open an existing identity with the same Android storage and lifecycle rules. */
suspend fun SDKClient.Companion.build(
    context: Context,
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId: InboxId? = null,
    codecs: List<ContentCodec<*>> = emptyList(),
): SDKClient {
    AndroidStreamLifecycle.awaitReady()
    return build(identity, androidOptions(context.applicationContext, options), inboxId, codecs = codecs)
}
