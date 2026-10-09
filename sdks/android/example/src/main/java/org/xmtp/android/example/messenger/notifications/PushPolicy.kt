package org.xmtp.android.example.messenger.notifications

import java.util.Base64

data class PushEnvelope(val topic: String, val sequence: ULong, val kind: PushKind, val identifier: String) {
    val tag: String get() = "$topic:$sequence"
}

enum class PushKind { GROUP, WELCOME }

/** PUSH topics contain a kind byte and a fixed length identifier. */
fun parsePush(data: Map<String, String>): PushEnvelope? {
    val topic = data["topic"] ?: return null
    val sequence = data["sequence_id"] ?: return null
    if (sequence.isEmpty() || sequence.any { it !in '0'..'9' }) return null
    val number = sequence.toULongOrNull() ?: return null
    val bytes = try { Base64.getDecoder().decode(topic) } catch (_: IllegalArgumentException) { return null }
    if (Base64.getEncoder().encodeToString(bytes) != topic) return null
    val kind = when {
        bytes.size == 17 && bytes[0] == 0.toByte() -> PushKind.GROUP
        bytes.size == 33 && bytes[0] == 1.toByte() -> PushKind.WELCOME
        else -> return null
    }
    val identifier = bytes.drop(1).joinToString("") { "%02x".format(it.toInt() and 255) }
    return PushEnvelope(topic, number, kind, identifier)
}

data class PushOwner(val profile: String, val generation: Long, val installation: String)
data class PushConversation(val route: String, val allowed: Boolean, val enabled: Boolean)
data class PushRoute(val profile: String, val conversation: String?)

/** Reads current local state through the process session owner. */
interface PushAdmission {
    fun current(): PushOwner?
    suspend fun enabled(owner: PushOwner): Boolean
    suspend fun conversation(owner: PushOwner, source: String): PushConversation?
    /** The host must check ownership and post in one synchronous operation. */
    fun postIfCurrent(owner: PushOwner, envelope: PushEnvelope, route: PushRoute): Boolean
}

class PushHandler(private val admission: PushAdmission) {
    suspend fun receive(data: Map<String, String>): Boolean {
        val envelope = parsePush(data) ?: return false
        val owner = admission.current() ?: return false
        if (!admission.enabled(owner)) return false
        val conversation = when (envelope.kind) {
            PushKind.GROUP -> admission.conversation(owner, envelope.identifier)?.takeIf { it.allowed && it.enabled }?.route ?: return false
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
data class RegistrationInput(val configured: Boolean, val signedIn: Boolean, val permission: Boolean, val appEnabled: Boolean, val token: String?)

/** Unconfigured builds do not call Firebase or the SDK notification API. */
fun registrationAction(input: RegistrationInput, registeredToken: String?, sdkEnabled: Boolean): RegistrationAction = when {
    !input.configured -> RegistrationAction.NONE
    !input.signedIn || !input.permission || !input.appEnabled || input.token.isNullOrBlank() -> if (sdkEnabled || registeredToken != null) RegistrationAction.DISABLE else RegistrationAction.NONE
    !sdkEnabled || registeredToken != input.token -> RegistrationAction.ENABLE
    else -> RegistrationAction.NONE
}
