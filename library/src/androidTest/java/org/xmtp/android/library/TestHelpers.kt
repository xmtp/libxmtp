package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import java.util.Date

class FakeWallet : SigningKey {
    private var privateKey: PrivateKey
    private var privateKeyBuilder: PrivateKeyBuilder

    constructor(key: PrivateKey, builder: PrivateKeyBuilder) {
        privateKey = key
        privateKeyBuilder = builder
    }

    companion object {
        fun generate(): FakeWallet {
            val key = PrivateKeyBuilder()
            return FakeWallet(key.getPrivateKey(), key)
        }
    }

    override suspend fun sign(data: ByteArray): Signature {
        val signature = privateKeyBuilder.sign(data)
        return signature
    }

    override suspend fun sign(message: String): Signature {
        val signature = privateKeyBuilder.sign(message)
        return signature
    }

    override val address: String
        get() = privateKey.walletAddress
}

data class Fixtures(
    val aliceAccount: PrivateKeyBuilder,
    val bobAccount: PrivateKeyBuilder,
    val caroAccount: PrivateKeyBuilder,
    val clientOptions: ClientOptions? = ClientOptions(
        ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false)
    ),
) {
    var alice: PrivateKey = aliceAccount.getPrivateKey()
    var aliceClient: Client = Client().create(account = aliceAccount, options = clientOptions)
    var bob: PrivateKey = bobAccount.getPrivateKey()
    var bobClient: Client = Client().create(account = bobAccount, options = clientOptions)
    var caro: PrivateKey = caroAccount.getPrivateKey()
    var caroClient: Client = Client().create(account = caroAccount, options = clientOptions)

    constructor(clientOptions: ClientOptions?) : this(
        aliceAccount = PrivateKeyBuilder(),
        bobAccount = PrivateKeyBuilder(),
        caroAccount = PrivateKeyBuilder(),
        clientOptions = clientOptions
    )

    fun publishLegacyContact(client: Client) {
        val contactBundle = ContactBundle.newBuilder().also { builder ->
            builder.v1 = builder.v1.toBuilder().also {
                it.keyBundle = client.privateKeyBundleV1.toPublicKeyBundle()
            }.build()
        }.build()
        val envelope = Envelope.newBuilder().apply {
            contentTopic = Topic.contact(client.address).description
            timestampNs = (Date().time * 1_000_000)
            message = contactBundle.toByteString()
        }.build()

        runBlocking { client.publish(envelopes = listOf(envelope)) }
    }
}

fun fixtures(clientOptions: ClientOptions? = null): Fixtures =
    Fixtures(clientOptions)
