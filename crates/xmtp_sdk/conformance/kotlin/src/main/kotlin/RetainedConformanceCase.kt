import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*

// Root uses this selector for bounded failure controls on one retained case.
internal suspend fun runRetainedConformanceCase(
    name: String,
    backend: BackendOptions,
) {
    when (name) {
        "credentials" -> {
            checkCredentialCallbacksStayDistinct()
        }

        "credential-display" -> {
            checkCredentialDisplayRedactsToken()
        }

        "configuration-discovery" -> {
            checkConfigurationDiscoveryDoesNotCallCredentials(backend)
        }

        "configuration-fields" -> {
            checkNativeConfigurationRecordProjection()
        }

        "pure-inbox-id" -> {
            checkPureInboxIdCalculation()
        }

        "pool" -> {
            checkStoragePoolOptionsCrossTheNativeBoundary(backend)
        }

        "storage-reconnect" -> {
            checkStorageReconnectAndRebuildKeepHistory(backend)
        }

        "configuration-refresh" -> {
            checkConfigurationRefreshKeepsTheHeldSnapshot(backend)
        }

        "reader-boundary" -> {
            checkReaderCollectorBoundarySurvivesDatabaseReopen(backend)
        }

        "reader-ended-owner" -> {
            checkEndedClientCannotHandOffReaderValues(backend)
        }

        "reader-read-failure", "reader-collector-close" -> {
            val owner =
                SDKClient.create(
                    generateLocalSigner(),
                    ClientOptions(
                        backend = BackendSource.Options(backend),
                        storage = StorageOptions(location = StorageLocation.InMemory),
                        registration = RegistrationOptions(auto = false),
                        deviceSync = false,
                    ),
                )
            try {
                if (name == "reader-collector-close") {
                    checkReaderCollectorCloseReasons(owner)
                } else {
                    checkReaderReadFailuresEndExactlyOnce(owner)
                }
            } finally {
                withContext(NonCancellable) { owner.end() }
            }
        }

        else -> {
            error("unknown Kotlin retained conformance case: $name")
        }
    }
}
