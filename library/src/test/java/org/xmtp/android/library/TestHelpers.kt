package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass

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

    override val address: String
        get() = privateKey.walletAddress

    override fun sign(data: ByteArray): Signature {
        val signature = privateKeyBuilder.sign(data)
        return signature
    }

    override fun sign(message: String): Signature {
        val signature = privateKeyBuilder.sign(message)
        return signature
    }
}

class FakeApiClient : ApiClient {
    override val environment: XMTPEnvironment = XMTPEnvironment.LOCAL
    private var authToken: String? = null
    private val responses: MutableMap<String, List<Envelope>> = mutableMapOf()
    val published: MutableList<Envelope> = mutableListOf()
    var forbiddingQueries = false

    fun assertNoPublish(callback: () -> Unit) {
        val oldCount = published.size
        callback()
        assertEquals(oldCount, published.size)
    }

    fun assertNoQuery(callback: () -> Unit) {
        forbiddingQueries = true
        callback()
        forbiddingQueries = false
    }

    fun findPublishedEnvelope(topic: Topic): Envelope? =
        findPublishedEnvelope(topic.description)

    fun findPublishedEnvelope(topic: String): Envelope? {
        for (envelope in published.reversed()) {
            if (envelope.contentTopic == topic) {
                return envelope
            }
        }
        return null
    }

    override fun setAuthToken(token: String) {
        authToken = token
    }

    override suspend fun query(topics: List<Topic>): MessageApiOuterClass.QueryResponse {
        return queryStrings(topics = topics.map { it.description })
    }

    override suspend fun queryStrings(topics: List<String>): MessageApiOuterClass.QueryResponse {
        val result: MutableList<Envelope> = mutableListOf()
        for (topic in topics) {
            val response = responses.toMutableMap().remove(topic)
            if (response != null) {
                result.addAll(response)
            }
            result.addAll(
                published.filter {
                    it.contentTopic == topic
                }.reversed()
            )
        }
        return QueryResponse.newBuilder().also {
            it.addAllEnvelopes(result)
        }.build()
    }

    override suspend fun publish(envelopes: List<MessageApiOuterClass.Envelope>): MessageApiOuterClass.PublishResponse {
        for (envelope in envelopes) {
        }
        published.addAll(envelopes)
        return PublishResponse.newBuilder().build()
    }
}

data class Fixtures(val aliceAccount: PrivateKeyBuilder, val bobAccount: PrivateKeyBuilder) {
    var fakeApiClient: FakeApiClient = FakeApiClient()
    var alice: PrivateKey = aliceAccount.getPrivateKey()
    var aliceClient: Client = Client().create(account = aliceAccount, apiClient = fakeApiClient)
    var bob: PrivateKey = bobAccount.getPrivateKey()
    var bobClient: Client = Client().create(account = bobAccount, apiClient = fakeApiClient)

    constructor() : this(aliceAccount = PrivateKeyBuilder(), bobAccount = PrivateKeyBuilder())
}

fun fixtures(): Fixtures =
    Fixtures()
