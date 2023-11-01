//
//  Contacts.swift
//
//
//  Created by Pat Nakajima on 12/8/22.
//

import Foundation
import XMTPRust


public typealias PrivatePreferencesAction = Xmtp_MessageContents_PrivatePreferencesAction

public enum ConsentState: String, Codable {
	case allowed, denied, unknown
}

struct ConsentListEntry: Codable, Hashable {
	enum EntryType: String, Codable {
		case address
	}

	static func address(_ address: String, type: ConsentState = .unknown) -> ConsentListEntry {
		ConsentListEntry(value: address, entryType: .address, consentType: type)
	}

	var value: String
	var entryType: EntryType
	var consentType: ConsentState

	var key: String {
		"\(entryType)-\(value)"
	}
}

public enum ContactError: Error {
    case invalidIdentifier
}

class ConsentList {
	var entries: [String: ConsentState] = [:]
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

    func load() async throws -> ConsentList {
        guard let identifier = identifier else {
            throw ContactError.invalidIdentifier
        }        
        
        let envelopes = try await client.query(topic: .preferenceList(identifier), pagination: Pagination(direction: .ascending))

		let consentList = ConsentList(client: client)
        
        var preferences: [PrivatePreferencesAction] = []

		for envelope in envelopes.envelopes {


			let payload = try XMTPRust.ecies_decrypt_k256_sha3_256(
				RustVec(publicKey),
				RustVec(privateKey),
				RustVec(envelope.message)
			)

            preferences.append(try PrivatePreferencesAction(serializedData: Data(payload)))
		}
        
        preferences.forEach { preference in
            preference.allow.walletAddresses.forEach { address in
                consentList.allow(address: address)
            }
            preference.block.walletAddresses.forEach { address in
                consentList.deny(address: address)
            }
        }

		return consentList
	}

	func publish(entry: ConsentListEntry) async throws {
        guard let identifier = identifier else {
            throw ContactError.invalidIdentifier
        }

        var payload = PrivatePreferencesAction()
        switch entry.consentType {
        case .allowed:
            payload.allow.walletAddresses = [entry.value]
        case .denied:
            payload.block.walletAddresses = [entry.value]
        case .unknown:
            payload.messageType = nil
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

	func allow(address: String) -> ConsentListEntry {
		entries[ConsentListEntry.address(address).key] = .allowed

		return .address(address, type: .allowed)
	}

	func deny(address: String) -> ConsentListEntry {
		entries[ConsentListEntry.address(address).key] = .denied

		return .address(address, type: .denied)
	}

	func state(address: String) -> ConsentState {
		let state = entries[ConsentListEntry.address(address).key]

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

    var consentList: ConsentList
	
    init(client: Client) {
		self.client = client
        self.consentList = ConsentList(client: client)
	}

	public func refreshConsentList() async throws {
		self.consentList = try await ConsentList(client: client).load()
	}

	public func isAllowed(_ address: String) -> Bool {
		return consentList.state(address: address) == .allowed
	}

	public func isDenied(_ address: String) -> Bool {
		return consentList.state(address: address) == .denied
	}

	public func allow(addresses: [String]) async throws {
		for address in addresses {
			try await ConsentList(client: client).publish(entry: consentList.allow(address: address))
		}
	}

	public func deny(addresses: [String]) async throws {
		for address in addresses {
			try await ConsentList(client: client).publish(entry: consentList.deny(address: address))
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
