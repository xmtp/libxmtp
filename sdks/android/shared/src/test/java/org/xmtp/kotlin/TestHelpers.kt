package org.xmtp.kotlin

import kotlinx.coroutines.runBlocking
import org.xmtp.kotlin.codecs.Fetcher
import org.xmtp.kotlin.messages.PrivateKey
import org.xmtp.kotlin.messages.PrivateKeyBuilder
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
    val dbDirectory = System.getProperty("java.io.tmpdir") + "/xmtp_test_" + System.nanoTime()
    val clientOptions =
        ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false),
            dbEncryptionKey = key,
            dbDirectory = dbDirectory,
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
