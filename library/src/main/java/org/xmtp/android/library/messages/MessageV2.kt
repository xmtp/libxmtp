package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import org.web3j.crypto.ECDSASignature
import org.web3j.crypto.Hash
import org.web3j.crypto.Sign
import org.xmtp.android.library.CipherText
import org.xmtp.android.library.Client
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import java.math.BigInteger
import java.util.Date

typealias MessageV2 = org.xmtp.proto.message.contents.MessageOuterClass.MessageV2

class MessageV2Builder(val senderHmac: ByteArray? = null, val shouldPush: Boolean = false) {
    lateinit var messageV2: MessageV2

    companion object {
        fun buildFromCipherText(
            headerBytes: ByteArray,
            ciphertext: CipherText?,
            senderHmac: ByteArray,
            shouldPush: Boolean,
        ): MessageV2Builder {
            val messageBuilder = MessageV2Builder(senderHmac = senderHmac, shouldPush = shouldPush)
            messageBuilder.messageV2 = MessageV2.newBuilder().also {
                it.headerBytes = headerBytes.toByteString()
                it.ciphertext = ciphertext
                it.shouldPush = shouldPush
                it.senderHmac = senderHmac.toByteString()
            }.build()
            return messageBuilder
        }

        fun buildDecode(
            id: String,
            topic: String,
            message: MessageV2,
            keyMaterial: ByteArray,
            client: Client,
        ): DecodedMessage {
            try {
                val decryptedMessage = buildDecrypt(id, topic, message, keyMaterial, client)

                return DecodedMessage(
                    id = id,
                    client = client,
                    topic = decryptedMessage.topic,
                    encodedContent = decryptedMessage.encodedContent,
                    senderAddress = decryptedMessage.senderAddress,
                    sent = decryptedMessage.sentAt,
                )
            } catch (e: Exception) {
                throw XMTPException("Error decoding message", e)
            }
        }

        fun buildDecrypt(
            id: String,
            topic: String,
            message: MessageV2,
            keyMaterial: ByteArray,
            client: Client,
        ): DecryptedMessage {
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
                    signed.sender.preKey.keyBytes.toByteArray(),
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
                it.secp256K1Uncompressed = it.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                    keyBuilder.bytes =
                        KeyUtil.addUncompressedByte(publicKey.toByteArray()).toByteString()
                }.build()
            }.build()

            if (key.walletAddress != (PublicKeyBuilder.buildFromSignedPublicKey(signed.sender.preKey).walletAddress)) {
                throw XMTPException("Invalid signature")
            }

            val encodedMessage = EncodedContent.parseFrom(signed.payload)
            val header = MessageHeaderV2.parseFrom(message.headerBytes)
            if (header.topic != topic) {
                throw XMTPException("Topic mismatch")
            }

            return DecryptedMessage(
                id = id,
                encodedContent = encodedMessage,
                senderAddress = signed.sender.walletAddress,
                sentAt = Date(header.createdNs / 1_000_000),
                topic = topic,
            )
        }

        private fun <Codec : ContentCodec<T>, T> shouldPush(codec: Codec, content: T?): Boolean {
            if (content != null) {
                return codec.shouldPush(content = content)
            } else {
                throw XMTPException("Codec invalid content")
            }
        }

        fun <Codec : ContentCodec<T>, T> buildEncode(
            client: Client,
            encodedContent: EncodedContent,
            topic: String,
            keyMaterial: ByteArray,
            codec: Codec,
        ): MessageV2Builder {
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

            val thirtyDayPeriodsSinceEpoch =
                (Date().time / 1000 / 60 / 60 / 24 / 30).toInt()
            val info = "$thirtyDayPeriodsSinceEpoch-${client.address}"
            val infoEncoded = info.toByteStringUtf8().toByteArray()
            val senderHmacGenerated =
                Crypto.calculateMac(
                    Crypto.deriveKey(keyMaterial, ByteArray(0), infoEncoded),
                    headerBytes
                )

            return buildFromCipherText(
                headerBytes,
                ciphertext,
                senderHmacGenerated,
                shouldPush(codec = codec, content = codec.decode(encodedContent)),
            )
        }
    }
}
