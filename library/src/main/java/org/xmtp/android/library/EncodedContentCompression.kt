package org.xmtp.android.library

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.DeflaterOutputStream
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream
import java.util.zip.InflaterOutputStream

enum class EncodedContentCompression {
    DEFLATE,
    GZIP;

    fun compress(content: ByteArray): ByteArray? {
        return when (this) {
            DEFLATE -> {
                ByteArrayOutputStream(content.size).use { bos ->
                    DeflaterOutputStream(bos).use { deflater ->
                        deflater.write(content)
                        deflater.close()
                        bos.toByteArray()
                    }
                }
            }
            GZIP -> {
                ByteArrayOutputStream(content.size).use { bos ->
                    GZIPOutputStream(bos).use { gzipOS ->
                        gzipOS.write(content)
                        gzipOS.close()
                        bos.toByteArray()
                    }
                }
            }
        }
    }

    fun decompress(content: ByteArray): ByteArray? {
        return when (this) {
            DEFLATE -> {
                val bos = ByteArrayOutputStream()
                InflaterOutputStream(bos).write(content)
                bos.toByteArray()
            }
            GZIP -> {
                ByteArrayInputStream(content).use { bis ->
                    ByteArrayOutputStream().use { bos ->
                        GZIPInputStream(bis).use { gzipIS ->
                            val buffer = ByteArray(1024)
                            var len: Int
                            while (gzipIS.read(buffer).also { len = it } != -1) {
                                bos.write(buffer, 0, len)
                            }
                            bos.toByteArray()
                        }
                    }
                }
            }
        }
    }
}
