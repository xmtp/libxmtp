package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Keys
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.EncryptedPrivateKeyBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.PrivateKeyBundleV2
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.decrypted
import org.xmtp.android.library.messages.encrypted
import org.xmtp.android.library.messages.ensureWalletSignature
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import java.util.Date

typealias PublishResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishResponse
typealias QueryResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryResponse

data class ClientOptions(val api: Api = Api()) {
    data class Api(val env: XMTPEnvironment = XMTPEnvironment.DEV, val isSecure: Boolean = true)
}

class Client() {
    var address: String? = null
        private set
    var privateKeyBundleV1: PrivateKeyBundleV1? = null
        private set
    var apiClient: ApiClient = GRPCApiClient(XMTPEnvironment.DEV, true)
        private set
    val environment: XMTPEnvironment = apiClient.environment
    val contacts: Contacts = Contacts(client = this)
    val conversations: Conversations = Conversations(client = this)

    companion object {
        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }
        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }
    }

    constructor(
        address: String,
        privateKeyBundleV1: PrivateKeyBundleV1,
        apiClient: ApiClient
    ) : this() {
        this.address = address
        this.privateKeyBundleV1 = privateKeyBundleV1
        this.apiClient = apiClient
    }

    fun create(account: SigningKey, options: ClientOptions? = null): Client {
        val clientOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(environment = clientOptions.api.env, secure = clientOptions.api.isSecure)
        return create(account = account, apiClient = apiClient)
    }

    fun create(account: SigningKey, apiClient: ApiClient): Client {
        return runBlocking {
            try {
                val privateKeyBundleV1 = loadOrCreateKeys(account, apiClient)
                val client = Client(account.address, privateKeyBundleV1, apiClient)
                client.ensureUserContactPublished()
                client
            } catch (e: java.lang.Exception) {
                throw XMTPException("Error creating client", e)
            }
        }
    }

    fun buildFromBundle(bundle: PrivateKeyBundle, options: ClientOptions? = null): Client =
        buildFromV1Bundle(v1Bundle = bundle.v1, options = options)

    fun buildFromV1Bundle(v1Bundle: PrivateKeyBundleV1, options: ClientOptions? = null): Client {
        val address = v1Bundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress
        val newOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(environment = newOptions.api.env, secure = newOptions.api.isSecure)
        return Client(address = address, privateKeyBundleV1 = v1Bundle, apiClient = apiClient)
    }

    private suspend fun loadOrCreateKeys(
        account: SigningKey,
        apiClient: ApiClient
    ): PrivateKeyBundleV1 {
        val keys = loadPrivateKeys(account, apiClient)
        if (keys != null) {
            return keys
        } else {
            val v1Keys = PrivateKeyBundleV1.newBuilder().build().generate(account)
            val keyBundle = PrivateKeyBundleBuilder.buildFromV1Key(v1Keys)
            val encryptedKeys = keyBundle.encrypted(account)
            val authorizedIdentity = AuthorizedIdentity(privateKeyBundleV1 = v1Keys)
            authorizedIdentity.address = account.address
            val authToken = authorizedIdentity.createAuthToken()
            apiClient.setAuthToken(authToken)
            apiClient.publish(
                envelopes = listOf(
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.userPrivateStoreKeyBundle(account.address),
                        timestamp = Date(),
                        message = encryptedKeys.toByteArray()
                    )
                )
            )
            return v1Keys
        }
    }

    private suspend fun loadPrivateKeys(
        account: SigningKey,
        apiClient: ApiClient
    ): PrivateKeyBundleV1? {
        val topics: List<Topic> = listOf(Topic.userPrivateStoreKeyBundle(account.address))
        val res = apiClient.query(topics = topics)
        for (envelope in res.envelopesList) {
            try {
                val encryptedBundle = EncryptedPrivateKeyBundle.parseFrom(envelope.message)
                val bundle = encryptedBundle.decrypted(account)
                return bundle.v1
            } catch (e: Throwable) {
                print("Error decoding encrypted private key bundle: $e")
                continue
            }
        }
        return null
    }

    fun publishUserContact(legacy: Boolean = false) {
        val envelopes: MutableList<MessageApiOuterClass.Envelope> = mutableListOf()
        if (legacy) {
            val contactBundle = ContactBundle.newBuilder().also {
                it.v1Builder.keyBundle = privateKeyBundleV1?.toPublicKeyBundle()
            }.build()

            val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
                address?.let {
                    contentTopic = Topic.contact(it).description
                }
                timestampNs = Date().time * 1_000_000
                message = contactBundle.toByteString()
            }.build()

            envelopes.add(envelope)
        }
        val contactBundle = ContactBundle.newBuilder().also {
            it.v2Builder.keyBundle = keys?.getPublicKeyBundle()
        }.build()
        contactBundle.v2.keyBundle.identityKey.signature.ensureWalletSignature()
        val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
            address?.let {
                contentTopic = Topic.contact(it).description
            }
            timestampNs = Date().time * 1_000_000
            message = contactBundle.toByteString()
        }.build()
        envelopes.add(envelope)
        runBlocking { publish(envelopes = envelopes) }
    }

    fun getUserContact(peerAddress: String): ContactBundle? {
        return contacts.find(Keys.toChecksumAddress(peerAddress))
    }

    suspend fun query(topics: List<Topic>): QueryResponse {
        return apiClient.query(topics = topics)
    }

    fun publish(envelopes: List<Envelope>): PublishResponse {
        privateKeyBundleV1?.let {
            address?.let { address ->
                val authorized = AuthorizedIdentity(
                    address = address,
                    authorized = it.identityKey.publicKey,
                    identity = it.identityKey
                )
                val authToken = authorized.createAuthToken()
                apiClient.setAuthToken(authToken)
            }
        }

        return runBlocking { apiClient.publish(envelopes = envelopes) }
    }

    fun ensureUserContactPublished() {
        address?.let {
            val contact = getUserContact(peerAddress = it)
            if (contact != null && keys?.getPublicKeyBundle() == contact.v2.keyBundle) {
                return
            }
        }

        publishUserContact(legacy = true)
    }

    val privateKeyBundle: PrivateKeyBundle?
        get() = privateKeyBundleV1?.let { PrivateKeyBundleBuilder.buildFromV1Key(it) }

    val v1keys: PrivateKeyBundleV1?
        get() = privateKeyBundleV1

    val keys: PrivateKeyBundleV2?
        get() = privateKeyBundleV1?.toV2()
}
