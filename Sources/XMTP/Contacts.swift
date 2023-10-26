//
//  Contacts.swift
//
//
//  Created by Pat Nakajima on 12/8/22.
//

import Foundation
import XMTPRust


public typealias PrivatePreferencesAction = Xmtp_MessageContents_PrivatePreferencesAction

public enum AllowState: String, Codable {
	case allowed, blocked, unknown
}

struct AllowListEntry: Codable, Hashable {
	enum EntryType: String, Codable {
		case address
	}

	static func address(_ address: String, type: AllowState = .unknown) -> AllowListEntry {
		AllowListEntry(value: address, entryType: .address, permissionType: type)
	}

	var value: String
	var entryType: EntryType
	var permissionType: AllowState

	var key: String {
		"\(entryType)-\(value)"
	}
}

public enum ContactError: Error {
    case invalidIdentifier
}

class AllowList {
	var entries: [String: AllowState] = [:]
    var publicKey: Data
    var privateKey: Data
    var identifier: String?
    
    var client: Client

    init(client: Client) {
        self.client = client
        self.privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes
        self.publicKey = client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
        // swiftlint:disable no_optional_try
        self.identifier = try? XMTPRust.generate_private_preferences_topic_identifier(RustVec(privateKey)).toString()
        // swiftlint:enable no_optional_try
    }

    func load() async throws -> AllowList {
        guard let identifier = identifier else {
            throw ContactError.invalidIdentifier
        }        
        
        let envelopes = try await client.query(topic: .preferenceList(identifier))

		let allowList = AllowList(client: client)
        
        var preferences: [PrivatePreferencesAction] = []

		for envelope in envelopes.envelopes {


			let payload = try XMTPRust.ecies_decrypt_k256_sha3_256(
				RustVec(publicKey),
				RustVec(privateKey),
				RustVec(envelope.message)
			)

            preferences.append(try PrivatePreferencesAction(contiguousBytes: Data(payload).bytes))
		}
        
        preferences.forEach { preference in
            preference.allow.walletAddresses.forEach { address in
                allowList.allow(address: address)
            }
            preference.block.walletAddresses.forEach { address in
                allowList.block(address: address)
            }
        }

		return allowList
	}

	func publish(entry: AllowListEntry) async throws {
        guard let identifier = identifier else {
            throw ContactError.invalidIdentifier
        }

        var payload = PrivatePreferencesAction()
        switch entry.permissionType {
        case .allowed:
            payload.allow.walletAddresses = [entry.value]
        case .blocked:
            payload.block.walletAddresses = [entry.value]
        case .unknown:
            payload.unknownFields
        }

		let message = try XMTPRust.ecies_encrypt_k256_sha3_256(
			RustVec(publicKey),
			RustVec(privateKey),
            RustVec(payload.serializedData())
		)

		let envelope = Envelope(
			topic: Topic.preferenceList(identifier),
			timestamp: Date(),
			message: Data(message)
		)

		try await client.publish(envelopes: [envelope])
	}

	func allow(address: String) -> AllowListEntry {
		entries[AllowListEntry.address(address).key] = .allowed

		return .address(address, type: .allowed)
	}

	func block(address: String) -> AllowListEntry {
		entries[AllowListEntry.address(address).key] = .blocked

		return .address(address, type: .blocked)
	}

	func state(address: String) -> AllowState {
		let state = entries[AllowListEntry.address(address).key]

		return state ?? .unknown
	}
}

/// Provides access to contact bundles.
public actor Contacts {
	var client: Client

	// Save all bundles here
	var knownBundles: [String: ContactBundle] = [:]

	// Whether or not we have sent invite/intro to this contact
	var hasIntroduced: [String: Bool] = [:]

    var allowList: AllowList
	
    init(client: Client) {
		self.client = client
        self.allowList = AllowList(client: client)
	}

	public func refreshAllowList() async throws {
		self.allowList = try await AllowList(client: client).load()
	}

	public func isAllowed(_ address: String) -> Bool {
		return allowList.state(address: address) == .allowed
	}

	public func isBlocked(_ address: String) -> Bool {
		return allowList.state(address: address) == .blocked
	}

	public func allow(addresses: [String]) async throws {
		for address in addresses {
			try await AllowList(client: client).publish(entry: allowList.allow(address: address))
		}
	}

	public func block(addresses: [String]) async throws {
		for address in addresses {
			try await AllowList(client: client).publish(entry: allowList.block(address: address))
		}
	}

	func markIntroduced(_ peerAddress: String, _ isIntroduced: Bool) {
		hasIntroduced[peerAddress] = isIntroduced
	}

	func has(_ peerAddress: String) -> Bool {
		return knownBundles[peerAddress] != nil
	}

	func needsIntroduction(_ peerAddress: String) -> Bool {
		return hasIntroduced[peerAddress] != true
	}

	func find(_ peerAddress: String) async throws -> ContactBundle? {
		if let knownBundle = knownBundles[peerAddress] {
			return knownBundle
		}

		let response = try await client.query(topic: .contact(peerAddress))

		for envelope in response.envelopes {
			// swiftlint:disable no_optional_try
			if let contactBundle = try? ContactBundle.from(envelope: envelope) {
				knownBundles[peerAddress] = contactBundle

				return contactBundle
			}
			// swiftlint:enable no_optional_try
		}

		return nil
	}
}
