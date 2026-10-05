package org.xmtp.android.library

import uniffi.xmtp_sdk.*

// Use the real generated forwarders with a recording native boundary.
internal fun testSDKClient(
    raw: Client,
    codecs: List<ContentCodec<*>> = emptyList(),
): SDKClient {
    val constructor = SDKClient::class.java.getDeclaredConstructor(Client::class.java, List::class.java)
    constructor.isAccessible = true
    return constructor.newInstance(raw, codecs)
}
