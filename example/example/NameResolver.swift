import SwiftUI
import XMTPiOS
import web3swift
import Web3Core

// A name resolver for XMTP identities.
//
// By default this returns `0x1234...1234` for the identifier.
// This uses `EnsResolver` for Ethereum addresses.
//
// TODO: support other name resolvers.
@Observable
class NameResolver {
    let ethereum = ObservableCache<String>(defaultValue: { identifier in
        return "\(identifier.prefix(6))...\(identifier.suffix(4))"
    })

    
    // Note: replace this with your own RPC provider URL.
    private let ensResolver = EnsResolver(
        rpcUrlString: "https://eth-mainnet.g.alchemy.com/v2/WV-bLot1hKjjCfpPq603Ro-jViFzwYX8")

    init() {
        ethereum.loader = self.resolveEthereumName
    }
    
    subscript(_ identity: PublicIdentity?) -> ObservableItem<String> {
        if identity == nil {
            return ObservableItem(identifier: "") // nil-ish
        }
        // For ethereum identifiers, try to resolve it.
        if case .ethereum = identity!.kind {
            return ethereum[identity!.identifier]
        }
        // For unknown identifier types, just show the abbreviated edition.
        return ObservableItem(
            identifier: identity!.identifier,
            defaultValue: identity!.abbreviated
        )
    }
    
    func resolveEthereumName(_ identifier: String) async throws -> String {
        if let name = try? await ensResolver.resolveName(forAddress: identifier) {
            return name
        }
        return "\(identifier.prefix(6))...\(identifier.suffix(4))"
    }
}

extension PublicIdentity {
    var abbreviated: String {
        "\(identifier.prefix(6))...\(identifier.suffix(4))"
    }
}

