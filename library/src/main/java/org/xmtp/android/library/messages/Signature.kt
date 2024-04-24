package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Util
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.SignatureOuterClass

typealias Signature = org.xmtp.proto.message.contents.SignatureOuterClass.Signature

private const val MESSAGE_PREFIX = "\u0019Ethereum Signed Message:\n"

class SignatureBuilder {
    companion object {
        fun buildFromSignatureData(data: ByteArray): Signature {
            return Signature.newBuilder().also {
                it.ecdsaCompact = it.ecdsaCompact.toBuilder().also { builder ->
                    builder.bytes = data.take(64).toByteArray().toByteString()
                    builder.recovery = data[64].toInt()
                }.build()
            }.build()
        }
    }
}

fun Signature.ethHash(message: String): ByteArray {
    val input = MESSAGE_PREFIX + message.length + message
    return Util.keccak256(input.toByteArray())
}

/**
 * This is the text that users sign when they want to create
 * an identity key associated with their wallet.
 * @param key bytes contains an unsigned [xmtp.PublicKey] of the identity key to be created.
 * @return The resulting signature is then published to prove that the
 * identity key is authorized on behalf of the wallet.
 */
fun Signature.createIdentityText(key: ByteArray): String =
    ("XMTP : Create Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

/**
 * This is the text that users sign when they want to save (encrypt)
 * or to load (decrypt) keys using the network private storage.
 * @param key bytes contains the `walletPreKey` of the encrypted bundle.
 * @return The resulting signature is the shared secret used to encrypt and
 * decrypt the saved keys.
 */
fun Signature.enableIdentityText(key: ByteArray): String =
    ("XMTP : Enable Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

fun Signature.consentProofText(peerAddress: String, timestamp: Long): String =
    ("XMTP : Grant inbox consent to sender\n" + "\n" + "Current Time: ${timestamp}\n" + "From Address: ${peerAddress}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

val Signature.rawData: ByteArray
    get() = if (hasEcdsaCompact()) {
        ecdsaCompact.bytes.toByteArray() + ecdsaCompact.recovery.toByte()
    } else {
        walletEcdsaCompact.bytes.toByteArray() + walletEcdsaCompact.recovery.toByte()
    }

val Signature.rawDataWithNormalizedRecovery: ByteArray
    get() {
        val data = rawData
        if (data[64] == 0.toByte()) {
            data[64] = 27.toByte()
        } else if (data[64] == 1.toByte()) {
            data[64] = 28.toByte()
        }
        return data
    }

@OptIn(ExperimentalUnsignedTypes::class)
fun Signature.verify(signedBy: PublicKey, digest: ByteArray): Boolean {
    return try {
        uniffi.xmtpv3.verifyK256Sha256(
            signedBy.secp256K1Uncompressed.bytes.toByteArray(),
            digest,
            ecdsaCompact.bytes.toByteArray(),
            ecdsaCompact.recovery.toUByte()
        )
    } catch (e: Exception) {
        false
    }
}

fun Signature.ensureWalletSignature(): Signature {
    return when (unionCase) {
        SignatureOuterClass.Signature.UnionCase.ECDSA_COMPACT -> {
            val walletEcdsa = SignatureOuterClass.Signature.WalletECDSACompact.newBuilder().also {
                it.bytes = ecdsaCompact.bytes
                it.recovery = ecdsaCompact.recovery
            }.build()
            this.toBuilder().also {
                it.walletEcdsaCompact = walletEcdsa
            }.build()
        }

        else -> this
    }
}
