package org.xmtp.android.example.messenger
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.nio.file.Files
import java.nio.file.NoSuchFileException
import java.nio.file.attribute.BasicFileAttributes
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Each record is encrypted before it enters the private file store. */
class SecureSecretStore(
    private val context: Context,
) {
    private fun file(
        profileId: String,
        ref: String,
    ): File {
        require(
            profileId
                .matches(Regex("[a-zA-Z0-9-]+")) &&
                ref
                    .matches(Regex("[a-zA-Z0-9-]+")),
        )
        return File(
            File(
                context.filesDir,
                "messenger/profiles/$profileId/secrets",
            ),
            ref,
        )
    }

    private fun key(): SecretKey {
        val store =
            KeyStore
                .getInstance("AndroidKeyStore")
                .apply {
                    load(null)
                }
        (
            store
                .getKey(
                    "messenger-secrets-v1",
                    null,
                ) as? SecretKey
        )?.let {
            return it
        }
        return KeyGenerator
            .getInstance(
                KeyProperties.KEY_ALGORITHM_AES,
                "AndroidKeyStore",
            ).apply {
                init(
                    KeyGenParameterSpec
                        .Builder(
                            "messenger-secrets-v1",
                            KeyProperties
                                .PURPOSE_ENCRYPT or
                                KeyProperties.PURPOSE_DECRYPT,
                        ).setBlockModes(
                            KeyProperties.BLOCK_MODE_GCM,
                        ).setEncryptionPaddings(
                            KeyProperties.ENCRYPTION_PADDING_NONE,
                        ).build(),
                )
            }.generateKey()
    }

    @Synchronized fun write(
        profileId: String,
        ref: String,
        value: ByteArray,
    ) {
        val cipher =
            Cipher
                .getInstance("AES/GCM/NoPadding")
                .apply {
                    init(
                        Cipher.ENCRYPT_MODE,
                        key(),
                    )
                }
        cipher
            .updateAAD(
                "$profileId/$ref"
                    .toByteArray(),
            )
        val target =
            file(
                profileId,
                ref,
            )
        check(
            target.parentFile!!
                .isDirectory ||
                target.parentFile!!
                    .mkdirs(),
        )
        val atomic = AtomicFile(target)
        val stream =
            atomic
                .startWrite()
        try {
            stream
                .write(
                    cipher.iv.size,
                )
            stream
                .write(
                    cipher.iv,
                )
            stream
                .write(
                    cipher
                        .doFinal(value),
                )
            atomic
                .finishWrite(stream)
        } catch (error: Throwable) {
            atomic
                .failWrite(stream)
            throw error
        }
    }

    @Synchronized fun read(
        profileId: String,
        ref: String,
    ): ByteArray? {
        val target =
            file(
                profileId,
                ref,
            )
        val atomic = AtomicFile(target)

        fun present(file: File): Boolean =
            try {
                Files
                    .readAttributes(
                        file
                            .toPath(),
                        BasicFileAttributes::class.java,
                    )
                true
            } catch (_: NoSuchFileException) {
                false
            }
        if (!present(target) && !present(File("${target.path}.bak"))) return null
        val bytes =
            atomic
                .readFully()
        val size =
            bytes[0]
                .toInt() and 255
        require(
            size == 12 && bytes.size > size + 1,
        )
        val cipher =
            Cipher
                .getInstance("AES/GCM/NoPadding")
                .apply {
                    init(
                        Cipher.DECRYPT_MODE,
                        key(),
                        GCMParameterSpec(
                            128,
                            bytes
                                .copyOfRange(
                                    1,
                                    size + 1,
                                ),
                        ),
                    )
                    updateAAD(
                        "$profileId/$ref"
                            .toByteArray(),
                    )
                }
        return cipher
            .doFinal(
                bytes
                    .copyOfRange(
                        size + 1,
                        bytes.size,
                    ),
            )
    }

    @Synchronized fun delete(
        profileId: String,
        ref: String,
    ) = AtomicFile(
        file(
            profileId,
            ref,
        ),
    ).delete()
}
