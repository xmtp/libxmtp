package org.xmtp.android.library

import org.web3j.crypto.Sign.SignatureData

object KeyUtil {
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
}
