//
//  Contacts.swift
//
//
//  Created by Pat Nakajima on 12/8/22.
//

import Foundation
import LibXMTP

public typealias PrivatePreferencesAction = Xmtp_MessageContents_PrivatePreferencesAction

public enum ConsentState: String, Codable {
	case allowed, denied, unknown
}

public struct ConsentListEntry: Codable, Hashable {
	public enum EntryType: String, Codable {
		case address
	}

	static func address(_ address: String, type: ConsentState = .unknown) -> ConsentListEntry {
		ConsentListEntry(value: address, entryType: .address, consentType: type)
	}

	public var value: String
	public var entryType: EntryType
	public var consentType: ConsentState

	var key: String {
		"\(entryType)-\(value)"
	}
}

public enum ContactError: Error {
	case invalidIdentifier
}

public class ConsentList {
	public var entries: [String: ConsentListEntry] = [:]
	var publicKey: Data
	var privateKey: Data
	var identifier: String?

	var client: Client

	init(client: Client) {
		self.client = client
		privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes
		publicKey = client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
		// swiftlint:disable no_optional_try
		identifier = try? LibXMTP.generatePrivatePreferencesTopicIdentifier(privateKey: privateKey.bytes)
		// swiftlint:enable no_optional_try
	}

	func load() async throws -> ConsentList {
		guard let identifier = identifier else {
			throw ContactError.invalidIdentifier
		}


		let envelopes = try await client.apiClient.envelopes(topic: Topic.preferenceList(identifier).description, pagination: Pagination(direction: .ascending))
		let consentList = ConsentList(client: client)

		var preferences: [PrivatePreferencesAction] = []

		for envelope in envelopes {
			let payload = try LibXMTP.userPreferencesDecrypt(publicKey: publicKey.bytes, privateKey: privateKey.bytes, message: envelope.message.bytes)

			try preferences.append(PrivatePreferencesAction(serializedData: Data(payload)))
		}

		for preference in preferences {
			for address in preference.allow.walletAddresses {
				_ = consentList.allow(address: address)
			}

			for address in preference.block.walletAddresses {
				_ = consentList.deny(address: address)
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

		let message = try LibXMTP.userPreferencesEncrypt(
			publicKey: publicKey.bytes,
			privateKey: privateKey.bytes,
			message: payload.serializedData().bytes
		)

		let envelope = Envelope(
			topic: Topic.preferenceList(identifier),
			timestamp: Date(),
			message: Data(message)
		)

		try await client.publish(envelopes: [envelope])
	}

	func allow(address: String) -> ConsentListEntry {
		let entry = ConsentListEntry.address(address, type: ConsentState.allowed)
		entries[ConsentListEntry.address(address).key] = entry

		return entry
	}

	func deny(address: String) -> ConsentListEntry {
		let entry = ConsentListEntry.address(address, type: ConsentState.denied)
		entries[ConsentListEntry.address(address).key] = entry

		return entry
	}

	func state(address: String) -> ConsentState {
		let entry = entries[ConsentListEntry.address(address).key]

		// swiftlint:disable no_optional_try
		return entry?.consentType ?? .unknown
		// swiftlint:enable no_optional_try
	}
}

/// Provides access to contact bundles.
public actor Contacts {
	var client: Client

	// Save all bundles here
	var knownBundles: [String: ContactBundle] = [:]

	// Whether or not we have sent invite/intro to this contact
	var hasIntroduced: [String: Bool] = [:]

	public var consentList: ConsentList

	init(client: Client) {
		self.client = client
		consentList = ConsentList(client: client)
	}

	public func refreshConsentList() async throws -> ConsentList {
		consentList = try await ConsentList(client: client).load()
		return consentList
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
