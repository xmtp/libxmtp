package org.xmtp.android.library

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.UUID

fun localApi(appVersion: String? = null): BackendOptions =
    BackendOptions(url = BuildConfig.XMTP_BACKEND_URL, appVersion = appVersion)

const val ANVIL_TEST_PRIVATE_KEY_1 =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
const val ANVIL_TEST_PRIVATE_KEY_2 =
    "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
const val ANVIL_TEST_PRIVATE_KEY_3 =
    "5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a"

typealias FakeSCWWallet = uniffi.xmtp_sdk.FakeSCWWallet

val PublicIdentity.walletAddress: String get() = identifier

class Fixtures(
    api: BackendOptions = localApi(),
) {
    val key = SecureRandom().generateSeed(32)
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val clientOptions =
        ClientOptions(
            backend = BackendSource.Options(api),
            storage = StorageOptions(StorageLocation.Default, encryptionKey = key),
        )
    val alixAccount = runBlocking { generateLocalSigner() }
    val boAccount = runBlocking { generateLocalSigner() }
    val caroAccount = runBlocking { generateLocalSigner() }
    val davonAccount = runBlocking { generateLocalSigner() }
    val eriAccount = runBlocking { generateLocalSigner() }
    val alix = runBlocking { alixAccount.identity() }
    val bo = runBlocking { boAccount.identity() }
    val caro = runBlocking { caroAccount.identity() }
    val davon = runBlocking { davonAccount.identity() }
    val eri = runBlocking { eriAccount.identity() }
    val alixClient = create(alixAccount)
    val boClient = create(boAccount)
    val caroClient = create(caroAccount)
    val davonClient = create(davonAccount)
    val eriClient = create(eriAccount)

    private fun create(signer: Signer): SDKClient =
        runBlocking {
            SDKClient.create(
                context,
                signer,
                clientOptions.copy(
                    storage = clientOptions.storage.copy(label = UUID.randomUUID().toString()),
                ),
            )
        }
}

fun fixtures(api: BackendOptions = localApi()): Fixtures = Fixtures(api)
