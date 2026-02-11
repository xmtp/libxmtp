package org.xmtp.android.example.extension

fun String.truncatedAddress(): String {
    if (length > 6) {
        val start = 6
        val end = lastIndex - 3
        return replaceRange(start, end, "...")
    }
    return this
}
