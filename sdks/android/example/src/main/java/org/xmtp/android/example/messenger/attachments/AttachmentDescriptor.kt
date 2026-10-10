package org.xmtp.android.example.messenger.attachments

import uniffi.xmtp_sdk.RemoteAttachment
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream

/** Versioned bytes for a record in SecureSecretStore. */
internal object AttachmentDescriptor {
    fun encode(remote: RemoteAttachment): ByteArray =
        ByteArrayOutputStream()
            .also { bytes ->
                DataOutputStream(bytes).use { out ->
                    out.writeInt(1)
                    out.writeUTF(remote.url)
                    out.writeUTF(remote.contentDigest)
                    listOf(remote.secret, remote.salt, remote.nonce).forEach {
                        out.writeInt(it.size)
                        out.write(it)
                    }
                    out.writeUTF(remote.scheme)
                    out.writeBoolean(remote.contentLength != null)
                    remote.contentLength?.let { out.writeLong(it.toLong()) }
                    out.writeBoolean(remote.filename != null)
                    remote.filename?.let(out::writeUTF)
                }
            }.toByteArray()

    fun decode(bytes: ByteArray): RemoteAttachment =
        DataInputStream(ByteArrayInputStream(bytes)).use { input ->
            require(input.readInt() == 1) { "Unsupported attachment draft version" }
            val url = input.readUTF()
            val digest = input.readUTF()
            val keys =
                listOf(32, 32, 12).map { expected ->
                    require(input.readInt() == expected) { "Invalid attachment draft key" }
                    ByteArray(expected).also(input::readFully)
                }
            val scheme = input.readUTF()
            val length =
                if (input.readBoolean()) {
                    input
                        .readLong()
                        .also {
                            require(it in 0..UInt.MAX_VALUE.toLong())
                        }.toUInt()
                } else {
                    null
                }
            val filename = if (input.readBoolean()) input.readUTF() else null
            require(input.available() == 0) { "Invalid attachment draft bytes" }
            RemoteAttachment(url, digest, keys[0], keys[1], keys[2], scheme, length, filename)
        }
}
