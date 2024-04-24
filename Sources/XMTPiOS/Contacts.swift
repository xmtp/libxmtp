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
		case address, groupId
	}

	static func address(_ address: String, type: ConsentState = .unknown) -> ConsentListEntry {
		ConsentListEntry(value: address, entryType: .address, consentType: type)
	}
	
	static func groupId(groupId: String, type: ConsentState = ConsentState.unknown) -> ConsentListEntry {
		ConsentListEntry(value: groupId, entryType: .groupId, consentType: type)
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

public actor EntriesManager {
	public var map: [String: ConsentListEntry] = [:]

	func set(_ key: String, _ object: ConsentListEntry) {
		map[key] = object
	}

	func get(_ key: String) -> ConsentListEntry? {
		map[key]
	}
}

public class ConsentList {
	public let entriesManager = EntriesManager()
	var publicKey: Data
	var privateKey: Data
	var identifier: String?
	var lastFetched: Date?
	var client: Client

	init(client: Client) {
		self.client = client
		privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes
		publicKey = client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
		identifier = try? LibXMTP.generatePrivatePreferencesTopicIdentifier(privateKey: privateKey)
	}

	func load() async throws -> [ConsentListEntry] {
		guard let identifier = identifier else {
			throw ContactError.invalidIdentifier
		}
		let newDate = Date()

		let pagination = Pagination(
			limit: 500,
            after: lastFetched,
            direction: .ascending
        )
		let envelopes = try await client.apiClient.envelopes(topic: Topic.preferenceList(identifier).description, pagination: pagination)
    lastFetched = newDate

		var preferences: [PrivatePreferencesAction] = []

		for envelope in envelopes {
			let payload = try LibXMTP.userPreferencesDecrypt(publicKey: publicKey, privateKey: privateKey, message: envelope.message)

			try preferences.append(PrivatePreferencesAction(serializedData: Data(payload)))
		}
		for preference in preferences {
			for address in preference.allowAddress.walletAddresses {
				_ = await allow(address: address)
			}

			for address in preference.denyAddress.walletAddresses {
				_ = await deny(address: address)
			}

			for groupId in preference.allowGroup.groupIds {
				_ = await allowGroup(groupId: groupId)
			}

			for groupId in preference.denyGroup.groupIds {
				_ = await denyGroup(groupId: groupId)
			}
		}

		return await Array(entriesManager.map.values)
	}

    func publish(entries: [ConsentListEntry]) async throws {
      guard let identifier = identifier else {
        throw ContactError.invalidIdentifier
      }
      var payload = PrivatePreferencesAction()

      for entry in entries {
        switch entry.entryType {

    	    case .address:
    	      switch entry.consentType {
    	      case .allowed:
              	payload.allowAddress.walletAddresses.append(entry.value)
    	      case .denied:
    	          payload.denyAddress.walletAddresses.append(entry.value)
    	      case .unknown:
    	          payload.messageType = nil
    	      }

    			case .groupId:
    	    	switch entry.consentType {
    	    	case .allowed:
    	    	    if let valueData = entry.value.data(using: .utf8) {
    	    	        payload.allowGroup.groupIds.append(valueData)
    	    	    }
    	    	case .denied:
    	    	    if let valueData = entry.value.data(using: .utf8) {
    	    	            payload.denyGroup.groupIds.append(valueData)
    	    	    }
    	    	case .unknown:
    	    	    payload.messageType = nil
    	    }
    	}

    }

    let message = try LibXMTP.userPreferencesEncrypt(
        publicKey: publicKey,
        privateKey: privateKey,
        message: payload.serializedData()
    )

    let envelope = Envelope(
        topic: Topic.preferenceList(identifier),
        timestamp: Date(),
        message: Data(message)
    )

    try await client.publish(envelopes: [envelope])
  }

	func allow(address: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.address(address, type: ConsentState.allowed)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func deny(address: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.address(address, type: ConsentState.denied)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func allowGroup(groupId: Data) async -> ConsentListEntry {
		let groupIdString = groupId.toHex
		let entry = ConsentListEntry.groupId(groupId: groupIdString, type: ConsentState.allowed)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func denyGroup(groupId: Data) async -> ConsentListEntry {
		let groupIdString = groupId.toHex
		let entry = ConsentListEntry.groupId(groupId: groupIdString, type: ConsentState.denied)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func state(address: String) async -> ConsentState {
		guard let entry = await entriesManager.get(ConsentListEntry.address(address).key) else {
			return .unknown
		}

		return entry.consentType
	}

	func groupState(groupId: Data) async -> ConsentState {
		guard let entry =  await entriesManager.get(ConsentListEntry.groupId(groupId: groupId.toHex).key) else {
			return .unknown
		}

		return entry.consentType
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
		_ = try await consentList.load()
		return consentList
	}

	public func isAllowed(_ address: String) async -> Bool {
		return await consentList.state(address: address) == .allowed
	}

	public func isDenied(_ address: String) async -> Bool {
		return await consentList.state(address: address) == .denied
	}

	public func isGroupAllowed(groupId: Data) async -> Bool {
		return await consentList.groupState(groupId: groupId) == .allowed
	}

	public func isGroupDenied(groupId: Data) async -> Bool {
		return await consentList.groupState(groupId: groupId) == .denied
	}

	public func allow(addresses: [String]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for address in addresses {
				group.addTask {
					return await self.consentList.allow(address: address)
				}
			}

			for try await entry in group {
				entries.append(entry)
			}
		}
        try await consentList.publish(entries: entries)
	}

	public func deny(addresses: [String]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for address in addresses {
				group.addTask {
					return await self.consentList.deny(address: address)
				}
			}

			for try await entry in group {
				entries.append(entry)
			}
		}
        try await consentList.publish(entries: entries)
	}

	public func allowGroup(groupIds: [Data]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for groupId in groupIds {
				group.addTask {
					return await self.consentList.allowGroup(groupId: groupId)
				}
			}

			for try await entry in group {
				entries.append(entry)
			}
		}
        try await consentList.publish(entries: entries)
	}

	public func denyGroup(groupIds: [Data]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for groupId in groupIds {
				group.addTask {
					return await self.consentList.denyGroup(groupId: groupId)
				}
			}

			for try await entry in group {
				entries.append(entry)
			}
		}        
		try await consentList.publish(entries: entries)
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
			if let contactBundle = try? ContactBundle.from(envelope: envelope) {
				knownBundles[peerAddress] = contactBundle
				return contactBundle
			}
		}
		return nil
	}
}
