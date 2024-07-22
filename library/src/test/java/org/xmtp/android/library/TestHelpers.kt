package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.Fetcher
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.io.File
import java.net.URL

class TestFetcher : Fetcher {
    override fun fetch(url: URL): ByteArray {
        return File(url.toString().replace("https://", "")).readBytes()
    }
}

data class Fixtures(
    val aliceAccount: PrivateKeyBuilder,
    val bobAccount: PrivateKeyBuilder,
    val clientOptions: ClientOptions? = ClientOptions(
        ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false)
    ),
) {
    var aliceClient: Client = runBlocking { Client().create(account = aliceAccount, options = clientOptions) }
    var alice: PrivateKey = aliceAccount.getPrivateKey()
    var bob: PrivateKey = bobAccount.getPrivateKey()
    var bobClient: Client = runBlocking { Client().create(account = bobAccount, options = clientOptions) }

    constructor(clientOptions: ClientOptions?) : this(
        aliceAccount = PrivateKeyBuilder(),
        bobAccount = PrivateKeyBuilder(),
        clientOptions = clientOptions
    )
}

fun fixtures(clientOptions: ClientOptions? = null): Fixtures =
    Fixtures(clientOptions)
