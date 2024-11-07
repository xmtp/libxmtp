package org.xmtp.android.library

import org.bouncycastle.jcajce.provider.digest.Keccak
import org.web3j.utils.Numeric

class Util {
    companion object {
        fun keccak256(data: ByteArray): ByteArray {
            val digest256 = Keccak.Digest256()
            return digest256.digest(data)
        }
    }
}

fun ByteArray.toHex(): String = joinToString(separator = "") { eachByte -> "%02x".format(eachByte) }

fun String.hexToByteArray(): ByteArray = Numeric.hexStringToByteArray(this)
