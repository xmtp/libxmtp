package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import org.web3j.crypto.ECDSASignature
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.web3j.crypto.Sign.SignatureData
import org.web3j.utils.Numeric
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.consentProofText
import org.xmtp.android.library.messages.rawData
import java.math.BigInteger

object KeyUtil {
    fun getPublicKey(privateKey: ByteArray): ByteArray {
        return Sign.publicKeyFromPrivate(BigInteger(1, privateKey)).toByteArray()
    }

    private fun recoverPublicKeyKeccak256(signature: ByteArray, digest: ByteArray): BigInteger? {
        val signatureData = getSignatureData(signature)
        return Sign.recoverFromSignature(
            BigInteger(1, signatureData.v).toInt(),
            ECDSASignature(BigInteger(1, signatureData.r), BigInteger(1, signatureData.s)),
            digest,
        )
    }

    private fun publicKeyToAddress(publicKey: BigInteger): String {
        return Keys.toChecksumAddress(Keys.getAddress(publicKey))
    }

    fun addUncompressedByte(publicKey: ByteArray): ByteArray {
        return if (publicKey.size >= 65) {
            val newPublicKey = ByteArray(64)
            System.arraycopy(publicKey, publicKey.size - 64, newPublicKey, 0, 64)
            byteArrayOf(0x4.toByte()) + newPublicKey
        } else if (publicKey.size < 64) {
            val newPublicKey = ByteArray(64)
            System.arraycopy(publicKey, 0, newPublicKey, 64 - publicKey.size, publicKey.size)
            byteArrayOf(0x4.toByte()) + newPublicKey
        } else {
            byteArrayOf(0x4.toByte()) + publicKey
        }
    }

    fun getSignatureData(signatureBytes: ByteArray): SignatureData {
        val v = signatureBytes[64]
        if (v < 27) {
            (v.plus(27))
        }
        val r = signatureBytes.copyOfRange(0, 32)
        val s = signatureBytes.copyOfRange(32, 64)
        return SignatureData(v, r, s)
    }

    fun getSignatureBytes(sig: SignatureData): ByteArray {
        val v = sig.v[0]
        val fixedV = if (v >= 27) (v - 27).toByte() else v
        return merge(
            sig.r,
            sig.s,
            byteArrayOf(fixedV),
        )
    }

    private fun merge(vararg arrays: ByteArray): ByteArray {
        var arrCount = 0
        var count = 0
        for (array in arrays) {
            arrCount++
            count += array.size
        }

        // Create new array and copy all array contents
        val mergedArray = ByteArray(count)
        var start = 0
        for (array in arrays) {
            System.arraycopy(array, 0, mergedArray, start, array.size)
            start += array.size
        }
        return mergedArray
    }

    fun validateConsentSignature(signature: String, clientAddress: String, peerAddress: String, timestamp: Long): Boolean {
        val messageData = Signature.newBuilder().build().consentProofText(peerAddress, timestamp).toByteArray()
        val signatureData = Numeric.hexStringToByteArray(signature)
        val sig = Signature.parseFrom(signatureData)
        val recoveredPublicKey = recoverPublicKeyKeccak256(sig.rawData.toByteString().toByteArray(), Util.keccak256(messageData))
            ?: return false
        return clientAddress == publicKeyToAddress(recoveredPublicKey)
    }
}
