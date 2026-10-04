import uniffi.xmtp_sdk.*

// verifies: CTYPE-015, CTYPE-026
fun checkEncryptedRemoteAttachmentProjection() {
    fun encrypted() =
        EncryptedEncodedContent(
            "ciphertext".toByteArray(),
            EncryptionKeys(ByteArray(32) { 1 }, ByteArray(32) { 2 }, ByteArray(12) { 3 }, "wrong digest", 999uL),
        )
    val records =
        listOf(
            "https://example.org/file?signature=value",
            "http://localhost/file",
            "http://127.0.0.1/file",
            "http://[::1]/file",
        ).map { url ->
            val record = remoteAttachmentFromEncrypted(url, encrypted(), "file")
            check(record.url == url)
            check(record.contentDigest == "305531dcc50ebca31cf1d5b31e9fc76ed51f66b3b6dd5a030c6539ae6532f979")
            check(record.contentLength == 10u)
            check(record.secret.contentEquals(ByteArray(32) { 1 }))
            check(record.salt.contentEquals(ByteArray(32) { 2 }))
            check(record.nonce.contentEquals(ByteArray(12) { 3 }))
            check(record.scheme == if (url.startsWith("https:")) "https://" else "http://")
            check(record.filename == "file")
            record
        }
    val optional = remoteAttachmentFromEncrypted("https://example.org/optional", encrypted(), null)
    check(optional.filename == null)
    val codec = MultiRemoteAttachmentCodec()
    val nested = MultiRemoteAttachment(records + optional)
    check(codec.decode(codec.encode(nested)) == nested)
    for (url in listOf("not a url", "ftp://example.org/file", "http://example.org/file")) {
        check(
            runCatching {
                remoteAttachmentFromEncrypted(url, encrypted(), null)
            }.exceptionOrNull() is XmtpException.InvalidArgument,
        )
    }
    println("Kotlin encrypted remote projection preserves ciphertext fields and shared URL policy")
}
