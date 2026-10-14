package org.xmtp.android.example.messenger.feature

import kotlinx.coroutines.CancellationException
import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.FailureRecovery

class FeatureInputFailure(
    val userMessage: String,
) : IllegalArgumentException(userMessage)

fun featureFailure(
    error: Throwable,
    message: String,
    code: String,
    recovery: FailureRecovery = FailureRecovery.RETRY,
): AppFailure {
    if (error is CancellationException) throw error
    return AppFailure((error as? FeatureInputFailure)?.userMessage ?: message, code, recovery)
}
