import CryptoKit
import ExampleCore
import Foundation
import Observation

@MainActor @Observable final class ProfilePreferences: ConversationPreferences, PushPreferences {
	private let directory: URL
	private let key: SymmetricKey
	private var values: FeaturePreferences
	private var notificationMutes: [String: Bool] = [:]
	private var postedPushes = PushReceiptWindow()
	private var pushEnabled = false
	private var readMarkers: [String: Int64] = [:]
	var dismissedRequests: Set<String> {
		values.dismissedRequests
	}

	var skippedHandle: Bool {
		values.skippedHandle
	}

	var claimTimes: [String: Date] {
		values.claimTimes
	}

	init(paths: ProfilePaths, secret: Data) throws {
		directory = paths.root.appendingPathComponent("prefs", isDirectory: true); key = SymmetricKey(data: secret)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		let file = directory.appendingPathComponent("feature-preferences")
		if FileManager.default.fileExists(atPath: file.path) {
			let encrypted = try Data(contentsOf: file)
			let plaintext = try AES.GCM.open(AES.GCM.SealedBox(combined: encrypted), using: key)
			values = try JSONDecoder().decode(FeaturePreferences.self, from: plaintext)
		} else {
			values = FeaturePreferences()
		}
		notificationMutes = try read([String: Bool].self, named: "notifications") ?? [:]
		pushEnabled = try read(Bool.self, named: "notifications-enabled") ?? false
		if let receipts = try read(PushReceiptWindow.self, named: "notification-receipts") {
			postedPushes = receipts
		} else if let legacy = try read([String].self, named: "notifications-posted") {
			for tag in legacy {
				postedPushes.record(tag)
			}
			try write(postedPushes, named: "notification-receipts")
			try remove(named: "notifications-posted")
		}
		readMarkers = try read([String: Int64].self, named: "read-markers") ?? [:]
	}

	var serverLabel: String? {
		values.serverLabel
	}

	func setServerLabel(_ label: String?) throws {
		var next = values; next.serverLabel = label; try save(next)
	}

	var notificationsEnabled: Bool {
		pushEnabled
	}

	func setNotificationsEnabled(_ enabled: Bool) throws {
		if enabled {
			try write(true, named: "notifications-enabled")
		} else {
			// Absence already restores as Off; deletion needs no replacement-file allocation.
			do {
				try FileManager.default.removeItem(at: directory.appendingPathComponent("notifications-enabled"))
			} catch let error as CocoaError where error.code == .fileNoSuchFile {
				// Already absent is already durably Off. Other removal errors propagate.
			}
		}
		pushEnabled = enabled
	}

	func hasPosted(_ tag: String) -> Bool {
		postedPushes.contains(tag)
	}

	func recordPosted(_ tag: String) throws {
		var next = postedPushes
		next.record(tag)
		try write(next, named: "notification-receipts")
		postedPushes = next
	}

	func alias(for id: String) -> String? {
		values.aliases[id]
	}

	func muted(for logicalKey: String) -> Bool {
		notificationMutes[logicalKey] ?? false
	}

	func marker(for logicalKey: String) -> Int64 {
		readMarkers[logicalKey] ?? 0
	}

	func dismissRequest(_ id: String) throws {
		var next = values; next.dismissedRequests.insert(id); try save(next)
	}

	func skipHandle() throws {
		var next = values; next.skippedHandle = true; try save(next)
	}

	func recordClaim(_ handle: String, at date: Date) throws {
		var next = values; next.claimTimes[handle] = date; try save(next)
	}

	func setAlias(_ alias: String?, for id: String) throws {
		var next = values; next.aliases[id] = alias; try save(next)
	}

	func setMute(_ value: Bool, for key: String) throws {
		var next = notificationMutes; next[key] = value; try write(
			next,
			named: "notifications",
		); notificationMutes = next
	}

	func setMarker(_ value: Int64, for key: String) throws {
		var next = readMarkers; next[key] = value; try write(
			next,
			named: "read-markers",
		); readMarkers = next
	}

	func remove(named name: String) throws {
		try validate(name)
		let path = directory.appendingPathComponent(name)
		if FileManager.default.fileExists(atPath: path.path) {
			try FileManager.default.removeItem(at: path)
		}
	}

	func names(prefix: String) throws -> [String] {
		try FileManager.default.contentsOfDirectory(atPath: directory.path)
			.filter { $0.hasPrefix(prefix) }
	}

	private func validate(_ name: String) throws {
		guard !name.isEmpty, !name.contains("/"), name != ".",
		      name != ".." else { throw CocoaError(.fileWriteInvalidFileName) }
	}

	func read<T: Decodable>(_ type: T.Type, named name: String) throws -> T? {
		try validate(name)
		let url = directory.appendingPathComponent(name)
		guard FileManager.default.fileExists(atPath: url.path) else { return nil }
		let data = try AES.GCM.open(AES.GCM.SealedBox(combined: Data(contentsOf: url)), using: key)
		return try JSONDecoder().decode(type, from: data)
	}

	func write(_ value: some Encodable, named name: String) throws {
		try validate(name)
		let data = try JSONEncoder().encode(value)
		guard let sealed = try AES.GCM.seal(data, using: key).combined else { throw CocoaError(.fileWriteUnknown) }
		try sealed.write(to: directory.appendingPathComponent(name), options: .atomic)
	}

	private func save(_ next: FeaturePreferences) throws {
		try write(next, named: "feature-preferences"); values = next
	}
}

private struct FeaturePreferences: Codable {
	var serverLabel: String?
	var aliases: [String: String] = [:]
	var dismissedRequests: Set<String> = []
	var skippedHandle = false
	var claimTimes: [String: Date] = [:]
}
