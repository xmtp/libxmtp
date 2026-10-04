package uniffi.xmtp_sdk

import org.web3j.abi.FunctionEncoder
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Uint
import org.web3j.crypto.Credentials
import org.web3j.crypto.Sign
import org.web3j.protocol.Web3j
import org.web3j.protocol.http.HttpService
import org.web3j.tx.gas.DefaultGasProvider
import org.web3j.utils.Numeric
import org.xmtp.android.library.BuildConfig
import org.xmtp.android.library.artifact.CoinbaseSmartWallet
import org.xmtp.android.library.artifact.CoinbaseSmartWalletFactory
import java.math.BigInteger

private val ANVIL_TEST_PORT = BuildConfig.ANVIL_URL

class FakeSCWWallet :
    Signer,
    AutoCloseable {
    override fun close() {
        web3j.shutdown()
    }

    private val web3j: Web3j = Web3j.build(HttpService(ANVIL_TEST_PORT))
    private var contractDeployerCredentials: Credentials? = null
    var walletAddress: String = ""

    override suspend fun identity() = PublicIdentity(walletAddress, PublicIdentityKind.ETHEREUM)

    override suspend fun kind() = SignerKind.Scw(31337uL, null)

    companion object {
        fun generate(privateKey: String): FakeSCWWallet =
            FakeSCWWallet().apply {
                contractDeployerCredentials = Credentials.create(privateKey)
                createSmartContractWallet()
            }
    }

    override suspend fun sign(request: SigningRequest): Signature {
        val smartWallet =
            CoinbaseSmartWallet.load(
                walletAddress,
                web3j,
                contractDeployerCredentials,
                DefaultGasProvider(),
            )
        val digest = Sign.getEthereumMessageHash(request.text.toByteArray(Charsets.UTF_8))
        val replaySafeHash = smartWallet.replaySafeHash(digest).send()

        val signature =
            Sign.signMessage(replaySafeHash, contractDeployerCredentials!!.ecKeyPair, false)
        val signatureBytes = signature.r + signature.s + signature.v
        val tokens =
            listOf(
                Uint(BigInteger.ZERO),
                DynamicBytes(signatureBytes),
            )
        val encoded = FunctionEncoder.encodeConstructor(tokens)
        val encodedBytes = Numeric.hexStringToByteArray(encoded)

        return Signature.Scw(encodedBytes, walletAddress, 31337uL, null)
    }

    private fun createSmartContractWallet() {
        val smartWalletContract =
            CoinbaseSmartWallet
                .deploy(
                    web3j,
                    contractDeployerCredentials,
                    DefaultGasProvider(),
                ).send()

        val factory =
            CoinbaseSmartWalletFactory
                .deploy(
                    web3j,
                    contractDeployerCredentials,
                    DefaultGasProvider(),
                    BigInteger.ZERO,
                    smartWalletContract.contractAddress,
                ).send()

        val ownerAddress =
            ByteArray(32) { 0 }.apply {
                System.arraycopy(
                    Numeric.hexStringToByteArray(contractDeployerCredentials!!.address),
                    0,
                    this,
                    12,
                    20,
                )
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
