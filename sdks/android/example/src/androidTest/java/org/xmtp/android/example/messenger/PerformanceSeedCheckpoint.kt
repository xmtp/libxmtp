package org.xmtp.android.example.messenger

/** Stop fixture sends until the receiver has processed the published text count. */
internal suspend fun verifySeedCheckpoint(
    expected: ULong,
    sync: suspend () -> Unit,
    count: suspend () -> ULong,
): ULong {
    sync()
    val received = count()
    check(received == expected) { "Seed checkpoint expected $expected Published texts, received $received" }
    return received
}
