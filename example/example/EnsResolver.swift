import Web3Core
import Foundation
import web3swift

// Convert between ENS names and addresses.
// .resolveAddress(forName: "dmccartney.eth") -> "0x359b0ceb2dabcbb6588645de3b480c8203aa5b76"
// .resolveName(forAddress: "0x359b0ceb2dabcbb6588645de3b480c8203aa5b76") -> "dmccartney.eth"
//
// https://docs.ens.domains/resolvers/universal#reverse-resolution
class EnsResolver {
    private let provider: Web3Provider
    private let addrContract: EthereumContract
    private let resolverContract: EthereumContract
    
    // ref: https://docs.ens.domains/learn/deployments
    private let resolverAddress = EthereumAddress("0xce01f8eee7E479C928F8919abD53E553a36CeF67")!

    // This works with any mainnet RPC provider
    //  e.g. `https://eth-mainnet.g.alchemy.com/v2/...`
    convenience init(rpcUrlString: String) {
        self.init(provider: Web3HttpProvider(url: URL(string: rpcUrlString)!, network: .Mainnet))
    }

    init(provider: Web3Provider) {
        self.provider = provider
        self.resolverContract = try! EthereumContract(resolverAbi, at: resolverAddress)
        self.addrContract = try! EthereumContract(addrAbi)
    }
    
    // Get the `name` specified by the `address`'s reverse record.
    //
    // Note: the universal resolver also forward-resolves to ensure a proper claims.
    //
    // https://docs.ens.domains/resolvers/universal#reverse-resolution
    func resolveName(forAddress address: String) async throws -> String? {
        // TODO: consider supporting batch resolution via multicall3.
        let reverseName = dnsPacketToBytes("\(address.dropFirst(2)).addr.reverse")
        let res = try await resolverContract.callStatic(
            "reverse",
            parameters: [reverseName],
            provider: provider
        )
        return res["resolvedName"] as! String?
    }
    
    // Get the address specified by the addr record for the specified `name`.
    //
    // https://docs.ens.domains/resolvers/universal#forward-resolution
    func resolveAddress(forName name: String) async throws -> EthereumAddress? {
        // TODO: consider supporting batch resolution via multicall3.
        // TODO: fully normalize it https://github.com/adraffy/ens-normalize.js
        let encodedName = dnsPacketToBytes(name.lowercased())
        let hashedName = NameHash.nameHash(name)!
        let encodedAddrCall = addrContract.method("addr", parameters: [hashedName], extraData: nil)!
        let res = try await resolverContract.callStatic(
            "resolve",
            parameters: [encodedName, encodedAddrCall],
            provider: provider
        )
        let addressBytes = res["0"] as! Data
        return EthereumAddress("0x\(addressBytes.suffix(20).toHexString())")
    }
    
    // Encodes a DNS packet into a byte array for the ENS contracts.
    //
    // ref: https://docs.ens.domains/resolution/names#dns
    // cribbed from https://github.com/wevm/viem/blob/main/src/utils/ens/packetToBytes.ts
    private func dnsPacketToBytes(_ packet: String) -> Data {
        // strip leading and trailing `.`
        let value = packet.replacing(/^\.|\.$/, with: "")
        if (value.isEmpty) {
            return Data()
        }
        var bytes = Data(count: value.data(using: .utf8)!.count + 2)
        var offset = 0
        let list = value.split(separator: ".")
        for i in 0..<list.count {
            var encoded = String(list[i]).data(using: .utf8)!
            // if the length is > 255, make the encoded label value a labelhash
            // this is compatible with the universal resolver
            if (encoded.count > 255) {
                let encodedHash = "[\(encoded.sha3(.keccak256).toHexString())]"
                encoded = encodedHash.data(using: .utf8)!
            }
            bytes[offset] = UInt8(encoded.count)
            bytes.replaceSubrange(offset + 1..<offset + 1 + encoded.count, with: encoded)
            offset += encoded.count + 1
        }
        
        if (bytes.count != offset + 1) {
            return bytes[0..<offset + 1]
        }
        return bytes
    }
    
    // ref: https://github.com/wevm/viem/blob/main/src/constants/abis.ts
    private let resolverAbi = """
[{
    "name":"resolve",
    "type":"function",
    "stateMutability":"view",
    "inputs":[
        {"type":"bytes","name":"name"},
        {"type":"bytes","name":"data"}
    ],
    "outputs": [
      {"type":"bytes","name":""},
      {"type":"address","name":"address"}
    ]
},{
    "name":"reverse",
    "type":"function",
    "stateMutability":"view",
    "inputs":[{"type":"bytes","name":"reverseName"}],
    "outputs": [
        {"type":"string","name":"resolvedName"},
        {"type":"address","name":"resolvedAddress"},
        {"type":"address","name":"reverseResolver"},
        {"type":"address","name":"resolver"}
    ]
}]
"""
    
    private let addrAbi = """
[{
    "name":"addr",
    "type":"function",
    "stateMutability":"view",
    "inputs": [{"name":"name","type":"bytes32" }],
    "outputs": [
        {"name":"","type":"address"}
    ]
}]
"""
}
