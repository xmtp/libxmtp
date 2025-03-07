package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.security.SecureRandom
import java.util.Date

typealias PrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKey

class PrivateKeyBuilder : SigningKey {
    private var privateKey: PrivateKey

    constructor() {
        privateKey = PrivateKey.newBuilder().also {
            val time = Date().time
            it.timestamp = time
            val privateKeyData = SecureRandom().generateSeed(32)
            it.secp256K1 = it.secp256K1.toBuilder().also { keyBuilder ->
                keyBuilder.bytes = privateKeyData.toByteString()
            }.build()
            val publicData = KeyUtil.getPublicKey(privateKeyData)
            val uncompressedKey = KeyUtil.addUncompressedByte(publicData)
            it.publicKey = it.publicKey.toBuilder().also { pubKey ->
                pubKey.timestamp = time
                pubKey.secp256K1Uncompressed =
                    pubKey.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes = uncompressedKey.toByteString()
                    }.build()
            }.build()
        }.build()
    }

    constructor(key: PrivateKey) {
        privateKey = key
    }

    companion object {
        fun buildFromPrivateKeyData(privateKeyData: ByteArray): PrivateKey {
            return PrivateKey.newBuilder().apply {
                val time = Date().time
                timestamp = time
                secp256K1 = secp256K1.toBuilder().also { keyBuilder ->
                    keyBuilder.bytes = privateKeyData.toByteString()
                }.build()
                val publicData = KeyUtil.getPublicKey(privateKeyData)
                val uncompressedKey = KeyUtil.addUncompressedByte(publicData)
                publicKey = publicKey.toBuilder().apply {
                    timestamp = time
                    secp256K1Uncompressed = secp256K1Uncompressed.toBuilder().apply {
                        bytes = uncompressedKey.toByteString()
                    }.build()
                }.build()
            }.build()
        }
    }

    fun getPrivateKey(): PrivateKey {
        return privateKey
    }

    override suspend fun sign(data: ByteArray): SignatureOuterClass.Signature {
        val signatureData =
            Sign.signMessage(
                data,
                ECKeyPair.create(privateKey.secp256K1.bytes.toByteArray()),
                false,
            )
        val signatureKey = KeyUtil.getSignatureBytes(signatureData)

        val sign = SignatureOuterClass.Signature.newBuilder().also {
            it.ecdsaCompact = it.ecdsaCompact.toBuilder().also { builder ->
                builder.bytes = signatureKey.take(64).toByteArray().toByteString()
                builder.recovery = signatureKey[64].toInt()
            }.build()
        }.build()

        return sign
    }

    override suspend fun sign(message: String): SignatureOuterClass.Signature {
        val digest = Signature.newBuilder().build().ethHash(message)
        return sign(digest)
    }

    override val publicIdentity: PublicIdentity
        get() = PublicIdentity(IdentityKind.ETHEREUM, privateKey.walletAddress)
}

fun PrivateKey.generate(): PrivateKey {
    return PrivateKeyBuilder.buildFromPrivateKeyData(SecureRandom().generateSeed(32))
}

val PrivateKey.walletAddress: String
    get() = publicKey.walletAddress.lowercase()
