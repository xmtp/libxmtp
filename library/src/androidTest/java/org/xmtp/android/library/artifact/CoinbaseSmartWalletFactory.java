package org.xmtp.android.library.artifact;

import org.web3j.abi.FunctionEncoder;
import org.web3j.abi.TypeReference;
import org.web3j.abi.datatypes.Address;
import org.web3j.abi.datatypes.Function;
import org.web3j.abi.datatypes.generated.Bytes32;
import org.web3j.crypto.Credentials;
import org.web3j.protocol.Web3j;
import org.web3j.protocol.core.RemoteCall;
import org.web3j.protocol.core.RemoteFunctionCall;
import org.web3j.protocol.core.methods.response.TransactionReceipt;
import org.web3j.tx.Contract;
import org.web3j.tx.TransactionManager;
import org.web3j.tx.gas.ContractGasProvider;

import java.math.BigInteger;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/**
 * <p>Auto generated code.
 * <p><strong>Do not modify!</strong>
 * <p>Please use the <a href="https://docs.web3j.io/command_line.html">web3j command line tools</a>,
 * or the org.web3j.codegen.SolidityFunctionWrapperGenerator in the
 * <a href="https://github.com/hyperledger/web3j/tree/main/codegen">codegen module</a> to update.
 *
 * <p>Generated with web3j version 1.6.1.
 */
@SuppressWarnings("rawtypes")
public class CoinbaseSmartWalletFactory extends Contract {
    public static final String BINARY = "0x60a06040526040516105eb3803806105eb83398101604081905261002291610033565b6001600160a01b0316608052610063565b60006020828403121561004557600080fd5b81516001600160a01b038116811461005c57600080fd5b9392505050565b60805161056061008b6000396000818160a60152818161013c015261023b01526105606000f3fe60806040526004361061003f5760003560e01c8063250b1b41146100445780633ffba36f146100815780635c60da1b14610094578063db4c545e146100c8575b600080fd5b34801561005057600080fd5b5061006461005f3660046103b7565b6100eb565b6040516001600160a01b0390911681526020015b60405180910390f35b61006461008f3660046103b7565b610111565b3480156100a057600080fd5b506100647f000000000000000000000000000000000000000000000000000000000000000081565b3480156100d457600080fd5b506100dd6101e6565b604051908152602001610078565b60006101096100f86101e6565b61010386868661027b565b306102b1565b949350505050565b600082810361013357604051633c776be160e01b815260040160405180910390fd5b60008061016b347f000000000000000000000000000000000000000000000000000000000000000061016689898961027b565b6102d3565b935091508290508115156000036101dd57604051633796f38760e11b81526001600160a01b03841690636f2de70e906101aa90899089906004016104f2565b600060405180830381600087803b1580156101c457600080fd5b505af11580156101d8573d6000803e3d6000fd5b505050505b50509392505050565b604080517fcc3735a920a3ca505d382bbc545af43d6000803e6038573d6000fd5b3d6000f360609081527f5155f3363d3d373d3d363d7f360894a13ba1a3210667c828492db98dca3e207683526160096020527f0000000000000000000000000000000000000000000000000000000000000000601e5268603d3d8160223d3973600a52605f60212091909252600090915290565b600083838360405160200161029293929190610506565b6040516020818303038152906040528051906020012090509392505050565b600060ff60005350603592835260601b60015260155260556000908120915290565b6000806040517fcc3735a920a3ca505d382bbc545af43d6000803e6038573d6000fd5b3d6000f36060527f5155f3363d3d373d3d363d7f360894a13ba1a3210667c828492db98dca3e207660405261600960205284601e5268603d3d8160223d3973600a52605f60212060358201523060581b815260ff8153836015820152605581209150813b61037f5783605f602188f591508161037a5763301164256000526004601cfd5b6103a5565b6001925085156103a55760003860003889865af16103a55763b12d13eb6000526004601cfd5b80604052506000606052935093915050565b6000806000604084860312156103cc57600080fd5b833567ffffffffffffffff808211156103e457600080fd5b818601915086601f8301126103f857600080fd5b81358181111561040757600080fd5b8760208260051b850101111561041c57600080fd5b6020928301989097509590910135949350505050565b81835281816020850137506000828201602090810191909152601f909101601f19169091010190565b6000838385526020808601955060208560051b8301018460005b878110156104e557848303601f19018952813536889003601e1901811261049b57600080fd5b8701848101903567ffffffffffffffff8111156104b757600080fd5b8036038213156104c657600080fd5b6104d1858284610432565b9a86019a9450505090830190600101610475565b5090979650505050505050565b60208152600061010960208301848661045b565b60408152600061051a60408301858761045b565b905082602083015294935050505056fea264697066735822122098bae64e62859ac8d5ed01c4927e5fce406f632b517c86f038f06fef8355dba164736f6c63430008170033";

    private static String librariesLinkedBinary;

    public static final String FUNC_CREATEACCOUNT = "createAccount";

    public static final String FUNC_GETADDRESS = "getAddress";

    public static final String FUNC_IMPLEMENTATION = "implementation";

    public static final String FUNC_INITCODEHASH = "initCodeHash";

    @Deprecated
    protected CoinbaseSmartWalletFactory(String contractAddress, Web3j web3j,
                                         Credentials credentials, BigInteger gasPrice, BigInteger gasLimit) {
        super(BINARY, contractAddress, web3j, credentials, gasPrice, gasLimit);
    }

    protected CoinbaseSmartWalletFactory(String contractAddress, Web3j web3j,
                                         Credentials credentials, ContractGasProvider contractGasProvider) {
        super(BINARY, contractAddress, web3j, credentials, contractGasProvider);
    }

    @Deprecated
    protected CoinbaseSmartWalletFactory(String contractAddress, Web3j web3j,
                                         TransactionManager transactionManager, BigInteger gasPrice, BigInteger gasLimit) {
        super(BINARY, contractAddress, web3j, transactionManager, gasPrice, gasLimit);
    }

    protected CoinbaseSmartWalletFactory(String contractAddress, Web3j web3j,
                                         TransactionManager transactionManager, ContractGasProvider contractGasProvider) {
        super(BINARY, contractAddress, web3j, transactionManager, contractGasProvider);
    }

    public RemoteFunctionCall<TransactionReceipt> createAccount(List<byte[]> owners,
                                                                BigInteger nonce, BigInteger weiValue) {
        final Function function = new Function(
                FUNC_CREATEACCOUNT,
                Arrays.asList(new org.web3j.abi.datatypes.DynamicArray<org.web3j.abi.datatypes.DynamicBytes>(
                                org.web3j.abi.datatypes.DynamicBytes.class,
                                org.web3j.abi.Utils.typeMap(owners, org.web3j.abi.datatypes.DynamicBytes.class)),
                        new org.web3j.abi.datatypes.generated.Uint256(nonce)),
                Collections.emptyList());
        return executeRemoteCallTransaction(function, weiValue);
    }

    public RemoteFunctionCall<String> getAddress(List<byte[]> owners, BigInteger nonce) {
        final Function function = new Function(FUNC_GETADDRESS,
                Arrays.asList(new org.web3j.abi.datatypes.DynamicArray<org.web3j.abi.datatypes.DynamicBytes>(
                                org.web3j.abi.datatypes.DynamicBytes.class,
                                org.web3j.abi.Utils.typeMap(owners, org.web3j.abi.datatypes.DynamicBytes.class)),
                        new org.web3j.abi.datatypes.generated.Uint256(nonce)),
                List.of(new TypeReference<Address>() {
                }));
        return executeRemoteCallSingleValueReturn(function, String.class);
    }

    public RemoteFunctionCall<String> implementation() {
        final Function function = new Function(FUNC_IMPLEMENTATION,
                List.of(),
                List.of(new TypeReference<Address>() {
                }));
        return executeRemoteCallSingleValueReturn(function, String.class);
    }

    public RemoteFunctionCall<byte[]> initCodeHash() {
        final Function function = new Function(FUNC_INITCODEHASH,
                List.of(),
                List.of(new TypeReference<Bytes32>() {
                }));
        return executeRemoteCallSingleValueReturn(function, byte[].class);
    }

    @Deprecated
    public static CoinbaseSmartWalletFactory load(String contractAddress, Web3j web3j,
                                                  Credentials credentials, BigInteger gasPrice, BigInteger gasLimit) {
        return new CoinbaseSmartWalletFactory(contractAddress, web3j, credentials, gasPrice, gasLimit);
    }

    @Deprecated
    public static CoinbaseSmartWalletFactory load(String contractAddress, Web3j web3j,
                                                  TransactionManager transactionManager, BigInteger gasPrice, BigInteger gasLimit) {
        return new CoinbaseSmartWalletFactory(contractAddress, web3j, transactionManager, gasPrice, gasLimit);
    }

    public static CoinbaseSmartWalletFactory load(String contractAddress, Web3j web3j,
                                                  Credentials credentials, ContractGasProvider contractGasProvider) {
        return new CoinbaseSmartWalletFactory(contractAddress, web3j, credentials, contractGasProvider);
    }

    public static CoinbaseSmartWalletFactory load(String contractAddress, Web3j web3j,
                                                  TransactionManager transactionManager, ContractGasProvider contractGasProvider) {
        return new CoinbaseSmartWalletFactory(contractAddress, web3j, transactionManager, contractGasProvider);
    }

    public static RemoteCall<CoinbaseSmartWalletFactory> deploy(Web3j web3j,
                                                                Credentials credentials, ContractGasProvider contractGasProvider,
                                                                BigInteger initialWeiValue, String erc4337) {
        String encodedConstructor = FunctionEncoder.encodeConstructor(List.of(new Address(160, erc4337)));
        return deployRemoteCall(CoinbaseSmartWalletFactory.class, web3j, credentials, contractGasProvider, getDeploymentBinary(), encodedConstructor, initialWeiValue);
    }

    public static RemoteCall<CoinbaseSmartWalletFactory> deploy(Web3j web3j,
                                                                TransactionManager transactionManager, ContractGasProvider contractGasProvider,
                                                                BigInteger initialWeiValue, String erc4337) {
        String encodedConstructor = FunctionEncoder.encodeConstructor(List.of(new Address(160, erc4337)));
        return deployRemoteCall(CoinbaseSmartWalletFactory.class, web3j, transactionManager, contractGasProvider, getDeploymentBinary(), encodedConstructor, initialWeiValue);
    }

    @Deprecated
    public static RemoteCall<CoinbaseSmartWalletFactory> deploy(Web3j web3j,
                                                                Credentials credentials, BigInteger gasPrice, BigInteger gasLimit,
                                                                BigInteger initialWeiValue, String erc4337) {
        String encodedConstructor = FunctionEncoder.encodeConstructor(List.of(new Address(160, erc4337)));
        return deployRemoteCall(CoinbaseSmartWalletFactory.class, web3j, credentials, gasPrice, gasLimit, getDeploymentBinary(), encodedConstructor, initialWeiValue);
    }

    @Deprecated
    public static RemoteCall<CoinbaseSmartWalletFactory> deploy(Web3j web3j,
                                                                TransactionManager transactionManager, BigInteger gasPrice, BigInteger gasLimit,
                                                                BigInteger initialWeiValue, String erc4337) {
        String encodedConstructor = FunctionEncoder.encodeConstructor(List.of(new Address(160, erc4337)));
        return deployRemoteCall(CoinbaseSmartWalletFactory.class, web3j, transactionManager, gasPrice, gasLimit, getDeploymentBinary(), encodedConstructor, initialWeiValue);
    }

    private static String getDeploymentBinary() {
        if (librariesLinkedBinary != null) {
            return librariesLinkedBinary;
        } else {
            return BINARY;
        }
    }
}
