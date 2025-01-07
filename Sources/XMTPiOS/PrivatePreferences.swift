import Foundation
import LibXMTP

public enum ConsentState: String, Codable {
	case allowed, denied, unknown
}
public enum EntryType: String, Codable {
	case address, conversation_id, inbox_id
}
public enum PreferenceType: String, Codable {
	case hmac_keys
}

public struct ConsentRecord: Codable, Hashable {
	public init(value: String, entryType: EntryType, consentType: ConsentState)
	{
		self.value = value
		self.entryType = entryType
		self.consentType = consentType
	}

	static func address(_ address: String, type: ConsentState = .unknown)
		-> ConsentRecord
	{
		ConsentRecord(value: address, entryType: .address, consentType: type)
	}

	static func conversationId(
		conversationId: String, type: ConsentState = ConsentState.unknown
	) -> ConsentRecord {
		ConsentRecord(
			value: conversationId, entryType: .conversation_id,
			consentType: type)
	}

	static func inboxId(_ inboxId: String, type: ConsentState = .unknown)
		-> ConsentRecord
	{
		ConsentRecord(
			value: inboxId, entryType: .inbox_id, consentType: type)
	}

	public var value: String
	public var entryType: EntryType
	public var consentType: ConsentState

	var key: String {
		"\(entryType)-\(value)"
	}
}

/// Provides access to contact bundles.
public actor PrivatePreferences {
	var client: Client
	var ffiClient: FfiXmtpClient

	init(client: Client, ffiClient: FfiXmtpClient) {
		self.client = client
		self.ffiClient = ffiClient
	}

	public func setConsentState(entries: [ConsentRecord]) async throws {
		try await ffiClient.setConsentStates(records: entries.map(\.toFFI))
	}

	public func addressState(address: String) async throws -> ConsentState {
		return try await ffiClient.getConsentState(
			entityType: .address,
			entity: address
		).fromFFI
	}

	public func conversationState(conversationId: String) async throws
		-> ConsentState
	{
		return try await ffiClient.getConsentState(
			entityType: .conversationId,
			entity: conversationId
		).fromFFI
	}

	public func inboxIdState(inboxId: String) async throws -> ConsentState {
		return try await ffiClient.getConsentState(
			entityType: .inboxId,
			entity: inboxId
		).fromFFI
	}

	public func syncConsent() async throws {
		try await ffiClient.sendSyncRequest(kind: .consent)
	}

	public func streamConsent()
		-> AsyncThrowingStream<ConsentRecord, Error>
	{
		AsyncThrowingStream { continuation in
			let ffiStreamActor = FfiStreamActor()

			let consentCallback = ConsentCallback(client: self.client) {
				records in
				guard !Task.isCancelled else {
					continuation.finish()
					Task {
						await ffiStreamActor.endStream()
					}
					return
				}
				for consent in records {
					continuation.yield(consent.fromFfi)
				}
			}

			let task = Task {
				let stream = await ffiClient.conversations().streamConsent(
					callback: consentCallback)
				await ffiStreamActor.setFfiStream(stream)
			}

			continuation.onTermination = { _ in
				task.cancel()
				Task {
					await ffiStreamActor.endStream()
				}
			}
		}
	}

	public func streamPreferenceUpdates()
		-> AsyncThrowingStream<PreferenceType, Error>
	{
		AsyncThrowingStream { continuation in
			let ffiStreamActor = FfiStreamActor()

			let preferenceCallback = PreferenceCallback(client: self.client) {
				records in
				guard !Task.isCancelled else {
					continuation.finish()
					Task {
						await ffiStreamActor.endStream()
					}
					return
				}
				for preference in records {
					if case .hmac(let key) = preference {
						continuation.yield(.hmac_keys)
					}
				}
			}

			let task = Task {
				let stream = await ffiClient.conversations().streamPreferences(
					callback: preferenceCallback)
				await ffiStreamActor.setFfiStream(stream)
			}

			continuation.onTermination = { _ in
				task.cancel()
				Task {
					await ffiStreamActor.endStream()
				}
			}
		}
	}
}

final class ConsentCallback: FfiConsentCallback {
	let client: Client
	let callback: ([FfiConsent]) -> Void

	init(client: Client, _ callback: @escaping ([FfiConsent]) -> Void) {
		self.client = client
		self.callback = callback
	}

	func onConsentUpdate(consent: [FfiConsent]) {
		callback(consent)
	}

	func onError(error: FfiSubscribeError) {
		print("Error ConsentCallback \(error)")
	}
}

final class PreferenceCallback: FfiPreferenceCallback {
	let client: Client
	let callback: ([FfiPreferenceUpdate]) -> Void

	init(client: Client, _ callback: @escaping ([FfiPreferenceUpdate]) -> Void)
	{
		self.client = client
		self.callback = callback
	}

	func onPreferenceUpdate(preference: [FfiPreferenceUpdate]) {
		callback(preference)
	}

	func onError(error: FfiSubscribeError) {
		print("Error ConsentCallback \(error)")
	}
}
