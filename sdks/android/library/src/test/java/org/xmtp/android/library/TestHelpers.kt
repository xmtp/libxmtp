package org.xmtp.android.library

import uniffi.xmtp_sdk.*

// Use the real generated forwarders with a recording native boundary.
internal fun testSDKClient(
    raw: Client,
    codecs: List<ContentCodec<*>> = emptyList(),
): SDKClient {
    val registry = Class.forName("uniffi.xmtp_sdk.CodecRegistry")
    val prepare = registry.getDeclaredConstructor(List::class.java)
    prepare.isAccessible = true
    val constructor = SDKClient::class.java.getDeclaredConstructor(Client::class.java, registry)
    constructor.isAccessible = true
    return constructor.newInstance(raw, prepare.newInstance(codecs)).also(ClientRegistry::register)
}
