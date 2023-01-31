package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Hash
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.SigningKey
import org.xmtp.proto.message.contents.PublicKeyOuterClass
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.security.SecureRandom

typealias PrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKey

class PrivateKeyBuilder : SigningKey {
    private var privateKey: PrivateKey

    constructor() {
        privateKey = PrivateKey.newBuilder().also {
            val time = System.currentTimeMillis()
            it.timestamp = time
            val privateKeyData = SecureRandom().generateSeed(32)
            it.secp256K1Builder.bytes = privateKeyData.toByteString()
            val publicData = KeyUtil.getPublicKey(privateKeyData)
            val uncompressedKey = KeyUtil.addUncompressedByte(publicData)
            it.publicKeyBuilder.also { pubKey ->
                pubKey.timestamp = time
                pubKey.secp256K1UncompressedBuilder.bytes = uncompressedKey.toByteString()
            }.build()
        }.build()
    }

    constructor(key: PrivateKey) {
        privateKey = key
    }

    companion object {
        fun buildFromPrivateKeyData(privateKeyData: ByteArray): PrivateKey {
            return PrivateKey.newBuilder().apply {
                val time = System.currentTimeMillis()
                timestamp = time
                secp256K1Builder.bytes = privateKeyData.toByteString()
                val publicData = KeyUtil.getPublicKey(privateKeyData)
                val uncompressedKey = KeyUtil.addUncompressedByte(publicData)
                publicKeyBuilder.apply {
                    timestamp = time
                    secp256K1UncompressedBuilder.apply {
                        bytes = uncompressedKey.toByteString()
                    }.build()
                }.build()
            }.build()
        }

        fun buildFromSignedPrivateKey(signedPrivateKey: SignedPrivateKey): PrivateKey {
            return PrivateKey.newBuilder().apply {
                timestamp = signedPrivateKey.createdNs / 1_000_000
                secp256K1Builder.bytes = signedPrivateKey.secp256K1.bytes
                publicKey = PublicKeyBuilder.buildFromSignedPublicKey(signedPrivateKey.publicKey)
            }.build()
        }
    }

    fun getPrivateKey(): PrivateKey {
        return privateKey
    }

    override val address: String
        get() = privateKey.walletAddress

    override fun sign(data: ByteArray): SignatureOuterClass.Signature {
        val signatureData =
            Sign.signMessage(
                data,
                ECKeyPair.create(privateKey.secp256K1.bytes.toByteArray()),
                false,
            )
        val signature = SignatureOuterClass.Signature.newBuilder()
        val signatureKey = KeyUtil.getSignatureBytes(signatureData)
        signature.ecdsaCompactBuilder.apply {
            bytes = signatureKey.take(64).toByteArray().toByteString()
            recovery = signatureKey[64].toInt()
        }.build()
        return signature.build()
    }

    override fun sign(message: String): SignatureOuterClass.Signature {
        val digest = Signature.newBuilder().build().ethHash(message)
        return sign(digest)
    }
}

fun PrivateKey.matches(publicKey: PublicKey): Boolean =
    publicKey.recoverKeySignedPublicKey() == (publicKey.recoverKeySignedPublicKey())

fun PrivateKey.generate(): PrivateKey {
    return PrivateKeyBuilder.buildFromPrivateKeyData(SecureRandom().generateSeed(32))
}

val PrivateKey.walletAddress: String
    get() = publicKey.walletAddress

fun PrivateKey.sign(key: PublicKeyOuterClass.UnsignedPublicKey): PublicKeyOuterClass.SignedPublicKey {
    val bytes = key.toByteArray()
    val signedPublicKey = PublicKeyOuterClass.SignedPublicKey.newBuilder()
    val signature = PrivateKeyBuilder(this).sign(Hash.sha256(bytes))
    signedPublicKey.signature = signature
    signedPublicKey.keyBytes = bytes.toByteString()
    return signedPublicKey.build()
}
