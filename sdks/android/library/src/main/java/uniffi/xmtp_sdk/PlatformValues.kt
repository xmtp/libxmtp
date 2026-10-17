package uniffi.xmtp_sdk

/** Return lowercase hexadecimal text without a prefix. */
fun ByteArray.toHex(): String = joinToString(separator = "") { eachByte -> "%02x".format(eachByte) }

/** Keep the retained hexadecimal conversion, including an odd first digit. */
fun String.hexToByteArray(): ByteArray {
    val text = if (startsWith("0x")) substring(2) else this
    val bytes = ByteArray((text.length + 1) / 2)
    val first = text.length % 2
    if (first == 1) bytes[0] = Character.digit(text[0], 16).toByte()
    for (index in first until text.length step 2) {
        bytes[(index + 1) / 2] =
            ((Character.digit(text[index], 16) shl 4) + Character.digit(text[index + 1], 16)).toByte()
    }
    return bytes
}

/** Reject the retained forbidden prefix. Native operations validate the full ID. */
fun validateInboxId(inboxId: InboxId) {
    if (inboxId.startsWith("0x", ignoreCase = true)) {
        throw XmtpException.InvalidArgument(
            ErrorDetails(
                "InvalidArgument",
                ErrorCategory.INPUT,
                false,
                "Invalid inboxId: $inboxId. Inbox IDs cannot start with '0x'.",
            ),
        )
    }
}

/** Apply the retained prefix check to each inbox ID. */
fun validateInboxIds(inboxIds: List<InboxId>) {
    inboxIds.forEach(::validateInboxId)
}
