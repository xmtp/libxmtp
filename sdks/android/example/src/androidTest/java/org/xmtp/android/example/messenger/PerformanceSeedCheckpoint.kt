package org.xmtp.android.example.messenger

import kotlinx.coroutines.withTimeout

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

/** Keep readiness failures distinct without changing the existing deadline. */
internal suspend fun <T> awaitPerformanceReady(
    wait: suspend () -> T,
    report: (String, Throwable?) -> Unit,
): T {
    report("start", null)
    try {
        val value = withTimeout(120_000) { wait() }
        report("complete", null)
        return value
    } catch (error: Throwable) {
        report("failed", error)
        throw error
    }
}

/** Report exception classes only. Discard all message text and unknown values. */
internal fun reportedFailureClass(value: String?): String? {
    if (value == null) return null
    val name =
        Regex("^([A-Za-z_$][A-Za-z0-9_.$]*)(?=:|$)")
            .find(value)
            ?.groupValues
            ?.get(1)
            ?.substringAfterLast('.')
    return name?.takeIf { it.endsWith("Exception") || it.endsWith("Error") || it.startsWith("XmtpException$") }
        ?: "REPORTED_ERROR"
}
