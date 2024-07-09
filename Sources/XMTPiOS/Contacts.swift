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
		case address, group_id, inbox_id
	}

	static func address(_ address: String, type: ConsentState = .unknown) -> ConsentListEntry {
		ConsentListEntry(value: address, entryType: .address, consentType: type)
	}
	
	static func groupId(groupId: String, type: ConsentState = ConsentState.unknown) -> ConsentListEntry {
		ConsentListEntry(value: groupId, entryType: .group_id, consentType: type)
	}
	
	static func inboxId(_ inboxId: String, type: ConsentState = .unknown) -> ConsentListEntry {
		ConsentListEntry(value: inboxId, entryType: .inbox_id, consentType: type)
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
			
			for inboxId in preference.allowInboxID.inboxIds {
				_ = await allowInboxId(inboxId: inboxId)
			}

			for inboxId in preference.denyInboxID.inboxIds {
				_ = await denyInboxId(inboxId: inboxId)
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
		case .group_id:
			switch entry.consentType {
			case .allowed:
				payload.allowGroup.groupIds.append(entry.value)
			case .denied:
				payload.denyGroup.groupIds.append(entry.value)
			case .unknown:
				payload.messageType = nil
    	    }
		case .inbox_id:
			switch entry.consentType {
			case .allowed:
			  payload.allowInboxID.inboxIds.append(entry.value)
			case .denied:
				payload.denyInboxID.inboxIds.append(entry.value)
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

	func allowGroup(groupId: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.groupId(groupId: groupId, type: ConsentState.allowed)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func denyGroup(groupId: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.groupId(groupId: groupId, type: ConsentState.denied)
		await entriesManager.set(entry.key, entry)

		return entry
	}
	
	func allowInboxId(inboxId: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.inboxId(inboxId, type: ConsentState.allowed)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func denyInboxId(inboxId: String) async -> ConsentListEntry {
		let entry = ConsentListEntry.inboxId(inboxId, type: ConsentState.denied)
		await entriesManager.set(entry.key, entry)

		return entry
	}

	func state(address: String) async -> ConsentState {
		guard let entry = await entriesManager.get(ConsentListEntry.address(address).key) else {
			return .unknown
		}

		return entry.consentType
	}

	func groupState(groupId: String) async -> ConsentState {
		guard let entry =  await entriesManager.get(ConsentListEntry.groupId(groupId: groupId).key) else {
			return .unknown
		}

		return entry.consentType
	}
	
	func inboxIdState(inboxId: String) async -> ConsentState {
		guard let entry = await entriesManager.get(ConsentListEntry.inboxId(inboxId).key) else {
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

	public func isGroupAllowed(groupId: String) async -> Bool {
		return await consentList.groupState(groupId: groupId) == .allowed
	}

	public func isGroupDenied(groupId: String) async -> Bool {
		return await consentList.groupState(groupId: groupId) == .denied
	}
	
	public func isInboxAllowed(inboxId: String) async -> Bool {
		return await consentList.inboxIdState(inboxId: inboxId) == .allowed
	}

	public func isInboxDenied(inboxId: String) async -> Bool {
		return await consentList.inboxIdState(inboxId: inboxId) == .denied
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

	public func allowGroups(groupIds: [String]) async throws {
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

	public func denyGroups(groupIds: [String]) async throws {
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
	
	public func allowInboxes(inboxIds: [String]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for inboxId in inboxIds {
				group.addTask {
					return await self.consentList.allowInboxId(inboxId: inboxId)
				}
			}

			for try await entry in group {
				entries.append(entry)
			}
		}
		try await consentList.publish(entries: entries)
	}

	public func denyInboxes(inboxIds: [String]) async throws {
		var entries: [ConsentListEntry] = []

		try await withThrowingTaskGroup(of: ConsentListEntry.self) { group in
			for inboxId in inboxIds {
				group.addTask {
					return await self.consentList.denyInboxId(inboxId: inboxId)
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
