import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicInteger

// Retains BackendAuthTest's per-call values and callback failure boundary.
internal suspend fun checkCredentialCallbacksStayDistinct() {
    val firstCalls = AtomicInteger()
    val secondCalls = AtomicInteger()

    fun source(
        prefix: String,
        calls: AtomicInteger,
    ) = SDKForeign.credentials(
        object : CredentialSource {
            override suspend fun credential(): Credential {
                val number = calls.incrementAndGet()
                return Credential(null, "Bearer $prefix-$number", 9_007_199_254_740_993L + number)
            }
        },
    )
    val first = source("first", firstCalls)
    val second = source("second", secondCalls)
    val a = withTimeout(5_000) { first.credential() }
    val b = withTimeout(5_000) { first.credential() }
    check(a.name == null && a.value == "Bearer first-1" && a.expiresAtSeconds == 9_007_199_254_740_994L)
    check(b.name == null && b.value == "Bearer first-2" && b.expiresAtSeconds == 9_007_199_254_740_995L)
    check(firstCalls.get() == 2 && secondCalls.get() == 0) { "foreign credentials cached or shared a callback" }
    check(withTimeout(5_000) { second.credential() }.value == "Bearer second-1")
    check(firstCalls.get() == 2 && secondCalls.get() == 1)

    val secret = "private-credential-callback-detail"
    val failing =
        SDKForeign.credentials(
            object : CredentialSource {
                override suspend fun credential(): Credential = throw LinkageError(secret)
            },
        )
    val failure = runCatching { withTimeout(5_000) { failing.credential() } }.exceptionOrNull()
    check(failure is CredentialException.Failed && failure.cause == null)
    check(!failure.toString().contains(secret)) { "credential failure exposed the callback detail" }
    val cancellation = CancellationException("caller cancelled credentials")
    val cancelling =
        SDKForeign.credentials(
            object : CredentialSource {
                override suspend fun credential(): Credential = throw cancellation
            },
        )
    check(runCatching { cancelling.credential() }.exceptionOrNull() === cancellation)
    println("Kotlin retained credentials: distinct callbacks, exact wide expiry, failure and cancellation passed")
}

// Retains the old Credential display guarantee without changing its value.
internal fun checkCredentialDisplayRedactsToken() {
    val secret = "private-credential-display-token"
    val credential = Credential(null, "Bearer $secret", 123L)
    check(credential.value == "Bearer $secret")
    check(!credential.toString().contains(secret)) { "Credential display exposed its token" }
    println("Kotlin retained credential display redacts its token")
}

// verifies: CONF-062, AUTH-020
internal suspend fun checkConfigurationDiscoveryDoesNotCallCredentials(backend: BackendOptions) {
    val calls = AtomicInteger()
    val source =
        object : CredentialSource {
            override suspend fun credential(): Credential {
                calls.incrementAndGet()
                throw LinkageError("configuration discovery called credentials")
            }
        }
    val configured = backend.copy(credentials = source, credential = null)
    val fetched = withTimeout(10_000) { fetchServerConfiguration(BackendSource.Options(configured)) }
    check(fetched.identifier.isNotEmpty())
    check(calls.get() == 0) { "configuration discovery invoked a credential callback" }
    println("Kotlin retained configuration discovery uses no credential callback")
}
