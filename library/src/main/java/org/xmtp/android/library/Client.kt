package org.xmtp.android.library

import android.content.Context
import android.util.Log
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.InboxState
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.libxmtp.SignatureRequest
import uniffi.xmtpv3.FfiLogLevel
import uniffi.xmtpv3.FfiLogRotation
import uniffi.xmtpv3.FfiKeyPackageStatus
import uniffi.xmtpv3.FfiSyncWorkerMode
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.XmtpApiClient
import uniffi.xmtpv3.connectToBackend
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.enterDebugWriter
import uniffi.xmtpv3.exitDebugWriter
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForIdentifier
import uniffi.xmtpv3.getVersionInfo
import java.io.File

typealias PreEventCallback = suspend () -> Unit

data class ClientOptions(
    val api: Api = Api(),
    val preAuthenticateToInboxCallback: PreEventCallback? = null,
    val appContext: Context,
    val dbEncryptionKey: ByteArray,
    val historySyncUrl: String? = api.env.getHistorySyncUrl(),
    val dbDirectory: String? = null,
) {
    data class Api(
        val env: XMTPEnvironment = XMTPEnvironment.DEV,
        val isSecure: Boolean = true,
    )
}

typealias InboxId = String

class Client(
    libXMTPClient: FfiXmtpClient,
    val dbPath: String,
    val installationId: String,
    val inboxId: InboxId,
    val environment: XMTPEnvironment,
    val publicIdentity: PublicIdentity,
) {
    val preferences: PrivatePreferences =
        PrivatePreferences(client = this, ffiClient = libXMTPClient)
    val conversations: Conversations = Conversations(
        client = this,
        ffiConversations = libXMTPClient.conversations(),
        ffiClient = libXMTPClient
    )
    val debugInformation: XMTPDebugInformation =
        XMTPDebugInformation(ffiClient = libXMTPClient, this)
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

        fun activatePersistentLibXMTPLogWriter(
            appContext: Context,
            logLevel: FfiLogLevel,
            rotationSchedule: FfiLogRotation,
            maxFiles: Int,
        ) {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                logDirectory.mkdirs()
            }
            enterDebugWriter(
                logDirectory.toString(), logLevel, rotationSchedule,
                maxFiles.toUInt()
            )
        }

        fun deactivatePersistentLibXMTPLogWriter() {
            exitDebugWriter()
        }

        fun getXMTPLogFilePaths(appContext: Context): List<String> {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                return emptyList()
            }

            return logDirectory.listFiles()
                ?.filter { it.isFile }
                ?.map { it.absolutePath }
                ?: emptyList()
        }

        fun clearXMTPLogs(appContext: Context): Int {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                return 0
            }

            try {
                deactivatePersistentLibXMTPLogWriter()
            } catch (e: Exception) {
                // Log writer might not be active, continue with deletion
            }

            var deletedCount = 0
            logDirectory.listFiles()?.forEach { file ->
                if (file.isFile && file.delete()) {
                    deletedCount++
                }
            }

            return deletedCount
        }

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
            publicIdentity: PublicIdentity,
        ): InboxId {
            val rootIdentity = publicIdentity.ffiPrivate
            var inboxId = getInboxIdForIdentifier(
                api = connectToApiBackend(api),
                accountIdentifier = rootIdentity
            )
            if (inboxId.isNullOrBlank()) {
                inboxId = generateInboxId(rootIdentity, 0.toULong())
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
            val publicIdentity = PublicIdentity(
                IdentityKind.ETHEREUM,
                "0x0000000000000000000000000000000000000000"
            )
            val inboxId = getOrCreateInboxId(api, publicIdentity)

            val ffiClient = createClient(
                api = connectToApiBackend(api),
                db = null,
                encryptionKey = null,
                accountIdentifier = publicIdentity.ffiPrivate,
                inboxId = inboxId,
                nonce = 0.toULong(),
                legacySignedPrivateKeyProto = null,
                deviceSyncServerUrl = null,
                deviceSyncMode = null,
                allowOffline = false,
            )

            return useClient(ffiClient)
        }

        suspend fun inboxStatesForInboxIds(
            inboxIds: List<InboxId>,
            api: ClientOptions.Api,
        ): List<InboxState> {
            return withFfiClient(api) { ffiClient ->
                ffiClient.addressesFromInboxId(true, inboxIds)
                    .map { InboxState(it) }
            }
        }

        suspend fun keyPackageStatusesForInstallationIds(
            installationIds: List<String>,
            api: ClientOptions.Api,
        ): Map<String, FfiKeyPackageStatus> {
            return withFfiClient(api) { ffiClient ->
                val byteArrays = installationIds.map { it.hexToByteArray() }
                val result = ffiClient.getKeyPackageStatusesForInstallationIds(byteArrays)
                result.entries.associate { (byteArrayKey, status) ->
                    byteArrayKey.toHex() to status
                }
            }
        }

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            api: ClientOptions.Api,
        ): Map<String, Boolean> {
            return withFfiClient(api) { ffiClient ->
                val ffiIdentifiers = identities.map { it.ffiPrivate }
                val result = ffiClient.canMessage(ffiIdentifiers)

                result.mapKeys { (ffiIdentifier, _) ->
                    ffiIdentifier.identifier
                }
            }
        }

        private suspend fun initializeV3Client(
            publicIdentity: PublicIdentity,
            clientOptions: ClientOptions,
            signingKey: SigningKey? = null,
            inboxId: InboxId? = null,
            buildOffline: Boolean = false,
        ): Client {
            val recoveredInboxId =
                inboxId ?: getOrCreateInboxId(clientOptions.api, publicIdentity)

            val (ffiClient, dbPath) = createFfiClient(
                publicIdentity,
                recoveredInboxId,
                clientOptions,
                clientOptions.appContext,
                buildOffline
            )
            clientOptions.preAuthenticateToInboxCallback?.let {
                runBlocking {
                    it.invoke()
                }
            }
            ffiClient.signatureRequest()?.let { signatureRequest ->
                signingKey?.let {
                    handleSignature(SignatureRequest(signatureRequest), it)
                } ?: run {
                    Log.d("XMTP", "No signer provided. Logging DB context...")
                    Log.d("XMTP", "dbPath: $dbPath")

                    if (clientOptions.dbDirectory != null) {
                        Log.d("XMTP", "dbDirectory: ${clientOptions.dbDirectory}")

                        val dbDirFile = File(clientOptions.dbDirectory)
                        val fileCount = dbDirFile.listFiles()?.size ?: 0

                        Log.d("XMTP", "Files in dbDirectory: $fileCount")
                    }
                    throw XMTPException("No signer passed but signer was required.")
                }

                ffiClient.registerIdentity(signatureRequest)
            }

            return Client(
                ffiClient,
                dbPath,
                ffiClient.installationId().toHex(),
                ffiClient.inboxId(),
                clientOptions.api.env,
                publicIdentity
            )
        }

        // Function to create a client with a signing key
        suspend fun create(
            account: SigningKey,
            options: ClientOptions,
        ): Client {
            return try {
                initializeV3Client(account.publicIdentity, options, account)
            } catch (e: Exception) {
                throw XMTPException("Error creating V3 client: ${e.message}", e)
            }
        }

        // Function to build a client from a address
        suspend fun build(
            publicIdentity: PublicIdentity,
            options: ClientOptions,
            inboxId: InboxId? = null,
        ): Client {
            return try {
                initializeV3Client(
                    publicIdentity,
                    options,
                    inboxId = inboxId,
                    buildOffline = inboxId != null
                )
            } catch (e: Exception) {
                throw XMTPException("Error creating V3 client: ${e.message}", e)
            }
        }

        private suspend fun createFfiClient(
            publicIdentity: PublicIdentity,
            inboxId: InboxId,
            options: ClientOptions,
            appContext: Context,
            buildOffline: Boolean = false,
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
                accountIdentifier = publicIdentity.ffiPrivate,
                inboxId = inboxId,
                nonce = 0.toULong(),
                legacySignedPrivateKeyProto = null,
                deviceSyncServerUrl = options.historySyncUrl,
                deviceSyncMode = FfiSyncWorkerMode.ENABLED,
                allowOffline = buildOffline
            )
            return Pair(ffiClient, dbPath)
        }

        private suspend fun handleSignature(
            signatureRequest: SignatureRequest,
            signingKey: SigningKey,
        ) {
            val signedData = signingKey.sign(signatureRequest.signatureText())

            when (signingKey.type) {
                SignerType.SCW -> {
                    val chainId =
                        signingKey.chainId ?: throw XMTPException("ChainId is required for SCW")
                    signatureRequest.addScwSignature(
                        signedData.rawData,
                        signingKey.publicIdentity.identifier,
                        chainId.toULong(),
                        signingKey.blockNumber?.toULong()
                    )
                }

                else -> {
                    signatureRequest.addEcdsaSignature(signedData.rawData)
                }
            }
        }

        @DelicateApi("This function is delicate and should be used with caution. Creating an FfiClient without signing or registering will create a broken experience use `create()` instead")
        suspend fun ffiCreateClient(
            publicIdentity: PublicIdentity,
            clientOptions: ClientOptions,
        ): Client {
            val recoveredInboxId = getOrCreateInboxId(clientOptions.api, publicIdentity)

            val (ffiClient, dbPath) = createFfiClient(
                publicIdentity,
                recoveredInboxId,
                clientOptions,
                clientOptions.appContext,
            )
            return Client(
                ffiClient,
                dbPath,
                ffiClient.installationId().toHex(),
                ffiClient.inboxId(),
                clientOptions.api.env,
                publicIdentity
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

    @DelicateApi("This function is delicate and should be used with caution. Adding a identity already associated with an inboxId will cause the identity to lose access to that inbox. See: inboxIdFromIdentity(publicIdentity)")
    suspend fun addAccount(newAccount: SigningKey, allowReassignInboxId: Boolean = false) {
        val signatureRequest = ffiAddIdentity(newAccount.publicIdentity, allowReassignInboxId)
        handleSignature(signatureRequest, newAccount)
        ffiApplySignatureRequest(signatureRequest)
    }

    suspend fun removeAccount(recoverAccount: SigningKey, publicIdentityToRemove: PublicIdentity) {
        val signatureRequest = ffiRevokeIdentity(publicIdentityToRemove)
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

    suspend fun canMessage(identities: List<PublicIdentity>): Map<String, Boolean> {
        val ffiIdentifiers = identities.map { it.ffiPrivate }
        val result = ffiClient.canMessage(ffiIdentifiers)

        return result.mapKeys { (ffiIdentifier, _) ->
            ffiIdentifier.identifier
        }
    }

    suspend fun inboxIdFromIdentity(publicIdentity: PublicIdentity): InboxId? {
        return ffiClient.findInboxId(publicIdentity.ffiPrivate)
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
        inboxIds: List<InboxId>,
    ): List<InboxState> {
        return ffiClient.addressesFromInboxId(refreshFromNetwork, inboxIds)
            .map { InboxState(it) }
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

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `removeAccount()` instead")
    suspend fun ffiRevokeIdentity(publicIdentityToRemove: PublicIdentity): SignatureRequest {
        return SignatureRequest(ffiClient.revokeIdentity(publicIdentityToRemove.ffiPrivate))
    }

    @DelicateApi("This function is delicate and should be used with caution. Should only be used if trying to manage the create and register flow independently otherwise use `addAccount()` instead")
    suspend fun ffiAddIdentity(
        publicIdentityToAdd: PublicIdentity,
        allowReassignInboxId: Boolean = false,
    ): SignatureRequest {
        val inboxId: InboxId? =
            if (!allowReassignInboxId) inboxIdFromIdentity(
                PublicIdentity(
                    publicIdentityToAdd.kind,
                    publicIdentityToAdd.identifier
                )
            ) else null

        if (allowReassignInboxId || inboxId.isNullOrBlank()) {
            return SignatureRequest(ffiClient.addIdentity(publicIdentityToAdd.ffiPrivate))
        } else {
            throw XMTPException("This identity is already associated with inbox $inboxId")
        }
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
