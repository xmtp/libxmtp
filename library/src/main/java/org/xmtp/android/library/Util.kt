package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.jcajce.provider.digest.Keccak
import org.web3j.utils.Numeric
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import uniffi.xmtpv3.FfiEnvelope

class Util {
    companion object {
        fun keccak256(data: ByteArray): ByteArray {
            val digest256 = Keccak.Digest256()
            return digest256.digest(data)
        }

        fun envelopeFromFFi(envelope: FfiEnvelope): MessageApiOuterClass.Envelope {
            return MessageApiOuterClass.Envelope.newBuilder().also {
                it.contentTopic = envelope.contentTopic
                it.timestampNs = envelope.timestampNs.toLong()
                it.message = envelope.message.toByteString()
            }.build()
        }
    }
}

fun ByteArray.toHex(): String = joinToString(separator = "") { eachByte -> "%02x".format(eachByte) }

fun String.hexToByteArray(): ByteArray = Numeric.hexStringToByteArray(this)
