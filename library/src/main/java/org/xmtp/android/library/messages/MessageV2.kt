package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.web3j.crypto.ECDSASignature
import org.web3j.crypto.Hash
import org.web3j.crypto.Sign
import org.xmtp.android.library.CipherText
import org.xmtp.android.library.Client
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.EncodedContent
import java.math.BigInteger
import java.util.Date

typealias MessageV2 = org.xmtp.proto.message.contents.MessageOuterClass.MessageV2

class MessageV2Builder {
    companion object {
        fun buildFromCipherText(headerBytes: ByteArray, ciphertext: CipherText?): MessageV2 {
            return MessageV2.newBuilder().also {
                it.headerBytes = headerBytes.toByteString()
                it.ciphertext = ciphertext
            }.build()
        }

        fun buildDecode(message: MessageV2, keyMaterial: ByteArray): DecodedMessage {
            val decrypted =
                Crypto.decrypt(keyMaterial, message.ciphertext, message.headerBytes.toByteArray())
            val signed = SignedContent.parseFrom(decrypted)

            if (!signed.sender.hasPreKey() || !signed.sender.hasIdentityKey()) {
                throw XMTPException("missing sender pre-key or identity key")
            }

            val senderPreKey = PublicKeyBuilder.buildFromSignedPublicKey(signed.sender.preKey)
            val senderIdentityKey =
                PublicKeyBuilder.buildFromSignedPublicKey(signed.sender.identityKey)

            if (!senderPreKey.signature.verify(
                    senderIdentityKey,
                    signed.sender.preKey.keyBytes.toByteArray()
                )
            ) {
                throw XMTPException("pre-key not signed by identity key")
            }

            // Verify content signature
            val digest =
                Hash.sha256(message.headerBytes.toByteArray() + signed.payload.toByteArray())

            val signatureData =
                KeyUtil.getSignatureData(signed.signature.rawData.toByteString().toByteArray())
            val publicKey = Sign.recoverFromSignature(
                BigInteger(1, signatureData.v).toInt(),
                ECDSASignature(BigInteger(1, signatureData.r), BigInteger(1, signatureData.s)),
                digest,
            )

            val key = PublicKey.newBuilder().also {
                it.secp256K1UncompressedBuilder.bytes =
                    KeyUtil.addUncompressedByte(publicKey.toByteArray()).toByteString()
            }.build()

            if (key.walletAddress != (PublicKeyBuilder.buildFromSignedPublicKey(signed.sender.preKey).walletAddress)) {
                throw throw XMTPException("Invalid signature")
            }
            val encodedMessage = EncodedContent.parseFrom(signed.payload)
            val header = MessageHeaderV2.parseFrom(message.headerBytes)
            return DecodedMessage(
                encodedContent = encodedMessage,
                senderAddress = signed.sender.walletAddress,
                sent = Date(header.createdNs / 1_000_000)
            )
        }

        fun buildEncode(
            client: Client,
            encodedContent: EncodedContent,
            topic: String,
            keyMaterial: ByteArray
        ): MessageV2 {
            val payload = encodedContent.toByteArray()
            val date = Date()
            val header = MessageHeaderV2Builder.buildFromTopic(topic, date)
            val headerBytes = header.toByteArray()
            val digest = Hash.sha256(headerBytes + payload)
            val preKey = client.keys.preKeysList?.get(0)
            val signature = preKey?.sign(digest)
            val bundle = client.privateKeyBundleV1.toV2().getPublicKeyBundle()
            val signedContent = SignedContentBuilder.builderFromPayload(payload, bundle, signature)
            val signedBytes = signedContent.toByteArray()
            val ciphertext = Crypto.encrypt(keyMaterial, signedBytes, additionalData = headerBytes)
            return buildFromCipherText(headerBytes, ciphertext)
        }
    }
}
