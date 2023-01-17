package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.crypto.digests.SHA256Digest
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.SigningKey
import org.xmtp.proto.message.contents.PublicKeyOuterClass
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.security.SecureRandom

typealias PrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKey

class PrivateKeyBuilder : SigningKey {
    constructor(key: PrivateKey) {
        privateKey = key
    }

    constructor() {
        privateKey = PrivateKey.newBuilder().apply {
            val time = System.currentTimeMillis()
            timestamp = time
            val privateKeyData = SecureRandom().generateSeed(32)
            secp256K1Builder.bytes = privateKeyData.toByteString()
            val publicData = ECKeyPair.create(privateKeyData)
            publicKeyBuilder.apply {
                timestamp = time
                secp256K1UncompressedBuilder.apply {
                    bytes = publicData.publicKey.toByteArray().toByteString()
                }.build()
            }.build()
        }.build()
    }

    companion object {
        lateinit var privateKey: PrivateKey

        fun buildFromPrivateKey(privateKeyData: ByteArray): PrivateKey {
            privateKey = PrivateKey.newBuilder().apply {
                val time = System.currentTimeMillis()
                timestamp = time
                secp256K1Builder.bytes = privateKeyData.toByteString()
                val publicData = ECKeyPair.create(privateKeyData)
                val uncompressedKey = byteArrayOf(0x4.toByte()) + publicData.publicKey.toByteArray()
                publicKeyBuilder.apply {
                    timestamp = time
                    secp256K1UncompressedBuilder.apply {
                        bytes = uncompressedKey.toByteString()
                    }.build()
                }.build()
            }.build()
            return privateKey
        }
    }

    fun setPrivateKey(key: PrivateKey) {
        privateKey = key
    }

    override val address: String
        get() = privateKey.walletAddress

    override fun sign(data: ByteArray): Signature {
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

    override fun sign(message: String): Signature {
        val digest = Signature.newBuilder().build().ethHash(message)
        return sign(digest)
    }
}

fun PrivateKey.matches(publicKey: PublicKey): Boolean =
    publicKey.recoverKeySignedPublicKey() == (publicKey.recoverKeySignedPublicKey())

fun PrivateKey.generate(): PrivateKey {
    return PrivateKeyBuilder.buildFromPrivateKey(SecureRandom().generateSeed(32))
}

val PrivateKey.walletAddress: String
    get() = publicKey.walletAddress

fun PrivateKey.sign(key: PublicKeyOuterClass.UnsignedPublicKey): PublicKeyOuterClass.SignedPublicKey {
    val bytes = key.secp256K1Uncompressed.bytes
    val digest = SHA256Digest(bytes.toByteArray()).encodedState
    val signedPublicKey = PublicKeyOuterClass.SignedPublicKey.newBuilder()
    val signature = PrivateKeyBuilder().sign(digest)
    signedPublicKey.signature = signature
    signedPublicKey.keyBytes = bytes
    return signedPublicKey.build()
}
