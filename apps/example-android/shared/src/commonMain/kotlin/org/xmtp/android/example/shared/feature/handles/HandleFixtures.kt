package org.xmtp.android.example.shared.feature.handles

/** One fixed state for each approved design board. */
object HandleFixtures {
    private val codes = listOf("nick#2341", "nick#0912", "nick#5530", "nick#6418", "nick#7702", "nick#1180")

    val empty = HandleState()
    val checking = HandleState(input = "nick", status = HandleStatus.Checking("nick"))
    val available = HandleState(input = "nick", status = HandleStatus.Available("nick"))
    val suggestions =
        HandleState(
            input = "nick",
            status = HandleStatus.Suggestions("nick", codes, HANDLE_SUGGESTION_PAGE, codes.first()),
        )
    val claiming = suggestions.copy(claiming = codes.first())
    val takenWhileClaiming =
        HandleState(
            input = "nick",
            status = HandleStatus.Suggestions("nick", codes.drop(1), HANDLE_SUGGESTION_PAGE, codes[1]),
            notice = HandleNotice.Taken(codes.first()),
        )
    val invalidCharacters = input("nick.eth")
    val startsWithDigit = input("1nick")
    val tooShort = input("jo")
    val badCode = input("nick#12")
    val tooLong = input("nicholas_the_magnificent_one")
    val notAllowed = HandleState(input = "admin", notice = HandleNotice.NotAllowed("admin"))
    val noneFree = HandleState(input = "nick", status = HandleStatus.NoneFree("nick"))
    val checkFailed = HandleState(input = "nick", notice = HandleNotice.CheckFailed)
    val changeLoading = HandleState(mode = HandleMode.CHANGE, loading = true)

    private val change =
        HandleState(
            mode = HandleMode.CHANGE,
            current = "nick#2341",
            currentSince = "Oct 3",
            others = listOf("nick#0912", "nickm"),
        )
    val changeAvailable = change.copy(input = "nicky", status = HandleStatus.Available("nicky"))
    val confirmChange = changeAvailable.copy(confirm = "nicky")
    val tooSoon =
        change.copy(
            currentSince = "Oct 10",
            notice = HandleNotice.TooSoon("Oct 10", "Oct 17"),
        )
    val switchBackBlocked =
        change.copy(
            currentSince = "Sep 2",
            minNameLength = 5,
            others = listOf("nick#0912", "nk"),
            notice = HandleNotice.SwitchBackBlocked("nk", shorterThanMinimum = true),
        )
    val limitReached =
        change.copy(
            currentSince = "Mar 14",
            others = listOf("nick#0912", "nickm", "nick_m") + (1..60).map { "nick_$it" },
            notice = HandleNotice.LimitReached,
        )
    val firstFromSettings = HandleState(mode = HandleMode.CHANGE)

    private fun input(value: String) =
        HandleState(input = value, problem = handleInputProblem(value, DEFAULT_HANDLE_MIN_NAME_LENGTH))
}
