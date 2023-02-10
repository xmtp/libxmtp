package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.CipherText
import org.xmtp.android.library.Crypto
import org.xmtp.proto.message.contents.MessageOuterClass
import java.util.Date

typealias MessageV1 = org.xmtp.proto.message.contents.MessageOuterClass.MessageV1

class MessageV1Builder {
    companion object {
        fun buildEncode(
            sender: PrivateKeyBundleV1,
            recipient: PublicKeyBundle,
            message: ByteArray,
            timestamp: Date
        ): MessageV1 {
            val secret = sender.sharedSecret(
                peer = recipient,
                myPreKey = sender.preKeysList[0].publicKey,
                isRecipient = false
            )
            val header = MessageHeaderV1Builder.buildFromPublicBundles(
                sender = sender.toPublicKeyBundle(),
                recipient = recipient,
                timestamp = timestamp.time
            )
            val headerBytes = header.toByteArray()
            val ciphertext = Crypto.encrypt(secret, message, additionalData = headerBytes)
            return buildFromCipherText(headerBytes = headerBytes, ciphertext = ciphertext)
        }

        fun buildFromBytes(bytes: ByteArray): MessageV1 {
            val message = Message.parseFrom(bytes)
            val headerBytes: ByteArray
            val ciphertext: CipherText
            when (message.versionCase) {
                MessageOuterClass.Message.VersionCase.V1 -> {
                    headerBytes = message.v1.headerBytes.toByteArray()
                    ciphertext = message.v1.ciphertext
                }
                MessageOuterClass.Message.VersionCase.V2 -> {
                    headerBytes = message.v2.headerBytes.toByteArray()
                    ciphertext = message.v2.ciphertext
                }
                else -> throw IllegalArgumentException("Cannot decode from bytes")
            }
            return buildFromCipherText(headerBytes, ciphertext)
        }

        fun buildFromCipherText(headerBytes: ByteArray, ciphertext: CipherText?): MessageV1 {
            return MessageV1.newBuilder().also {
                it.headerBytes = headerBytes.toByteString()
                it.ciphertext = ciphertext
            }.build()
        }
    }
}

val MessageV1.header: MessageHeaderV1
    get() = MessageHeaderV1.parseFrom(headerBytes)

val MessageV1.senderAddress: String
    get() = header.sender.identityKey.recoverWalletSignerPublicKey().walletAddress

val MessageV1.sentAt: Date get() = Date(header.timestamp)

val MessageV1.recipientAddress: String
    get() = header.recipient.identityKey.recoverWalletSignerPublicKey().walletAddress

fun MessageV1.decrypt(viewer: PrivateKeyBundleV1?): ByteArray? {
    val header = MessageHeaderV1.parseFrom(headerBytes)
    val recipient = header.recipient
    val sender = header.sender
    val secret: ByteArray = if (viewer?.walletAddress == sender.walletAddress) {
        viewer.sharedSecret(peer = recipient, myPreKey = sender.preKey, isRecipient = false)
    } else {
        viewer?.sharedSecret(peer = sender, myPreKey = recipient.preKey, isRecipient = true)
            ?: byteArrayOf()
    }
    return Crypto.decrypt(secret, ciphertext, additionalData = headerBytes.toByteArray())
}
