import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*

// Native storage validates the pair; options() lifts the values back to Kotlin.
internal suspend fun checkStoragePoolOptionsCrossTheNativeBoundary(backend: BackendOptions) {
    val defaultPool = StoragePoolOptions()
    check(defaultPool.min == null && defaultPool.max == null) { "default pool fields must stay absent" }
    val explicitPool = StoragePoolOptions(min = 2u, max = 10u)
    check(explicitPool.min == 2u && explicitPool.max == 10u) { "pool fields must stay distinct" }
    val partialPool = StoragePoolOptions(max = 7u)
    check(partialPool.min == null && partialPool.max == 7u) { "partial pool must keep the absent field" }
    val options =
        ClientOptions(
            backend = BackendSource.Options(backend),
            storage = StorageOptions(location = StorageLocation.InMemory),
            registration = RegistrationOptions(auto = false),
            deviceSync = false,
        )
    val pools =
        listOf(
            null,
            StoragePoolOptions(),
            StoragePoolOptions(min = 2u, max = 10u),
            StoragePoolOptions(max = 7u),
        )
    for (pool in pools) {
        val configured = options.copy(storage = options.storage.copy(pool = pool))
        val client = withTimeout(10_000) { SDKClient.create(generateLocalSigner(), configured) }
        try {
            check(client.options().storage.pool == pool) { "native storage changed optional pool values" }
        } finally {
            withContext(NonCancellable) { client.end() }
        }
    }
    val invalid = options.copy(storage = options.storage.copy(pool = StoragePoolOptions(min = 4u, max = 2u)))
    val failure =
        runCatching {
            withTimeout(10_000) {
                val unexpected = SDKClient.create(generateLocalSigner(), invalid)
                withContext(NonCancellable) { unexpected.end() }
            }
        }.exceptionOrNull()
    check(failure is XmtpException.InvalidInput) {
        "native storage accepted a pool minimum above its maximum: $failure"
    }
    check(failure.v1.category == ErrorCategory.INPUT && !failure.v1.retryable)
    println("Kotlin retained storage pool: default, partial, distinct fields and native range rejection passed")
}

// verifies: CONF-020, CONF-061, CONF-062
internal suspend fun checkConfigurationRefreshKeepsTheHeldSnapshot(backend: BackendOptions) {
    val baseline = fetchServerConfiguration(BackendSource.Options(backend))
    val occupied = baseline.applicationComponents.map { it.componentId }.toSet()
    val id = (0xC000..0xFFFF).map { it.toUShort() }.first { it !in occupied }
    val policy = MetadataPolicy.Base(MetadataBasePolicy.Allow)
    val fixture =
        ApplicationComponentDefinition(
            id,
            "held_configuration_probe",
            MetadataComponentType.String,
            ComponentPermissions(policy, policy, policy),
            inGroups = true,
            inDms = false,
        )
    val options =
        ClientOptions(
            backend = BackendSource.Options(backend),
            storage = StorageOptions(location = StorageLocation.InMemory),
            deviceSync = false,
        )
    sdkConformanceUseApplicationComponents(listOf(fixture))
    val client =
        try {
            withTimeout(10_000) { SDKClient.create(generateLocalSigner(), options) }
        } finally {
            sdkConformanceUseApplicationComponents(null)
        }
    try {
        val held = client.serverConfiguration()
        check(held.applicationComponents == listOf(fixture)) { "the distinct held catalogue was not installed" }
        val refreshed = withTimeout(10_000) { client.refreshServerConfiguration() }
        check(refreshed.applicationComponents == baseline.applicationComponents) {
            "refresh did not return the fetched catalogue"
        }
        check(refreshed.applicationComponents != held.applicationComponents)
        check(client.serverConfiguration() == held) { "refresh replaced the running client's held snapshot" }
    } finally {
        withContext(NonCancellable) { client.end() }
    }
    println("Kotlin retained configuration refresh returns new fields and keeps the held snapshot")
}
