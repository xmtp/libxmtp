package org.xmtp.android.example.shared.feature.handles

/** The longest name part of a handle, the text before `#`. */
const val MAX_HANDLE_NAME_LENGTH = 27

/** The minimum a client uses when the server publishes none. */
const val DEFAULT_HANDLE_MIN_NAME_LENGTH = 3

/** The number of suggestions the editor shows at first, and adds for each "Show more". */
const val HANDLE_SUGGESTION_PAGE = 3

/** The number of claims one inbox can make. Apps learn that it is used up from the server. */
const val MAX_HANDLE_CLAIMS = 64

/** [CHOOSE] is the onboarding step after Join a server. [CHANGE] is the Settings screen. */
enum class HandleMode { CHOOSE, CHANGE }

/**
 * Input problems. The app checks them before it asks the server; the server decides the rest.
 * [CURRENT] is the inbox's current handle, which the editor finds when the user asks for a check.
 */
enum class HandleInputProblem { INVALID_CHARACTERS, START_WITH_LETTER, TOO_LONG, TOO_SHORT, BAD_CODE, CURRENT }

/** The name part of a handle: the text before `#`. */
fun handleName(handle: String): String = handle.substringBefore('#')

/** The code of a handle, with its `#`, or an empty string. */
fun handleCode(handle: String): String = handle.substring(handleName(handle).length)

/**
 * The input after the SDK's own conversion: ASCII capitals become lowercase.
 * No other character changes, so the server sees what the field shows.
 */
fun normalizedHandleInput(input: String): String =
    buildString(input.length) {
        input.forEach { append(if (it in 'A'..'Z') it + ('a' - 'A') else it) }
    }

/**
 * The first rule that [input] breaks, or null. The grammar is the protocol's, so
 * every server agrees. [minNameLength] is the server's published minimum: an app
 * may check input against it, but only the server decides a claim.
 */
fun handleInputProblem(
    input: String,
    minNameLength: Int,
): HandleInputProblem? {
    if (input.isEmpty()) return null
    val name = handleName(input)
    val code = handleCode(input)
    return when {
        name.any { it !in 'a'..'z' && it !in '0'..'9' && it != '_' && it != '-' } -> {
            HandleInputProblem.INVALID_CHARACTERS
        }

        name.firstOrNull()?.let { it in 'a'..'z' } != true -> {
            HandleInputProblem.START_WITH_LETTER
        }

        name.length > MAX_HANDLE_NAME_LENGTH -> {
            HandleInputProblem.TOO_LONG
        }

        name.length < minNameLength -> {
            HandleInputProblem.TOO_SHORT
        }

        code.isNotEmpty() && (code.length != 5 || code.drop(1).any { it !in '0'..'9' }) -> {
            HandleInputProblem.BAD_CODE
        }

        else -> {
            null
        }
    }
}

/** What the editor shows under the field. */
sealed interface HandleStatus {
    /** Nothing is checked yet, or the input changed after a check. */
    data object Editing : HandleStatus

    data class Checking(
        val input: String,
    ) : HandleStatus

    data class Available(
        val handle: String,
    ) : HandleStatus

    /** [input] is taken. [handles] are free; the editor shows the first [shown]. */
    data class Suggestions(
        val input: String,
        val handles: List<String>,
        val shown: Int,
        val selected: String,
    ) : HandleStatus {
        val visible: List<String> get() = handles.take(shown)
        val more: Boolean get() = shown < handles.size
    }

    /** The server found no free handle for [input]. */
    data class NoneFree(
        val input: String,
    ) : HandleStatus
}

/** A message in an error box. Each one comes from a server answer or a failed request. */
sealed interface HandleNotice {
    /** Another inbox claimed [handle] first. The editor shows refreshed suggestions. */
    data class Taken(
        val handle: String,
    ) : HandleNotice

    /** The server refuses [name] for a new claim. */
    data class NotAllowed(
        val name: String,
    ) : HandleNotice

    /** The inbox claimed a handle less than 7 days ago. The dates are known only for claims made here. */
    data class TooSoon(
        val changedOn: String?,
        val nextOn: String?,
    ) : HandleNotice

    /** The inbox has made all its claims. */
    data object LimitReached : HandleNotice

    /** The availability check failed. Retry checks again. */
    data object CheckFailed : HandleNotice

    /** The claim of [handle] failed with no answer from the server. Retry claims it again. */
    data class ClaimFailed(
        val handle: String,
    ) : HandleNotice

    /** The server refuses a switch back to [handle] under its current rules. */
    data class SwitchBackBlocked(
        val handle: String,
        val shorterThanMinimum: Boolean,
    ) : HandleNotice
}

data class HandleState(
    val mode: HandleMode = HandleMode.CHOOSE,
    val loading: Boolean = false,
    val current: String? = null,
    val currentSince: String? = null,
    val others: List<String> = emptyList(),
    val minNameLength: Int = DEFAULT_HANDLE_MIN_NAME_LENGTH,
    val input: String = "",
    val problem: HandleInputProblem? = null,
    val status: HandleStatus = HandleStatus.Editing,
    val claiming: String? = null,
    val confirm: String? = null,
    val notice: HandleNotice? = null,
) {
    /** The server refused every new claim for this inbox. The editor stays read-only. */
    val blocked: Boolean get() = notice is HandleNotice.TooSoon || notice is HandleNotice.LimitReached

    /** The handle the primary button claims. */
    val pick: String?
        get() =
            when (val value = status) {
                is HandleStatus.Available -> value.handle
                is HandleStatus.Suggestions -> value.selected
                else -> null
            }

    val busy: Boolean get() = loading || claiming != null || status is HandleStatus.Checking

    /** The primary button checks the input when there is no pick. */
    val canCheck: Boolean
        get() =
            claiming == null && !blocked && input.isNotEmpty() && problem == null &&
                status == HandleStatus.Editing && notice !is HandleNotice.NotAllowed

    val canClaim: Boolean get() = claiming == null && !blocked && pick != null

    val editable: Boolean get() = claiming == null && !blocked
}

sealed interface HandleAction {
    data class ChangeInput(
        val value: String,
    ) : HandleAction

    /** Check the input with the server. */
    data object Check : HandleAction

    data object ShowMore : HandleAction

    data class Select(
        val handle: String,
    ) : HandleAction

    /** Claim the pick. In [HandleMode.CHANGE] this asks for confirmation first. */
    data object Claim : HandleAction

    /** Ask to claim an older handle of this inbox again. */
    data class SwitchBack(
        val handle: String,
    ) : HandleAction

    data object ConfirmClaim : HandleAction

    data object CancelClaim : HandleAction

    /** Repeat the request that failed. */
    data object Retry : HandleAction

    data object Skip : HandleAction
}

/** The copy of the approved designs, in one place for the screens and the tests. */
object HandleCopy {
    const val TITLE = "Choose your handle."
    const val INTRO = "Your handle is public. People on this server use it to find you and send you messages."
    const val CARD_TITLE = "Your handle"
    const val FIELD_LABEL = "Handle"
    const val PLACEHOLDER = "nick"
    const val SKIP = "Skip for now"
    const val SHOW_MORE = "Show more"
    const val LOOK_AGAIN = "Look again"
    const val RETRY = "Retry"
    const val CHOOSE_FOOTER = "You can change your handle once every 7 days."
    const val CHANGE_FOOTER = "You can change your handle once every 7 days, up to 64 times in total."
    const val CURRENT_TITLE = "Current handle"
    const val NEW_TITLE = "New handle"
    const val OTHERS_TITLE = "Your other handles"
    const val OTHERS_TEXT =
        "They are still yours, and people can still find you by them. Switching back counts as a change."
    const val OTHERS_LIMIT_TEXT = "People can still find you by any of them."
    const val USE = "Use"
    const val CHOOSE_SCREEN = "Choose handle"
    const val CHANGE_SCREEN = "Change handle"
    const val NOT_ALLOWED_FIELD = "Choose a different name."
    const val CHECK_FAILED = "Cannot check this handle. Try again."
    const val LIMIT_REACHED =
        "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles."

    /** The Settings screen title: Choose handle for an inbox with no handle, else Change handle. */
    fun screenTitle(state: HandleState): String = if (state.current == null) CHOOSE_SCREEN else CHANGE_SCREEN

    fun supporting(minNameLength: Int): String =
        if (minNameLength >= MAX_HANDLE_NAME_LENGTH) {
            "$MAX_HANDLE_NAME_LENGTH lowercase letters, numbers, _ or -."
        } else {
            "$minNameLength to $MAX_HANDLE_NAME_LENGTH lowercase letters, numbers, _ or -."
        }

    fun problem(
        problem: HandleInputProblem,
        input: String,
        minNameLength: Int,
    ): String =
        when (problem) {
            HandleInputProblem.INVALID_CHARACTERS -> "Use only lowercase letters, numbers, _ and -."
            HandleInputProblem.START_WITH_LETTER -> "Start with a letter."
            HandleInputProblem.TOO_LONG -> "Use $MAX_HANDLE_NAME_LENGTH characters or fewer before #."
            HandleInputProblem.TOO_SHORT -> "Use at least $minNameLength characters on this server."
            HandleInputProblem.BAD_CODE -> "After #, use exactly 4 digits, like ${handleName(input)}#0012."
            HandleInputProblem.CURRENT -> "This is your current handle."
        }

    /** The character counter that [HandleInputProblem.TOO_LONG] shows. */
    fun counter(input: String): String = "${handleName(input).length}/$MAX_HANDLE_NAME_LENGTH"

    fun checking(input: String) = "Checking $input…"

    fun available(handle: String) = "$handle is available."

    fun suggestions(input: String) = "$input is taken. These handles are free:"

    fun noneFree(input: String) = "$input and the codes we checked are all taken. Look again, or try a different name."

    fun notice(notice: HandleNotice): String =
        when (notice) {
            is HandleNotice.Taken -> {
                "Someone claimed ${notice.handle} a moment before you. Pick another handle."
            }

            is HandleNotice.NotAllowed -> {
                "${notice.name} is not allowed on this server."
            }

            is HandleNotice.TooSoon -> {
                if (notice.changedOn != null && notice.nextOn != null) {
                    "You changed your handle on ${notice.changedOn}. You can change it again on ${notice.nextOn}."
                } else {
                    "You changed your handle less than 7 days ago. Try again later."
                }
            }

            HandleNotice.LimitReached -> {
                LIMIT_REACHED
            }

            HandleNotice.CheckFailed -> {
                CHECK_FAILED
            }

            is HandleNotice.ClaimFailed -> {
                "Cannot claim ${notice.handle}. Try again."
            }

            is HandleNotice.SwitchBackBlocked -> {
                if (notice.shorterThanMinimum) {
                    "${notice.handle} is shorter than this server now allows. " +
                        "It still finds you, but you cannot switch back to it."
                } else {
                    "${notice.handle} is not allowed on this server now. " +
                        "It still finds you, but you cannot switch back to it."
                }
            }
        }

    /** The primary button label. */
    fun primary(state: HandleState): String {
        val change = state.mode == HandleMode.CHANGE && state.current != null
        state.claiming?.let { return if (change) "Changing to $it…" else "Claiming $it…" }
        state.pick?.let { return if (change) "Change to $it" else "Claim $it" }
        return if (change) CHANGE_SCREEN else "Continue"
    }

    fun confirmTitle(state: HandleState): String =
        if (state.current == null) "Claim this handle?" else "Change your handle?"

    fun confirmText(
        state: HandleState,
        handle: String,
    ): String =
        if (state.current == null) {
            "People will see $handle as your handle. You cannot change your handle again for 7 days."
        } else {
            "People will see $handle as your handle. ${state.current} stays yours, and people can still find you " +
                "by it. You cannot change your handle again for 7 days."
        }

    fun confirmButton(state: HandleState): String = if (state.current == null) "Claim" else "Change"

    fun since(day: String) = "Yours since $day"

    fun more(count: Int) = "and $count more"
}
