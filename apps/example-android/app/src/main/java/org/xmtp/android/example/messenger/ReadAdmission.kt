package org.xmtp.android.example.messenger

/** The second guard runs after both local queries complete. */
suspend fun markLocalRead(
    eligible: () -> Boolean,
    latestIncoming: suspend () -> Long?,
    currentMarker: suspend () -> Long?,
    saveMarker: suspend (Long) -> Unit,
) {
    if (!eligible()) return
    val latest = latestIncoming() ?: return
    val current = currentMarker()
    if (!eligible()) return
    if (current == null || latest > current) saveMarker(latest)
}
