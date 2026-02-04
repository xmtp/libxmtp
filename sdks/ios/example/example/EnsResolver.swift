import BigInt
import Foundation

// Convert between ENS names and addresses.
// .resolveAddress(forName: "dmccartney.eth") -> "0x359b0ceb2dabcbb6588645de3b480c8203aa5b76"
// .resolveName(forAddress: "0x359b0ceb2dabcbb6588645de3b480c8203aa5b76") -> "dmccartney.eth"
//
// Note: to avoid dependencies this does its own ABI encoding of call data.
//       Consider using a more robust ABI encoder if you have one handy.
//
// ref:
// https://docs.ens.domains/resolvers/universal#reverse-resolution
class EnsResolver {
	private let rpcUrl: URL

	// ref: https://docs.ens.domains/learn/deployments
	private let resolverAddress = "0xce01f8eee7E479C928F8919abD53E553a36CeF67".lowercased()

	/// This works with any mainnet RPC provider
	///  e.g. `https://eth-mainnet.g.alchemy.com/v2/...`
	convenience init(rpcUrlString: String) {
		// swiftlint:disable:next force_unwrapping
		self.init(rpcUrl: URL(string: rpcUrlString)!)
	}

	init(rpcUrl: URL) {
		self.rpcUrl = rpcUrl
	}

	/// Get the `name` specified by the `address`'s reverse record.
	///
	/// Note: the universal resolver also forward-resolves to ensure a proper claims.
	///
	/// https://docs.ens.domains/resolvers/universal#reverse-resolution
	func resolveName(forAddress address: String) async throws -> String? {
		// TODO: consider supporting batch resolution via multicall3.

		// Produce the dns-packed edition of the reverse record name.
		var reverseName = dnsPacketToBytes("\(address.dropFirst(2)).addr.reverse")

		// This performs extremely lightweight ABI encoding of the expected input/output.
		// ref: universalResolverReverseAbi
		// ref: https://github.com/wevm/viem/blob/main/src/constants/abis.ts
		//     "name":"reverse",
		//     "inputs":[{"type":"bytes","name":"reverseName"}],
		//     "outputs": [
		//         {"type":"string","name":"resolvedName"},
		//         {"type":"address","name":"resolvedAddress"},
		//         {"type":"address","name":"reverseResolver"},
		//         {"type":"address","name":"resolver"}
		//     ]
		let reverseCallEncoded = Data(hex: "ec11c823") + // 8-byte method identifier for reverse(bytes)
			BigUInt(32).serialize().wordPaddedLeft() +
			BigUInt(reverseName.count).serialize().wordPaddedLeft() +
			reverseName.wordPaddedRight()

		let result = try await EthereumRPCRequest.ethCall(
			using: rpcUrl,
			to: resolverAddress,
			data: reverseCallEncoded
		)

		let resolvedNameLength = UInt64(BigUInt(result[128 ..< 160]))
		let resolvedNameData = result[160 ..< 160 + resolvedNameLength]
		if let resolvedName = String(data: resolvedNameData, encoding: .utf8) {
			return resolvedName
		}
		return nil
	}

	/// Get the address specified by the addr record for the specified `name`.
	///
	/// https://docs.ens.domains/resolvers/universal#forward-resolution
	func resolveAddress(forName name: String) async throws -> String? {
		// TODO: consider supporting batch resolution via multicall3.

		// TODO: fully normalize it https://github.com/adraffy/ens-normalize.js
		let encodedName = dnsPacketToBytes(name.lowercased())

		// This encodes the nested call to addr(name)
		//  ref: addressResolverAbi
		//  ref: https://github.com/wevm/viem/blob/main/src/constants/abis.ts
		//     "name":"addr",
		//     "inputs":[{"type":"bytes32","name":"name"}],
		//     "outputs": [{"type":"address","name":""}]
		let encodedAddrCall = Data(hex: "3b3b57de") + (namehash(name.lowercased()) ?? Data())

		// This performs extremely lightweight ABI encoding of the expected input/output.
		//  ref: universalResolverResolveAbi
		//  ref: https://github.com/wevm/viem/blob/main/src/constants/abis.ts
		//     "name":"resolve",
		//     "inputs":[
		//         {"type":"bytes","name":"name"},
		//         {"type":"bytes","name":"data"}
		//     ],
		//     "outputs": [
		//         {"type":"bytes","name":""},
		//         {"type":"address","name":"address"}
		//     ]

		// The byte-offsets where each of `encodedName` and `encodedAddrCall` begin in the encoding.
		let encodedNameOffset = 32 * 2
		let encodedAddrCallOffset = 32 * (3 + encodedName.count.requiredWords()) // i.e. after the encodedName

		// Bring it all together (with the offsets in the header)
		let encodedResolveCall = Data(hex: "9061b923") + // 8-byte signature for resolve(bytes,bytes)
			BigUInt(encodedNameOffset).serialize().wordPaddedLeft() +
			BigUInt(encodedAddrCallOffset).serialize().wordPaddedLeft() +
			// encodedName bytes (length-prefixed):
			BigUInt(encodedName.count).serialize().wordPaddedLeft() +
			encodedName.wordPaddedRight() +
			// encodedAddrCall bytes (length-prefixed):
			BigUInt(encodedAddrCall.count).serialize().wordPaddedLeft() +
			encodedAddrCall.wordPaddedRight()

		let result = try await EthereumRPCRequest.ethCall(
			using: rpcUrl,
			to: resolverAddress,
			data: encodedResolveCall
		)

		// Note: this works because the address is encoded in the last bytes of the output.
		return "0x\(result.suffix(20).toHexString())".lowercased()
	}

	// Encodes a DNS packet into a byte array for the ENS contracts.
	//
	// ref: https://docs.ens.domains/resolution/names#dns
	// cribbed from https://github.com/wevm/viem/blob/main/src/utils/ens/packetToBytes.ts
	private func dnsPacketToBytes(_ packet: String) -> Data {
		// strip leading and trailing `.`
		let value = packet.replacing(/^\.|\.$/, with: "")
		if value.isEmpty {
			return Data()
		}
		var bytes = Data(count: value.data(using: .utf8)?.count ?? 0 + 2)
		var offset = 0
		let list = value.split(separator: ".")
		for i in 0 ..< list.count {
			var encoded = String(list[i]).data(using: .utf8) ?? Data()
			// if the length is > 255, make the encoded label value a labelhash
			// this is compatible with the universal resolver
			if encoded.count > 255 {
				let encodedHash = "[\(encoded.sha3(.keccak256).toHexString())]"
				encoded = encodedHash.data(using: .utf8) ?? Data()
			}
			bytes[offset] = UInt8(encoded.count)
			bytes.replaceSubrange(offset + 1 ..< offset + 1 + encoded.count, with: encoded)
			offset += encoded.count + 1
		}

		if bytes.count != offset + 1 {
			return bytes[0 ..< offset + 1]
		}
		return bytes
	}

	// ref: https://docs.ens.domains/resolution/names#namehash
	private func namehash(_ name: String) -> Data? {
		if name.isEmpty {
			return Data(repeating: 0x00, count: 32)
		}
		let parts = name.split(separator: ".")
		guard let label = parts.first?.data(using: .utf8) else {
			return nil
		}
		guard let remainder = namehash(parts[1...].joined(separator: ".")) else {
			return nil
		}
		return (remainder + label.sha3(.keccak256)).sha3(.keccak256)
	}
}

private struct EthereumRPCRequest: Encodable {
	var jsonrpc = "2.0"
	var id: Int
	var method: String
	var params: [Encodable]

	enum EthereumRPCError: Error {
		case error(String)
	}

	static func ethCall(using providerUrl: URL, to: String, data: Data) async throws -> Data {
		var ethRpcRequest = EthereumRPCRequest(
			jsonrpc: "2.0",
			id: Int.random(in: 0 ..< Int.max),
			method: "eth_call",
			params: [
				Call(
					from: "0x0000000000000000000000000000000000000000",
					to: to,
					data: "0x\(data.toHexString())"
				),
				"latest",
			]
		)
		var req = URLRequest(
			url: providerUrl,
			cachePolicy: .reloadIgnoringCacheData
		)
		req.setValue("application/json", forHTTPHeaderField: "Content-Type")
		req.setValue("application/json", forHTTPHeaderField: "Accept")
		req.httpMethod = "POST"
		req.httpBody = ethRpcRequest.encoded

		let (data, response) = try await URLSession.shared.data(for: req)
		guard let statusCode = (response as? HTTPURLResponse)?.statusCode,
		      200 ..< 400 ~= statusCode,
		      let res = try? JSONDecoder().decode(EthereumRPCResponse.self, from: data)
		else {
			throw URLError(.badServerResponse)
		}
		guard let resultHex = res.result else {
			throw EthereumRPCError.error(res.error ?? "")
		}
		return Data(hex: resultHex)
	}

	enum CodingKeys: String, CodingKey {
		case jsonrpc
		case id
		case method
		case params
	}

	struct Call: Codable {
		let from: String
		let to: String
		let data: String
	}

	func encode(to encoder: Encoder) throws {
		var container = encoder.container(keyedBy: CodingKeys.self)
		try container.encode(jsonrpc, forKey: .jsonrpc)
		try container.encode(id, forKey: .id)
		try container.encode(method, forKey: .method)

		var paramsContainer = container.superEncoder(forKey: .params).unkeyedContainer()
		try params.forEach { a in
			try paramsContainer.encode(a)
		}
	}

	var encoded: Data {
		(try? JSONEncoder().encode(self)) ?? Data()
	}
}

private struct EthereumRPCResponse: Decodable {
	var id: Int
	var jsonrpc = "2.0"
	var result: String?
	var error: String?
}

private extension Data {
	func paddingLeft(_ toLength: Int, padding _: UInt8 = 0x00) -> Data {
		if count >= toLength {
			return Data(self)
		}
		return Data(repeating: 0x00, count: toLength - count) + self
	}

	func paddingRight(_ toLength: Int, padding _: UInt8 = 0x00) -> Data {
		if count >= toLength {
			return Data(self)
		}
		return self + Data(repeating: 0x00, count: toLength - count)
	}

	func wordPaddedLeft(_ wordSize: UInt8 = 32) -> Data {
		paddingLeft(Int(wordSize) * count.requiredWords(wordSize))
	}

	func wordPaddedRight(_ wordSize: UInt8 = 32) -> Data {
		paddingRight(Int(wordSize) * count.requiredWords(wordSize))
	}
}

private extension Int {
	func requiredWords(_ wordSize: UInt8 = 32) -> Int {
		((self - 1) / Int(wordSize)) + 1
	}
}
