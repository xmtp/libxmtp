package org.xmtp.android.example.messenger.feature.handles

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.shared.feature.handles.*
import uniffi.xmtp_sdk.ErrorCategory
import uniffi.xmtp_sdk.ErrorDetails
import uniffi.xmtp_sdk.XmtpException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.Continuation
import kotlin.coroutines.resume
import kotlin.coroutines.suspendCoroutine

/** Every editor state of the approved designs, driven through the controller with a fake account. */
class HandleControllerTest {
    private val day = TimeUnit.DAYS.toMillis(1)
    private var clock = 100 * day

    private class FakeAccount(
        var current: String? = null,
        var others: List<String> = emptyList(),
        var minimum: Int = DEFAULT_HANDLE_MIN_NAME_LENGTH,
    ) : HandleAccount {
        val lookups = mutableListOf<String>()
        val ownedQueries = mutableListOf<List<String>>()
        val claims = mutableListOf<String>()
        val times = mutableMapOf<String, Long>()
        var finished = 0
        var held = emptySet<String>()
        var available: suspend (String) -> List<String> = { emptyList() }
        var claim: suspend (String) -> Unit = {}
        var ownedFailure: (() -> Throwable)? = null
        var readFailure: (() -> Throwable)? = null

        val reads = mutableListOf<Boolean>()

        override suspend fun handles(refresh: Boolean): OwnHandles {
            reads += refresh
            readFailure?.let { throw it() }
            return OwnHandles(current, others)
        }

        override fun minNameLength() = minimum

        override suspend fun availableHandles(name: String): List<String> {
            lookups += name
            return available(name)
        }

        override suspend fun owned(handles: List<String>): Set<String> {
            ownedQueries += handles
            ownedFailure?.let { throw it() }
            return handles.filterTo(mutableSetOf()) { it in held }
        }

        override suspend fun claimHandle(handle: String) {
            claims += handle
            claim(handle)
        }

        override fun claimTimes(): Map<String, Long> = times

        override fun recordClaim(
            handle: String,
            atMs: Long,
        ) {
            times[handle] = atMs
        }

        override fun finish() {
            finished++
        }
    }

    private fun details(code: String) = ErrorDetails(code, ErrorCategory.IDENTITY, false, code)

    private fun taken() = XmtpException.HandleTaken(details("HandleTaken"))

    private fun notAllowed() = XmtpException.HandleNotAllowed(details("HandleNotAllowed"))

    private fun tooSoon() = XmtpException.HandleTooSoon(details("HandleTooSoon"))

    private fun limit() = XmtpException.HandleLimitReached(details("HandleLimitReached"))

    private fun offline() = XmtpException.RequestTimeout(details("RequestTimeout"))

    private fun invalid() = XmtpException.InvalidArgument(details("InvalidArgument"))

    private fun open(
        account: FakeAccount,
        mode: HandleMode = HandleMode.CHOOSE,
    ) = HandleController(
        CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
        Dispatchers.Unconfined,
        now = { clock },
        day = { "day ${it / day}" },
    ).also { it.open(account, mode) }

    private fun HandleController.type(value: String) = dispatch(HandleAction.ChangeInput(value))

    private val HandleController.value get() = state.value

    @Test fun failedOwnReadsShowTheLoadErrorAndBlockEveryClaim() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nickm")).apply {
                available = { listOf(it) }
                readFailure = { offline() }
            }
        val editor = open(account, HandleMode.CHANGE)
        // Unknown is not "no handle": the error shows in place of the handles.
        assertTrue(editor.value.ownUnknown)
        assertEquals(HandleNotice.LoadFailed, editor.value.notice)
        assertEquals("Cannot load your handle. Try again.", HandleCopy.notice(editor.value.notice!!))
        // Unknown handles are not "no handle": the screen keeps the Change title.
        assertEquals("Change handle", HandleCopy.screenTitle(editor.value))
        assertFalse(editor.value.loading)
        assertFalse(editor.value.editable)
        // No edit, switch back, check, or claim goes through.
        editor.type("nicky")
        assertEquals("", editor.value.input)
        editor.dispatch(HandleAction.SwitchBack("nickm"))
        assertNull(editor.value.confirm)
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals(emptyList<String>(), account.claims)
        // Retry with a working read shows the real handles.
        account.readFailure = null
        editor.dispatch(HandleAction.Retry)
        assertFalse(editor.value.ownUnknown)
        assertNull(editor.value.notice)
        assertEquals("nick#2341", editor.value.current)
        assertEquals(listOf("nickm"), editor.value.others)
        assertTrue(editor.value.editable)
    }

    @Test fun onboardingWithFailedOwnReadsOffersNoClaim() {
        val account =
            FakeAccount().apply {
                available = { listOf(it) }
                readFailure = { offline() }
            }
        val editor = open(account)
        assertTrue(editor.value.ownUnknown)
        assertEquals(HandleNotice.LoadFailed, editor.value.notice)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals(emptyList<String>(), account.claims)
        assertEquals(0, account.finished)
        // The inbox already has a handle, from another device: the step ends with no claim.
        account.current = "nick#0412"
        account.readFailure = null
        editor.dispatch(HandleAction.Retry)
        assertEquals("nick#0412", editor.value.current)
        assertEquals(1, account.finished)
        assertEquals(emptyList<String>(), account.claims)
    }

    @Test fun emptyEditorWaitsForInput() {
        val account = FakeAccount()
        val editor = open(account)
        val state = editor.value
        assertEquals(HandleState(), state)
        assertFalse(state.canCheck)
        assertFalse(state.canClaim)
        assertEquals("Continue", HandleCopy.primary(state))
        assertEquals("3 to 27 lowercase letters, numbers, _ or -.", HandleCopy.supporting(state.minNameLength))
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals(emptyList<String>(), account.claims)
        assertEquals(0, account.finished)
    }

    @Test fun invalidGrammarShowsWhileTypingAndIsNeverSent() {
        val account = FakeAccount()
        val editor = open(account)
        val cases =
            listOf(
                "nick.eth" to HandleInputProblem.INVALID_CHARACTERS,
                "ni ck" to HandleInputProblem.INVALID_CHARACTERS,
                "nick@x" to HandleInputProblem.INVALID_CHARACTERS,
                "1nick" to HandleInputProblem.START_WITH_LETTER,
                "_nick" to HandleInputProblem.START_WITH_LETTER,
                "#0012" to HandleInputProblem.START_WITH_LETTER,
                "nicholas_the_magnificent_one" to HandleInputProblem.TOO_LONG,
                "nick#12" to HandleInputProblem.BAD_CODE,
                "nick#12345" to HandleInputProblem.BAD_CODE,
                "nick#" to HandleInputProblem.BAD_CODE,
            )
        for ((input, problem) in cases) {
            editor.type(input)
            assertEquals(input, problem, editor.value.problem)
            assertFalse(input, editor.value.canCheck)
            editor.dispatch(HandleAction.Check)
        }
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals("28/27", HandleCopy.counter("nicholas_the_magnificent_one"))
        assertEquals(
            "After #, use exactly 4 digits, like nick#0012.",
            HandleCopy.problem(HandleInputProblem.BAD_CODE, "nick#12", 3),
        )
        // The SDK lowercases ASCII capitals, so the field shows the text the server receives.
        editor.type("Nick#0012")
        assertEquals("nick#0012", editor.value.input)
        assertNull(editor.value.problem)
        editor.type("n" + "a".repeat(MAX_HANDLE_NAME_LENGTH - 1) + "#0001")
        assertNull(editor.value.problem)
        assertTrue(editor.value.canCheck)
    }

    @Test fun tooShortUsesThePublishedMinimumForTypedInputOnly() {
        val account = FakeAccount(minimum = 5)
        val editor = open(account)
        assertEquals(5, editor.value.minNameLength)
        editor.type("nick")
        assertEquals(HandleInputProblem.TOO_SHORT, editor.value.problem)
        assertEquals(
            "Use at least 5 characters on this server.",
            HandleCopy.problem(HandleInputProblem.TOO_SHORT, "nick", 5),
        )
        editor.dispatch(HandleAction.Check)
        assertEquals(emptyList<String>(), account.lookups)
        editor.type("nicky")
        assertNull(editor.value.problem)
        // A server that publishes no minimum gets the client default.
        assertEquals(DEFAULT_HANDLE_MIN_NAME_LENGTH, open(FakeAccount(minimum = 0)).value.minNameLength)
    }

    @Test fun checkingShowsUntilTheServerAnswers() {
        val answer = CompletableDeferred<List<String>>()
        val account = FakeAccount().apply { available = { answer.await() } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        assertEquals(HandleStatus.Checking("nick"), editor.value.status)
        assertTrue(editor.value.busy)
        assertFalse(editor.value.canCheck)
        assertFalse(editor.value.canClaim)
        assertEquals("Checking nick…", HandleCopy.checking("nick"))
        editor.dispatch(HandleAction.Check)
        assertEquals(listOf("nick"), account.lookups)
        answer.complete(listOf("nick"))
        assertEquals(HandleStatus.Available("nick"), editor.value.status)
    }

    @Test fun anEditWhileCheckingDropsTheOldAnswer() {
        // A lookup that ignores cancellation, as a native call can, still answers after the edit.
        var pending: Continuation<List<String>>? = null
        val account = FakeAccount().apply { available = { suspendCoroutine { pending = it } } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.type("nicky")
        checkNotNull(pending).resume(listOf("nick"))
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertEquals("nicky", editor.value.input)
    }

    @Test fun bareNameFreeIsOfferedAndClaimedOnlyWhenPicked() {
        val account = FakeAccount().apply { available = { listOf("nick", "nick#2341") } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        assertEquals(HandleStatus.Available("nick"), editor.value.status)
        assertEquals("nick is available.", HandleCopy.available("nick"))
        assertEquals("Claim nick", HandleCopy.primary(editor.value))
        assertEquals(emptyList<String>(), account.claims)
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf("nick"), account.claims)
        assertEquals(mapOf("nick" to clock), account.times)
        assertEquals(1, account.finished)
        assertEquals("nick", editor.value.current)
    }

    @Test fun claimingDisablesTheEditorUntilTheServerAnswers() {
        val answer = CompletableDeferred<Unit>()
        val account =
            FakeAccount().apply {
                available = { listOf("nick#2341", "nick#0912", "nick#5530") }
                claim = { answer.await() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals("nick#2341", editor.value.claiming)
        assertFalse(editor.value.editable)
        assertEquals("Claiming nick#2341…", HandleCopy.primary(editor.value))
        editor.type("other")
        editor.dispatch(HandleAction.Select("nick#0912"))
        editor.dispatch(HandleAction.Skip)
        assertEquals("nick", editor.value.input)
        assertEquals("nick#2341", editor.value.pick)
        assertEquals(0, account.finished)
        answer.complete(Unit)
        assertEquals(1, account.finished)
    }

    @Test fun takenNameShowsThreeSuggestionsAndShowMore() {
        val free = listOf("nick#2341", "nick#0912", "nick#5530", "nick#6418", "nick#7702", "nick#8120", "nick#9031")
        val account = FakeAccount().apply { available = { free } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        val status = editor.value.status as HandleStatus.Suggestions
        assertEquals(HandleStatus.Suggestions("nick", free, 3, "nick#2341"), status)
        assertEquals(free.take(3), status.visible)
        assertTrue(status.more)
        assertEquals("nick is taken. These handles are free:", HandleCopy.suggestions("nick"))
        assertEquals("Claim nick#2341", HandleCopy.primary(editor.value))
        // Each Show more adds 3 from the same answer, until the answer runs out.
        editor.dispatch(HandleAction.ShowMore)
        val more = editor.value.status as HandleStatus.Suggestions
        assertEquals(free.take(6), more.visible)
        assertTrue(more.more)
        editor.dispatch(HandleAction.ShowMore)
        val all = editor.value.status as HandleStatus.Suggestions
        assertEquals(free, all.visible)
        assertFalse(all.more)
        assertEquals(listOf("nick"), account.lookups)
        editor.dispatch(HandleAction.Select("nick#9031"))
        assertEquals("nick#9031", editor.value.pick)
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf("nick#9031"), account.claims)
    }

    @Test fun takenHandleWithACodeSuggestsCodesForItsName() {
        val account =
            FakeAccount().apply {
                available = { name -> if (name == "nick#1337") emptyList() else listOf("nick#2341", "nick#0912") }
            }
        val editor = open(account)
        editor.type("nick#1337")
        editor.dispatch(HandleAction.Check)
        assertEquals(listOf("nick#1337", "nick"), account.lookups)
        assertEquals(
            HandleStatus.Suggestions("nick#1337", listOf("nick#2341", "nick#0912"), 2, "nick#2341"),
            editor.value.status,
        )
    }

    @Test fun noSuggestionsOfferLookAgain() {
        val account = FakeAccount().apply { available = { emptyList() } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        assertEquals(HandleStatus.NoneFree("nick"), editor.value.status)
        assertFalse(editor.value.canClaim)
        assertFalse(editor.value.canCheck)
        assertEquals(
            "nick and the codes we checked are all taken. Look again, or try a different name.",
            HandleCopy.noneFree("nick"),
        )
        account.available = { listOf("nick#4410") }
        editor.dispatch(HandleAction.Check)
        assertEquals(listOf("nick", "nick"), account.lookups)
        assertEquals("nick#4410", editor.value.pick)
    }

    @Test fun takenDuringClaimReturnsToRefreshedSuggestionsAndWaitsForThePick() {
        val free = listOf("nick#2341", "nick#0912", "nick#5530", "nick#6418")
        val account =
            FakeAccount().apply {
                available = { free }
                claim = { if (it == "nick#2341") throw taken() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        // The refreshed suggestions come back with the next one selected; nothing else is claimed.
        assertEquals(listOf("nick#2341"), account.claims)
        assertEquals(listOf(free.drop(1)), account.ownedQueries)
        assertEquals(
            HandleStatus.Suggestions("nick", free.drop(1), 3, "nick#0912"),
            editor.value.status,
        )
        assertEquals(HandleNotice.Taken("nick#2341"), editor.value.notice)
        assertNull(editor.value.claiming)
        assertEquals(0, account.finished)
        assertEquals(
            "Someone claimed nick#2341 a moment before you. Pick another handle.",
            HandleCopy.notice(editor.value.notice!!),
        )
        // The user picks again.
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf("nick#2341", "nick#0912"), account.claims)
        assertEquals(1, account.finished)
    }

    @Test fun takenDuringClaimDropsSuggestionsThatAreNoLongerFree() {
        val free = listOf("nick#2341", "nick#0912", "nick#5530", "nick#6418", "nick#7702")
        val account =
            FakeAccount().apply {
                available = { free }
                claim = { throw taken() }
                held = setOf("nick#5530")
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Select("nick#0912"))
        editor.dispatch(HandleAction.Claim)
        val expected = listOf("nick#2341", "nick#6418", "nick#7702")
        assertEquals(HandleStatus.Suggestions("nick", expected, 3, "nick#6418"), editor.value.status)
        assertEquals(listOf("nick"), account.lookups)
    }

    @Test fun takenBareNameRefreshesWithNewCodes() {
        var calls = 0
        val account =
            FakeAccount().apply {
                // The second answer still names the bare name free, as a lagging replica can.
                available = { if (calls++ == 0) listOf("nick") else listOf("nick", "nick#3001", "nick#3002") }
                claim = { throw taken() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf("nick", "nick"), account.lookups)
        assertEquals(
            HandleStatus.Suggestions("nick", listOf("nick#3001", "nick#3002"), 2, "nick#3001"),
            editor.value.status,
        )
        assertEquals(HandleNotice.Taken("nick"), editor.value.notice)
        assertEquals(listOf("nick"), account.claims)
    }

    @Test fun takenWithAFailedRecheckShowsTheNetworkErrorAndRetryChecksAgain() {
        val free = listOf("nick#2341", "nick#0912", "nick#5530", "nick#6418")
        val fresh = listOf("nick#5530", "nick#6418", "nick#7001")
        val account =
            FakeAccount().apply {
                available = { if (lookups.size == 1) free else fresh }
                claim = { throw taken() }
                ownedFailure = { offline() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        // The earlier suggestions were not checked again, so none of them shows.
        assertEquals(HandleNotice.CheckFailed, editor.value.notice)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertNull(editor.value.claiming)
        assertEquals("Cannot check this handle. Try again.", HandleCopy.notice(editor.value.notice!!))
        // Retry asks the server again, and shows only its fresh answer. Nothing is claimed.
        account.ownedFailure = null
        editor.dispatch(HandleAction.Retry)
        assertEquals(listOf("nick", "nick"), account.lookups)
        assertEquals(HandleStatus.Suggestions("nick", fresh, 3, "nick#5530"), editor.value.status)
        assertNull(editor.value.notice)
        assertEquals(listOf("nick#2341"), account.claims)
    }

    @Test fun takenWithAFailedFreshLookupShowsTheNetworkError() {
        var calls = 0
        val account =
            FakeAccount().apply {
                available = { if (calls++ == 0) listOf("nick") else throw offline() }
                claim = { throw taken() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(HandleNotice.CheckFailed, editor.value.notice)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertEquals(listOf("nick"), account.claims)
    }

    @Test fun notAllowedAsksForAnotherName() {
        val account =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = { throw notAllowed() }
            }
        val editor = open(account)
        editor.type("admin")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf("admin"), account.claims)
        assertEquals(HandleNotice.NotAllowed("admin"), editor.value.notice)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertFalse(editor.value.canCheck)
        assertFalse(editor.value.canClaim)
        assertEquals("admin is not allowed on this server.", HandleCopy.notice(editor.value.notice!!))
        editor.type("admins")
        assertNull(editor.value.notice)
        assertTrue(editor.value.canCheck)
    }

    @Test fun tooSoonShowsWhenTheUserCanChangeAgain() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nick#0912")).apply {
                times["nick#2341"] = clock - 3 * day
                available = { listOf(it) }
                claim = { throw tooSoon() }
            }
        val editor = open(account, HandleMode.CHANGE)
        assertEquals("day 97", editor.value.currentSince)
        editor.type("nicky")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.TooSoon("day 97", "day 104"), editor.value.notice)
        assertTrue(editor.value.blocked)
        assertFalse(editor.value.editable)
        assertEquals(
            "You changed your handle on day 97. You can change it again on day 104.",
            HandleCopy.notice(editor.value.notice!!),
        )
        editor.type("other")
        editor.dispatch(HandleAction.SwitchBack("nick#0912"))
        assertEquals("", editor.value.input)
        assertNull(editor.value.confirm)
        assertEquals(listOf("nicky"), account.claims)
        // A claim from another device has no date on this one.
        val elsewhere =
            FakeAccount(current = "nick#2341", others = listOf("nick#0912")).apply { claim = { throw tooSoon() } }
        val other = open(elsewhere, HandleMode.CHANGE)
        other.dispatch(HandleAction.SwitchBack("nick#0912"))
        other.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.TooSoon(null, null), other.value.notice)
        assertEquals(
            "You changed your handle less than 7 days ago. Try again later.",
            HandleCopy.notice(other.value.notice!!),
        )
        // This device's last claim is older than the interval, so another device made the newest one.
        val stale =
            FakeAccount(current = "nick#2341", others = listOf("nick#0912")).apply {
                times["nick#2341"] = clock - 10 * day
                claim = { throw tooSoon() }
            }
        val third = open(stale, HandleMode.CHANGE)
        third.dispatch(HandleAction.SwitchBack("nick#0912"))
        third.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.TooSoon(null, null), third.value.notice)
    }

    @Test fun tooSoonFirstClaimReadsTheInboxAgain() {
        // This device sees no handle, but another device claimed one: the first claim is too soon.
        val account =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = {
                    current = "nick#0412"
                    throw tooSoon()
                }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(listOf(false, true, true), account.reads)
        assertEquals("nick#0412", editor.value.current)
        assertEquals(HandleNotice.TooSoon(null, null), editor.value.notice)
        // The inbox has a handle, so onboarding ends with no other claim.
        assertEquals(1, account.finished)
        assertEquals(listOf("nick"), account.claims)
        // The network read still shows no handle: the editor stays blocked and says why.
        val stale =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = { throw limit() }
            }
        val other = open(stale)
        other.type("nick")
        other.dispatch(HandleAction.Check)
        other.dispatch(HandleAction.Claim)
        assertEquals(listOf(false, true, true), stale.reads)
        assertNull(other.value.current)
        assertEquals(HandleNotice.LimitReached, other.value.notice)
        assertFalse(other.value.editable)
        assertEquals(0, stale.finished)
    }

    @Test fun tooSoonRetryOfAClaimThatLandedIsASuccess() {
        // The first claim landed, but its transport error and the read after it hid that.
        var landed = false
        val account =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = {
                    if (landed) throw tooSoon()
                    landed = true
                    throw offline()
                }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        account.readFailure = { offline() }
        editor.dispatch(HandleAction.Claim)
        assertEquals(HandleNotice.ClaimFailed("nick"), editor.value.notice)
        // The retry is refused as too soon, and the network read now shows the claim.
        account.readFailure = null
        account.current = "nick"
        editor.dispatch(HandleAction.Retry)
        assertEquals(listOf("nick", "nick"), account.claims)
        assertNull(editor.value.notice)
        assertEquals("nick", editor.value.current)
        assertEquals(1, account.finished)
    }

    @Test fun claimLimitReachedBlocksEveryChange() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nick#0912", "nickm")).apply {
                claim = { throw limit() }
            }
        val editor = open(account, HandleMode.CHANGE)
        editor.dispatch(HandleAction.SwitchBack("nickm"))
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.LimitReached, editor.value.notice)
        assertTrue(editor.value.blocked)
        assertEquals(HandleCopy.LIMIT_REACHED, HandleCopy.notice(editor.value.notice!!))
        editor.dispatch(HandleAction.SwitchBack("nick#0912"))
        assertNull(editor.value.confirm)
        assertEquals(listOf("nickm"), account.claims)
    }

    @Test fun networkErrorOffersRetry() {
        var down = true
        val account = FakeAccount().apply { available = { if (down) throw offline() else listOf("nick") } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        assertEquals(HandleNotice.CheckFailed, editor.value.notice)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertTrue(editor.value.canCheck)
        assertEquals("Cannot check this handle. Try again.", HandleCopy.notice(editor.value.notice!!))
        down = false
        editor.dispatch(HandleAction.Retry)
        assertNull(editor.value.notice)
        assertEquals(HandleStatus.Available("nick"), editor.value.status)
    }

    @Test fun inputTheSdkRejectsShowsAsAFieldError() {
        val account = FakeAccount().apply { available = { throw invalid() } }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        // The input is wrong, so Retry would fail again: the field asks for another name.
        assertEquals(HandleInputProblem.INVALID_CHARACTERS, editor.value.problem)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertNull(editor.value.notice)
        assertFalse(editor.value.canCheck)
    }

    @Test fun claimWithNoAnswerChecksTheLogBeforeItOffersRetry() {
        var fail = true
        val account =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = { if (fail) throw offline() }
            }
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        assertEquals(HandleNotice.ClaimFailed("nick"), editor.value.notice)
        assertNull(editor.value.claiming)
        assertEquals(0, account.finished)
        fail = false
        editor.dispatch(HandleAction.Retry)
        assertEquals(listOf("nick", "nick"), account.claims)
        assertEquals(1, account.finished)
        // A claim that landed before the failure is a success, and is not sent again.
        val landed =
            FakeAccount().apply {
                available = { listOf(it) }
                claim = {
                    current = it
                    throw offline()
                }
            }
        val second = open(landed)
        second.type("nick")
        second.dispatch(HandleAction.Check)
        second.dispatch(HandleAction.Claim)
        assertEquals(listOf("nick"), landed.claims)
        assertEquals(1, landed.finished)
        assertNull(second.value.notice)
        assertEquals(setOf("nick"), landed.times.keys)
    }

    @Test fun changeWithNoAnswerChecksTheLogBeforeItOffersRetry() {
        // The change landed before the failure: a success, with no retry.
        val landed =
            FakeAccount(current = "nick#2341").apply {
                available = { listOf(it) }
                claim = {
                    others = listOfNotNull(current)
                    current = it
                    throw offline()
                }
            }
        val editor = open(landed, HandleMode.CHANGE)
        editor.type("nicky")
        editor.dispatch(HandleAction.Check)
        editor.dispatch(HandleAction.Claim)
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(listOf("nicky"), landed.claims)
        assertNull(editor.value.notice)
        assertEquals("nicky", editor.value.current)
        assertEquals(1, landed.finished)
        // The change did not land: the error shows, and the user can try again.
        val lost =
            FakeAccount(current = "nick#2341").apply {
                available = { listOf(it) }
                claim = { throw offline() }
            }
        val other = open(lost, HandleMode.CHANGE)
        other.type("nicky")
        other.dispatch(HandleAction.Check)
        other.dispatch(HandleAction.Claim)
        other.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.ClaimFailed("nicky"), other.value.notice)
        assertEquals("nick#2341", other.value.current)
        assertEquals(0, lost.finished)
    }

    @Test fun switchBackThatFailsTheCurrentRulesShowsWhy() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nickm", "nk"), minimum = 5).apply {
                claim = { throw notAllowed() }
            }
        val editor = open(account, HandleMode.CHANGE)
        editor.dispatch(HandleAction.SwitchBack("nk"))
        assertEquals("nk", editor.value.confirm)
        editor.dispatch(HandleAction.ConfirmClaim)
        // The server decides: the app sends the claim even though nk is shorter than the published minimum.
        assertEquals(listOf("nk"), account.claims)
        assertEquals(HandleNotice.SwitchBackBlocked("nk", shorterThanMinimum = true), editor.value.notice)
        assertEquals(
            "nk is shorter than this server now allows. It still finds you, but you cannot switch back to it.",
            HandleCopy.notice(editor.value.notice!!),
        )
        assertTrue(editor.value.editable)
        // A name the server refuses for another reason says so.
        editor.dispatch(HandleAction.SwitchBack("nickm"))
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(HandleNotice.SwitchBackBlocked("nickm", shorterThanMinimum = false), editor.value.notice)
        assertEquals(
            "nickm is not allowed on this server now. It still finds you, but you cannot switch back to it.",
            HandleCopy.notice(editor.value.notice!!),
        )
    }

    @Test fun changeAsksForConfirmationBeforeItClaims() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nick#0912")).apply { available = { listOf(it) } }
        val editor = open(account, HandleMode.CHANGE)
        editor.type("nicky")
        editor.dispatch(HandleAction.Check)
        assertEquals("Change to nicky", HandleCopy.primary(editor.value))
        editor.dispatch(HandleAction.Claim)
        assertEquals("nicky", editor.value.confirm)
        assertEquals(
            "People will see nicky as your handle. nick#2341 stays yours, and people can still find you by it. " +
                "You cannot change your handle again for 7 days.",
            HandleCopy.confirmText(editor.value, "nicky"),
        )
        editor.dispatch(HandleAction.CancelClaim)
        assertNull(editor.value.confirm)
        assertEquals(emptyList<String>(), account.claims)
        editor.dispatch(HandleAction.Claim)
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(listOf("nicky"), account.claims)
        assertEquals("nicky", editor.value.current)
        assertEquals(listOf("nick#2341", "nick#0912"), editor.value.others)
        assertEquals(1, account.finished)
    }

    @Test fun typingTheCurrentHandleSaysItIsCurrent() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nickm")).apply {
                available = { listOf("${handleName(it)}#0412") }
            }
        val editor = open(account, HandleMode.CHANGE)
        editor.type("nick#2341")
        editor.dispatch(HandleAction.Check)
        // The server lists no handle that this inbox holds as free, so the editor does not ask it.
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals(HandleInputProblem.CURRENT, editor.value.problem)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertFalse(editor.value.canCheck)
        assertFalse(editor.value.canClaim)
        assertEquals(
            "This is your current handle.",
            HandleCopy.problem(HandleInputProblem.CURRENT, "nick#2341", editor.value.minNameLength),
        )
        editor.type("nick#2342")
        assertNull(editor.value.problem)
        assertTrue(editor.value.canCheck)
    }

    @Test fun typingAnOtherHandleOffersTheSwitchBack() {
        val account =
            FakeAccount(current = "nick#2341", others = listOf("nickm")).apply {
                available = { listOf("${handleName(it)}#0412") }
            }
        val editor = open(account, HandleMode.CHANGE)
        // Use offers only the inbox's own other handles.
        editor.dispatch(HandleAction.SwitchBack("nick#2341"))
        editor.dispatch(HandleAction.SwitchBack("someone"))
        assertNull(editor.value.confirm)
        // A typed other handle opens the same confirmation as Use. It is not offered as taken.
        editor.type("nickm")
        editor.dispatch(HandleAction.Check)
        assertEquals(emptyList<String>(), account.lookups)
        assertEquals("nickm", editor.value.confirm)
        assertEquals(HandleStatus.Editing, editor.value.status)
        assertEquals(emptyList<String>(), account.claims)
        editor.dispatch(HandleAction.ConfirmClaim)
        assertEquals(listOf("nickm"), account.claims)
        assertEquals("nickm", editor.value.current)
        assertEquals(listOf("nick#2341"), editor.value.others)
    }

    @Test fun skipLeavesOnboardingWithoutAClaim() {
        val account = FakeAccount()
        val editor = open(account)
        editor.type("nick")
        editor.dispatch(HandleAction.Skip)
        assertEquals(1, account.finished)
        assertEquals(emptyList<String>(), account.claims)
    }

    @Test fun anAccountWithAHandleSkipsOnboarding() {
        val account = FakeAccount(current = "nick")
        open(account)
        assertEquals(1, account.finished)
        val settings = FakeAccount(current = "nick")
        val editor = open(settings, HandleMode.CHANGE)
        assertEquals(0, settings.finished)
        assertEquals("nick", editor.value.current)
    }
}
