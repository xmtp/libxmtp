package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Uint
import org.web3j.crypto.Credentials
import org.web3j.crypto.Sign
import org.web3j.protocol.Web3j
import org.web3j.protocol.http.HttpService
import org.web3j.tx.gas.DefaultGasProvider
import org.web3j.utils.Numeric
import org.xmtp.android.library.artifact.CoinbaseSmartWallet
import org.xmtp.android.library.artifact.CoinbaseSmartWalletFactory
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.ethHash
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import java.math.BigInteger
import java.util.Date

class FakeWallet : SigningKey {
    private var privateKey: PrivateKey
    private var privateKeyBuilder: PrivateKeyBuilder

    constructor(key: PrivateKey, builder: PrivateKeyBuilder) {
        privateKey = key
        privateKeyBuilder = builder
    }

    companion object {
        fun generate(): FakeWallet {
            val key = PrivateKeyBuilder()
            return FakeWallet(key.getPrivateKey(), key)
        }
    }

    override suspend fun sign(data: ByteArray): Signature {
        val signature = privateKeyBuilder.sign(data)
        return signature
    }

    override suspend fun sign(message: String): Signature {
        val signature = privateKeyBuilder.sign(message)
        return signature
    }

    override val address: String
        get() = privateKey.walletAddress
}

private const val ANVIL_TEST_PRIVATE_KEY =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
private const val ANVIL_TEST_PORT = "http://10.0.2.2:8545"

class FakeSCWWallet : SigningKey {
    private var web3j: Web3j = Web3j.build(HttpService(ANVIL_TEST_PORT))
    private val contractDeployerCredentials: Credentials =
        Credentials.create(ANVIL_TEST_PRIVATE_KEY)
    var walletAddress: String = ""

    override val address: String
        get() = walletAddress

    override val type: WalletType
        get() = WalletType.SCW

    override var chainId: Long? = 31337L

    companion object {
        fun generate(): FakeSCWWallet {
            return FakeSCWWallet().apply {
                createSmartContractWallet()
            }
        }
    }

    override suspend fun signSCW(message: String): ByteArray {
        val smartWallet = CoinbaseSmartWallet.load(
            walletAddress,
            web3j,
            contractDeployerCredentials,
            DefaultGasProvider()
        )
        val digest = Signature.newBuilder().build().ethHash(message)
        val replaySafeHash = smartWallet.replaySafeHash(digest).send()

        val signature =
            Sign.signMessage(replaySafeHash, contractDeployerCredentials.ecKeyPair, false)
        val signatureBytes = signature.r + signature.s + signature.v
        val tokens = listOf(
            Uint(BigInteger.ZERO),
            DynamicBytes(signatureBytes)
        )
        val encoded = FunctionEncoder.encodeConstructor(tokens)
        val encodedBytes = Numeric.hexStringToByteArray(encoded)

        return encodedBytes
    }

    private fun createSmartContractWallet() {
        val smartWalletContract = CoinbaseSmartWallet.deploy(
            web3j,
            contractDeployerCredentials,
            DefaultGasProvider()
        ).send()

        val factory = CoinbaseSmartWalletFactory.deploy(
            web3j,
            contractDeployerCredentials,
            DefaultGasProvider(),
            BigInteger.ZERO,
            smartWalletContract.contractAddress
        ).send()

        val ownerAddress = ByteArray(32) { 0 }.apply {
            System.arraycopy(contractDeployerCredentials.address.hexToByteArray(), 0, this, 12, 20)
        }
        val owners = listOf(ownerAddress)
        val nonce = BigInteger.ZERO

        val transactionReceipt = factory.createAccount(owners, nonce, BigInteger.ZERO).send()
        val smartWalletAddress = factory.getAddress(owners, nonce).send()

        if (transactionReceipt.isStatusOK) {
            walletAddress = smartWalletAddress
        } else {
            throw Exception("Transaction failed: ${transactionReceipt.status}")
        }
    }
}

data class Fixtures(
    val clientOptions: ClientOptions? = ClientOptions(
        ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false)
    ),
) {
    val aliceAccount = PrivateKeyBuilder()
    val bobAccount = PrivateKeyBuilder()
    val caroAccount = PrivateKeyBuilder()

    var alice: PrivateKey = aliceAccount.getPrivateKey()
    var aliceClient: Client =
        runBlocking { Client().create(account = aliceAccount, options = clientOptions) }

    var bob: PrivateKey = bobAccount.getPrivateKey()
    var bobClient: Client =
        runBlocking { Client().create(account = bobAccount, options = clientOptions) }

    var caro: PrivateKey = caroAccount.getPrivateKey()
    var caroClient: Client =
        runBlocking { Client().create(account = caroAccount, options = clientOptions) }

    fun publishLegacyContact(client: Client) {
        val contactBundle = ContactBundle.newBuilder().also { builder ->
            builder.v1 = builder.v1.toBuilder().also {
                it.keyBundle = client.v1keys.toPublicKeyBundle()
            }.build()
        }.build()
        val envelope = Envelope.newBuilder().apply {
            contentTopic = Topic.contact(client.address).description
            timestampNs = (Date().time * 1_000_000)
            message = contactBundle.toByteString()
        }.build()

        runBlocking { client.publish(envelopes = listOf(envelope)) }
    }
}

fun fixtures(clientOptions: ClientOptions? = null): Fixtures =
    Fixtures(clientOptions)
