package org.xmtp.android.library

import uniffi.xmtpv3.FfiNotificationChannel
import uniffi.xmtpv3.FfiNotificationConfig
import uniffi.xmtpv3.FfiNotificationFailure
import uniffi.xmtpv3.FfiNotificationOverride
import uniffi.xmtpv3.FfiNotificationState

/** Delivery channel for this installation. */
sealed class NotificationChannel {
    class Apns(
        val token: String,
    ) : NotificationChannel()

    class Fcm(
        val token: String,
    ) : NotificationChannel()

    class Http(
        val url: String,
        val signingKey: ByteArray,
    ) : NotificationChannel()

    internal fun toFfi(): FfiNotificationChannel =
        when (this) {
            is Apns -> FfiNotificationChannel.Apns(token)
            is Fcm -> FfiNotificationChannel.Fcm(token)
            is Http -> FfiNotificationChannel.Http(url, signingKey)
        }
}

/** Notification delivery and conversation rules. */
class NotificationConfig(
    val channel: NotificationChannel,
    val consentStates: List<ConsentState> = listOf(ConsentState.ALLOWED),
    val includeWelcomes: Boolean = true,
    val includeSyncGroups: Boolean = false,
    val includeCommits: Boolean = false,
) {
    internal fun toFfi(): FfiNotificationConfig =
        FfiNotificationConfig(
            channel = channel.toFfi(),
            consentStates = consentStates.map { ConsentState.toFfiConsentState(it) },
            includeWelcomes = includeWelcomes,
            includeSyncGroups = includeSyncGroups,
            includeCommits = includeCommits,
        )
}

/** Reset to the configured consent rules with Default. */
enum class NotificationOverride {
    Enabled,
    Disabled,
    Default,
    ;

    internal fun toFfi(): FfiNotificationOverride =
        when (this) {
            Enabled -> FfiNotificationOverride.ENABLED
            Disabled -> FfiNotificationOverride.DISABLED
            Default -> FfiNotificationOverride.DEFAULT
        }
}

/** A notification failure with a stable error code. */
class NotificationError internal constructor(
    val code: String,
    cause: Throwable? = null,
) : Exception(code, cause) {
    companion object {
        internal fun from(failure: FfiNotificationFailure): NotificationError =
            NotificationError(
                "NotificationError::" +
                    when (failure) {
                        FfiNotificationFailure.PERMISSION_DENIED -> "PermissionDenied"
                        FfiNotificationFailure.INVALID_ARGUMENT -> "InvalidArgument"
                        FfiNotificationFailure.OUT_OF_RANGE -> "OutOfRange"
                        FfiNotificationFailure.UNIMPLEMENTED -> "Unimplemented"
                        FfiNotificationFailure.CHANNEL_NOT_CONFIGURED -> "ChannelNotConfigured"
                    },
            )

        internal fun from(error: Exception): Exception {
            val code = Regex("^\\[(NotificationError::[^]]+)]").find(error.message.orEmpty())?.groupValues?.get(1)
            return if (code == null) error else NotificationError(code, error)
        }
    }
}

/** Local notification state. Reading it makes no backend request. */
sealed class NotificationState {
    object Disabled : NotificationState()

    object Enabled : NotificationState()

    class Failed(
        val error: NotificationError,
    ) : NotificationState()

    companion object {
        internal fun fromFfi(value: FfiNotificationState): NotificationState =
            when (value) {
                FfiNotificationState.Disabled -> Disabled
                FfiNotificationState.Enabled -> Enabled
                is FfiNotificationState.Failed -> Failed(NotificationError.from(value.error))
            }
    }
}
