package org.xmtp.android.library

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.Fetcher
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.io.File
import java.net.URL
import java.security.SecureRandom

class TestFetcher : Fetcher {
    override fun fetch(url: URL): ByteArray = File(url.toString().replace("https://", "")).readBytes()
}

data class Fixtures(
    val aliceAccount: PrivateKeyBuilder,
    val bobAccount: PrivateKeyBuilder,
) {
    val key = SecureRandom().generateSeed(32)
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val clientOptions =
        ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false),
            dbEncryptionKey = key,
            appContext = context,
        )
    var aliceClient: Client =
        runBlocking { Client.create(account = aliceAccount, options = clientOptions) }
    var alice: PrivateKey = aliceAccount.getPrivateKey()
    var bob: PrivateKey = bobAccount.getPrivateKey()
    var bobClient: Client =
        runBlocking { Client.create(account = bobAccount, options = clientOptions) }

    constructor() : this(
        aliceAccount = PrivateKeyBuilder(),
        bobAccount = PrivateKeyBuilder(),
    )
}

fun fixtures(): Fixtures = Fixtures()
