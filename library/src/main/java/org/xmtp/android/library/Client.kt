package org.xmtp.android.library

import android.content.Context
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.InboxState
import org.xmtp.android.library.libxmtp.SignatureRequest
import org.xmtp.android.library.messages.rawData
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.XmtpApiClient
import uniffi.xmtpv3.connectToBackend
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForAddress
import uniffi.xmtpv3.getVersionInfo
import java.io.File

typealias PreEventCallback = suspend () -> Unit

data class ClientOptions(
    val api: Api = Api(),
    val preAuthenticateToInboxCallback: PreEventCallback? = null,
    val appContext: Context,
    val dbEncryptionKey: ByteArray,
    val historySyncUrl: String? = when (api.env) {
        XMTPEnvironment.PRODUCTION -> "https://message-history.production.ephemera.network/"
        XMTPEnvironment.LOCAL -> "http://0.0.0.0:5558"
        else -> "https://message-history.dev.ephemera.network/"
    },
    val dbDirectory: String? = null,
) {
    data class Api(
        val env: XMTPEnvironment = XMTPEnvironment.DEV,
        val isSecure: Boolean = true,
        val appVersion: String? = null,
    )
}

class Client(
    address: String,
    libXMTPClient: FfiXmtpClient,
    val dbPath: String,
    val installationId: String,
    val inboxId: String,
    val environment: XMTPEnvironment,
) {
    val address: String = address.lowercase()
    val preferences: PrivatePreferences =
        PrivatePreferences(client = this, ffiClient = libXMTPClient)
    val conversations: Conversations = Conversations(
        client = this,
        ffiConversations = libXMTPClient.conversations(),
        ffiClient = libXMTPClient
    )
    val libXMTPVersion: String = getVersionInfo()
    private val ffiClient: FfiXmtpClient = libXMTPClient

    companion object {
        private const val TAG = "Client"

        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }

        private val apiClientCache = mutableMapOf<String, XmtpApiClient>()
        private val cacheLock = Mutex()

        suspend fun connectToApiBackend(api: ClientOptions.Api): XmtpApiClient {
            val cacheKey = api.env.getUrl()
            return cacheLock.withLock {
                apiClientCache.getOrPut(cacheKey) {
                    connectToBackend(api.env.getUrl(), api.isSecure)
                }
            }
        }

        suspend fun getOrCreateInboxId(
            api: ClientOptions.Api,
            address: String,
        ): String {
            var inboxId = getInboxIdForAddress(
                api = connectToApiBackend(api),
                accountAddress = address.lowercase()
            )
            if (inboxId.isNullOrBlank()) {
                inboxId = generateInboxId(address.lowercase(), 0.toULong())
            }
            return inboxId
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }

        private suspend fun <T> withFfiClient(
            api: ClientOptions.Api,
            useClient: suspend (ffiClient: FfiXmtpClient) -> T,
        ): T {
            val accountAddress = "0x0000000000000000000000000000000000000000"
            val inboxId = getOrCreateInboxId(api, accountAddress)

            val ffiClient = createClient(
                api = connectToApiBackend(api),
                db = null,
                encryptionKey = null,
                accountAddress = accountAddress.lowercase(),
                inboxId = inboxId,
                nonce = 0.toULong(),
                legacySignedPrivateKeyProto = null,
                historySyncUrl = null
            )

            return useClient(ffiClient)
        }

        suspend fun inboxStatesForInboxIds(
            inboxIds: List<String>,
            api: ClientOptions.Api,
        ): List<InboxState> {
            return withFfiClient(api) { ffiClient ->
                ffiClient.addressesFromInboxId(true, inboxIds).map { InboxState(it) }
            }
        }

        suspend fun canMessage(
            accountAddresses: List<String>,
            api: ClientOptions.Api,
        ): Map<String, Boolean> {
            return withFfiClient(api) { ffiClient ->
                ffiClient.canMessage(accountAddresses)
            }
        }

        private suspend fun initializeV3Client(
            address: String,
            clientOptions: ClientOptions,
            signingKey: SigningKey? = null,
            inboxId: String? = null,
        ): Client {
            val accountAddress = address.lowercase()
            val recoveredInboxId =
                inboxId ?: getOrCreateInboxId(clientOptions.api, accountAddress)

            val (ffiClient, dbPath) = createFfiClient(
                accountAddress,
                recoveredInboxId,
                clientOptions,
                clientOptions.appContext,
            )
            clientOptions.preAuthenticateToInboxCallback?.let {
                runBlocking {
                    it.invoke()
                }
            }
            ffiClient.signatureRequest()?.let { signatureRequest ->
                signingKey?.let { handleSignature(SignatureRequest(signatureRequest), it) }
                    ?: throw XMTPException("No signer passed but signer was required.")
                ffiClient.registerIdentity(signatureRequest)
            }

            return Client(
                accountAddress,
                ffiClient,
                dbPath,
                ffiClient.installationId().toHex(),
                ffiClient.inboxId(),
                clientOptions.api.env,
            )
        }

        // Function to create a client with a signing key
        suspend fun create(
            account: SigningKey,
            options: ClientOptions,
        ): Client {
            return try {
                initializeV3Client(account.address, options, account)
            } catch (e: Exception) {
                throw XMTPException("Error creating V3 client: ${e.message}", e)
            }
        }

        // Function to build a client from a address
        suspend fun build(
            address: String,
            options: ClientOptions,
            inboxId: String? = null,
        ): Client {
            return try {
                initializeV3Client(address, options, inboxId = inboxId)
            } catch (e: Exception) {
                throw XMTPException("Error creating V3 client: ${e.message}", e)
            }
        }

        private suspend fun createFfiClient(
            accountAddress: String,
            inboxId: String,
            options: ClientOptions,
            appContext: Context,
        ): Pair<FfiXmtpClient, String> {
            val alias = "xmtp-${options.api.env}-$inboxId"

            val mlsDbDirectory = options.dbDirectory
            val directoryFile = if (mlsDbDirectory != null) {
                File(mlsDbDirectory)
            } else {
                File(appContext.filesDir.absolutePath, "xmtp_db")
            }
            directoryFile.mkdir()
            val dbPath = directoryFile.absolutePath + "/$alias.db3"

            val ffiClient = createClient(
                api = connectToApiBackend(options.api),
                db = dbPath,
                encryptionKey = options.dbEncryptionKey,
                accountAddress = accountAddress.lowercase(),
                inboxId = inboxId,
                nonce = 0.toULong(),
                legacySignedPrivateKeyProto = null,
                historySyncUrl = options.historySyncUrl
            )

            return Pair(ffiClient, dbPath)
        }

        private suspend fun handleSignature(
            signatureRequest: SignatureRequest,
            signingKey: SigningKey,
        ) {
            if (signingKey.type == WalletType.SCW) {
                val chainId = signingKey.chainId
                    ?: throw XMTPException("ChainId is required for smart contract wallets")
                signatureRequest.addScwSignature(
                    signingKey.signSCW(signatureRequest.signatureText()),
                    signingKey.address.lowercase(),
                    chainId.toULong(),
                    signingKey.blockNumber?.toULong()
                )
            } else {
                signingKey.sign(signatureRequest.signatureText())?.let {
                    signatureRequest.addEcdsaSignature(it.rawData)
                }
            }
        }

        @DelicateApi("This function is delicate and should be used with caution. Creating an FfiClient without signing or registering will create a broken experience use `create()` instead")
        suspend fun ffiCreateClient(address: String, clientOptions: ClientOptions): Client {
            val accountAddress = address.lowercase()
            val recoveredInboxId = getOrCreateInboxId(clientOptions.api, accountAddress)

            val (ffiClient, dbPath) = createFfiClient(
                accountAddress,
                recoveredInboxId,
                clientOptions,
                clientOptions.appContext,
            )
            return Client(
                accountAddress,
                ffiClient,
                dbPath,
                ffiClient.installationId().toHex(),
                ffiClient.inboxId(),
                clientOptions.api.env,
            )
        }
    }

    suspend fun revokeInstallations(signingKey: SigningKey, installationIds: List<String>) {
        val ids = installationIds.map { it.hexToByteArray() }
        val signatureRequest = ffiRevokeInstallations(ids)
        handleSignature(signatureRequest, signingKey)
        ffiApplySignatureRequest(signatureRequest)
    }

    suspend fun revokeAllOtherInstallations(signingKey: SigningKey) {
        val signatureRequest = ffiRevokeAllOtherInstallations()
        handleSignature(signatureRequest, signingKey)
        ffiApplySignatureRequest(signatureRequest)
    }

    @DelicateApi("This function is delicate and should be used with caution. Adding a wallet already associated with an inboxId will cause the wallet to lose access to that inbox. See: inboxIdFromAddress(address)")
    suspend fun addAccount(newAccount: SigningKey, allowReassignInboxId: Boolean = false) {
        val inboxId: String? =
            if (!allowReassignInboxId) inboxIdFromAddress(newAccount.address) else null

        if (allowReassignInboxId || inboxId.isNullOrBlank()) {
            val signatureRequest = ffiAddWallet(newAccount.address.lowercase())
            handleSignature(signatureRequest, newAccount)
            ffiApplySignatureRequest(signatureRequest)
        } else {
            throw XMTPException("This wallet is already associated with inbox $inboxId")
        }
    }

    suspend fun removeAccount(recoverAccount: SigningKey, addressToRemove: String) {
        val signatureRequest = ffiRevokeWallet(addressToRemove.lowercase())
        handleSignature(signatureRequest, recoverAccount)
        ffiApplySignatureRequest(signatureRequest)
    }

    fun signWithInstallationKey(message: String): ByteArray {
        return ffiClient.signWithInstallationKey(message)
    }

    fun verifySignature(message: String, signature: ByteArray): Boolean {
        return try {
            ffiClient.verifySignedWithInstallationKey(message, signature)
            true
        } catch (e: Exception) {
            false
        }
    }

    fun verifySignatureWithInstallationId(
        message: String,
        signature: ByteArray,
        installationId: String,
    ): Boolean {
        return try {
            ffiClient.verifySignedWithPublicKey(message, signature, installationId.hexToByteArray())
            true
        } catch (e: Exception) {
            false
        }
    }

    suspend fun canMessage(addresses: List<String>): Map<String, Boolean> {
        return ffiClient.canMessage(addresses)
    }

    suspend fun inboxIdFromAddress(address: String): String? {
        return ffiClient.findInboxId(address.lowercase())
    }

    fun deleteLocalDatabase() {
        dropLocalDatabaseConnection()
        File(dbPath).delete()
    }

    @DelicateApi("This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()")
    fun dropLocalDatabaseConnection() {
        ffiClient.releaseDbConnection()
    }

    suspend fun reconnectLocalDatabase() {
        ffiClient.dbReconnect()
    }

    suspend fun inboxStatesForInboxIds(
        refreshFromNetwork: Boolean,
        inboxIds: List<String>,
    ): List<InboxState> {
        return ffiClient.addressesFromInboxId(refreshFromNetwork, inboxIds).map { InboxState(it) }
    }

    suspend fun inboxState(refreshFromNetwork: Boolean): InboxState {
        return InboxState(ffiClient.inboxState(refreshFromNetwork))
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `addAccount(), removeAccount(), or revoke()` instead")
    suspend fun ffiApplySignatureRequest(signatureRequest: SignatureRequest) {
        ffiClient.applySignatureRequest(signatureRequest.ffiSignatureRequest)
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeInstallations()` instead")
    suspend fun ffiRevokeInstallations(ids: List<ByteArray>): SignatureRequest {
        return SignatureRequest(ffiClient.revokeInstallations(ids))
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeAllOtherInstallations()` instead")
    suspend fun ffiRevokeAllOtherInstallations(): SignatureRequest {
        return SignatureRequest(
            ffiClient.revokeAllOtherInstallations()
        )
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `removeWallet()` instead")
    suspend fun ffiRevokeWallet(addressToRemove: String): SignatureRequest {
        return SignatureRequest(ffiClient.revokeWallet(addressToRemove.lowercase()))
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the create and register flow independently otherwise use `addWallet()` instead")
    suspend fun ffiAddWallet(addressToAdd: String): SignatureRequest {
        return SignatureRequest(ffiClient.addWallet(addressToAdd.lowercase()))
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `create()` instead")
    fun ffiSignatureRequest(): SignatureRequest? {
        return ffiClient.signatureRequest()?.let { SignatureRequest(it) }
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the create and register flow independently otherwise use `create()` instead")
    suspend fun ffiRegisterIdentity(signatureRequest: SignatureRequest) {
        ffiClient.registerIdentity(signatureRequest.ffiSignatureRequest)
    }
}
