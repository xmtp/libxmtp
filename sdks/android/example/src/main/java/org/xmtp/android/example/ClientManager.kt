package org.xmtp.android.example

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.utils.KeyUtil
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

object ClientManager {
    fun clientOptions(
        context: Context,
        address: String,
    ): ClientOptions {
        val keys = KeyUtil(context)
        val encryptionKey =
            keys.retrieveKey(address)?.takeUnless { it.isEmpty() }
                ?: SecureRandom().generateSeed(32).also { keys.storeKey(address, it) }
        return ClientOptions(
            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
            storage = StorageOptions(location = StorageLocation.Default, encryptionKey = encryptionKey),
        )
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lock = Mutex()
    private val state = MutableStateFlow<ClientState>(ClientState.Unknown)
    val clientState: StateFlow<ClientState> = state
    private var current: SDKClient? = null
    val client: SDKClient get() = checkNotNull(current) { "Client is not ready" }

    fun createClient(
        address: String,
        context: Context,
    ) {
        scope.launch {
            lock.withLock {
                if (current != null) return@withLock
                try {
                    current =
                        SDKClient.build(
                            context.applicationContext,
                            PublicIdentity(address, PublicIdentityKind.ETHEREUM),
                            clientOptions(context, address),
                        )
                    state.value = ClientState.Ready
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    state.value = ClientState.Error(error.message.orEmpty())
                }
            }
        }
    }

    fun clearClient() {
        scope.launch {
            lock.withLock {
                try {
                    withContext(NonCancellable) { current?.end() }
                } finally {
                    current = null
                    state.value = ClientState.Unknown
                }
            }
        }
    }

    sealed class ClientState {
        object Unknown : ClientState()

        object Ready : ClientState()

        data class Error(
            val message: String,
        ) : ClientState()
    }
}
