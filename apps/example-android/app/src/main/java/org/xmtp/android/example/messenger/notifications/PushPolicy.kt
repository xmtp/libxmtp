package org.xmtp.android.example.messenger.notifications

import uniffi.xmtp_sdk.Topic
import uniffi.xmtp_sdk.XmtpException
import uniffi.xmtp_sdk.parsePushHint

data class PushEnvelope(
    val topic: String,
    val sequence: ULong,
    val kind: PushKind,
    val identifier: String,
) {
    val tag: String get() = "$topic:$sequence"
}

enum class PushKind { GROUP, WELCOME }

/**
 * The SDK checks the topic encoding and length and the sequence range. Push subscriptions
 * carry only group and welcome topics (PUSH-215), so any other kind is ignored.
 */
fun parsePush(data: Map<String, String>): PushEnvelope? {
    val topic = data["topic"] ?: return null
    val sequence = data["sequence_id"] ?: return null
    val hint =
        try {
            parsePushHint(topic, sequence)
        } catch (_: XmtpException) {
            return null
        }
    return when (val parsed = hint.topic) {
        is Topic.GroupMessages -> PushEnvelope(topic, hint.sequenceId, PushKind.GROUP, parsed.conversationId)
        is Topic.WelcomeMessages -> PushEnvelope(topic, hint.sequenceId, PushKind.WELCOME, parsed.installationId)
        else -> null
    }
}

data class PushOwner(
    val profile: String,
    val generation: Long,
    val installation: String,
)

data class PushConversation(
    val route: String,
    val allowed: Boolean,
    val enabled: Boolean,
)

data class PushRoute(
    val profile: String,
    val conversation: String?,
)

/** Reads current local state through the process session owner. */
interface PushAdmission {
    fun current(): PushOwner?

    suspend fun enabled(owner: PushOwner): Boolean

    suspend fun conversation(
        owner: PushOwner,
        source: String,
    ): PushConversation?

    /** Recheck privacy state, then fence ownership and post in one synchronous operation. */
    suspend fun postIfCurrent(
        owner: PushOwner,
        envelope: PushEnvelope,
        route: PushRoute,
    ): Boolean
}

class PushHandler(
    private val admission: PushAdmission,
) {
    suspend fun receive(data: Map<String, String>): Boolean {
        val envelope = parsePush(data) ?: return false
        val owner = admission.current() ?: return false
        if (!admission.enabled(owner)) return false
        val conversation =
            when (envelope.kind) {
                PushKind.GROUP -> {
                    admission.conversation(owner, envelope.identifier)?.takeIf { it.allowed && it.enabled }?.route
                        ?: return false
                }

                PushKind.WELCOME -> {
                    if (envelope.identifier != owner.installation) return false
                    null
                }
            }
        if (admission.current() != owner || !admission.enabled(owner)) return false
        // Recheck consent and mute after the other suspended reads.
        if (envelope.kind == PushKind.GROUP) {
            val fresh = admission.conversation(owner, envelope.identifier) ?: return false
            if (!fresh.allowed || !fresh.enabled || fresh.route != conversation) return false
        }
        return admission.postIfCurrent(owner, envelope, PushRoute(owner.profile, conversation))
    }
}

enum class RegistrationAction { NONE, DISABLE, ENABLE }

data class RegistrationInput(
    val configured: Boolean,
    val signedIn: Boolean,
    val permission: Boolean,
    val appEnabled: Boolean,
    val token: String?,
)

/** Unconfigured builds never enable notification registration. */
fun registrationAction(
    input: RegistrationInput,
    registeredToken: String?,
    sdkEnabled: Boolean,
): RegistrationAction =
    when {
        !input.configured -> {
            RegistrationAction.NONE
        }

        !input.signedIn || !input.permission || !input.appEnabled || input.token.isNullOrBlank() -> {
            if (sdkEnabled ||
                registeredToken != null
            ) {
                RegistrationAction.DISABLE
            } else {
                RegistrationAction.NONE
            }
        }

        !sdkEnabled || registeredToken != input.token -> {
            RegistrationAction.ENABLE
        }

        else -> {
            RegistrationAction.NONE
        }
    }
