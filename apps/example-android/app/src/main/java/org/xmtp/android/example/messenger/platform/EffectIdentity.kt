package org.xmtp.android.example.messenger.platform

import org.xmtp.android.example.messenger.SessionKey

/** Identifies one platform request from one session and destination. */
data class EffectIdentity(
    val requestId: Long,
    val sessionKey: SessionKey,
    val destinationToken: Long,
)
