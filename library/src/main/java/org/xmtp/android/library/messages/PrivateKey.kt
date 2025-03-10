package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.util.Arrays
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.SignedData
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.toHex
import java.security.SecureRandom
import java.util.Date

typealias PrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKey
typealias PublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKey

class PrivateKeyBuilder : SigningKey {
    private var privateKey: PrivateKey

    constructor() {
        privateKey = generatePrivateKey()
    }

    constructor(key: PrivateKey) {
        privateKey = key
    }

    companion object {
        fun buildFromPrivateKeyData(privateKeyData: ByteArray): PrivateKey {
            val time = Date().time
            val publicData = KeyUtil.getPublicKey(privateKeyData)
            val uncompressedKey = KeyUtil.addUncompressedByte(publicData)

            return PrivateKey.newBuilder().apply {
                timestamp = time
                secp256K1 = secp256K1.toBuilder().setBytes(privateKeyData.toByteString()).build()
                publicKey = publicKey.toBuilder().apply {
                    this.timestamp = time
                    secp256K1Uncompressed = secp256K1Uncompressed.toBuilder()
                        .setBytes(uncompressedKey.toByteString()).build()
                }.build()
            }.build()
        }
    }

    private fun generatePrivateKey(): PrivateKey {
        val privateKeyData = SecureRandom().generateSeed(32)
        return buildFromPrivateKeyData(privateKeyData)
    }

    fun getPrivateKey(): PrivateKey = privateKey

    override val publicIdentity: PublicIdentity
        get() = PublicIdentity(IdentityKind.ETHEREUM, privateKey.walletAddress)

    override suspend fun sign(message: String): SignedData {
        val digest = KeyUtil.ethHash(message) // Hashes the message properly
        val ecKeyPair = ECKeyPair.create(privateKey.secp256K1.bytes.toByteArray())
        val signatureData = Sign.signMessage(digest, ecKeyPair, false)

        val fullSignature = KeyUtil.getSignatureBytes(signatureData)

        return SignedData(
            rawData = fullSignature,
            publicKey = privateKey.publicKey.secp256K1Uncompressed.bytes.toByteArray(),
            authenticatorData = null,
            clientDataJson = null
        )
    }
}

val PrivateKey.walletAddress: String
    get() = publicKey.walletAddress.lowercase()

val PublicKey.walletAddress: String
    get() {
        val address = Keys.getAddress(
            Arrays.copyOfRange(
                secp256K1Uncompressed.bytes.toByteArray(),
                1,
                secp256K1Uncompressed.bytes.toByteArray().size
            )
        )
        return Keys.toChecksumAddress(address.toHex()).lowercase()
    }
