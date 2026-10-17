import ExampleCore
import Foundation

@MainActor final class ProfileFiles: ProfileFileStore {
	private let root: URL
	private let manager = FileManager.default
	private let defaults: UserDefaults
	private let indexKey = "messenger.session-index"
	init(root: URL? = nil, defaults: UserDefaults = .standard) throws {
		self.defaults = defaults
		self.root = try root ?? FileManager.default.url(
			for: .applicationSupportDirectory,
			in: .userDomainMask,
			appropriateFor: nil,
			create: true,
		).appendingPathComponent("messenger", isDirectory: true); try prepare(self.root)
	}

	private func prepare(_ url: URL) throws {
		try manager.createDirectory(
			at: url,
			withIntermediateDirectories: true,
			attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
		); var mutable = url; var values = URLResourceValues(); values.isExcludedFromBackup = true; try mutable
			.setResourceValues(values)
	}

	func loadIndex() throws -> SessionIndex {
		guard let stored = defaults.object(forKey: indexKey) else { return SessionIndex() }
		guard let data = stored as? Data else { throw CocoaError(.fileReadCorruptFile) }
		return try JSONDecoder().decode(SessionIndex.self, from: data)
	}

	func saveIndex(_ index: SessionIndex) throws {
		try defaults.set(JSONEncoder().encode(index), forKey: indexKey)
	}

	func paths(_ profile: UUID) throws -> ProfilePaths {
		let paths = ProfilePaths(root: profileRoot(profile)); try prepare(paths.root); try prepare(paths
			.attachments); try prepare(paths.temp); try prepare(paths.preferences); return paths
	}

	private func profileRoot(_ profile: UUID) -> URL {
		root.appendingPathComponent("profiles", isDirectory: true).appendingPathComponent(
			profile.uuidString,
			isDirectory: true,
		)
	}

	func databaseExists(_ profile: UUID) throws -> Bool {
		let directory = profileRoot(profile)
		let database = ProfilePaths(root: directory).database
		for parent in [directory, directory.appendingPathComponent("sdk", isDirectory: true)] {
			guard manager.fileExists(atPath: parent.path) else { return false }
			let values = try parent.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])
			guard values.isSymbolicLink != true, values.isDirectory == true else { return false }
		}
		guard manager.fileExists(atPath: database.path) else { return false }
		let values = try database.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey])
		return values.isSymbolicLink != true && values.isRegularFile == true
	}

	func removeDatabase(_ profile: UUID) throws {
		let directory = profileRoot(profile); for name in [
			"sdk/messages.db3",
			"sdk/messages.db3-wal",
			"sdk/messages.db3-shm",
		] {
			try remove(directory.appendingPathComponent(name), inside: directory)
		}
	}

	func removeExports(_ profile: UUID) throws {
		let directory = profileRoot(profile); try remove(directory.appendingPathComponent("exports"), inside: directory)
	}

	func removeProfile(_ profile: UUID) throws {
		let directory = profileRoot(profile); guard manager.fileExists(atPath: directory.path)
		else { return }; let values = try directory.resourceValues(forKeys: [.isSymbolicLinkKey]); if values
			.isSymbolicLink == true
		{
			try manager.removeItem(at: directory); return
		}; for child in try manager
			.contentsOfDirectory(
				at: directory,
				includingPropertiesForKeys: nil,
			)
		{
			try remove(child, inside: directory)
		}; try manager.removeItem(at: directory)
	}

	#if DEBUG
		func resetAllForTesting() throws {
			let profiles = root.appendingPathComponent("profiles", isDirectory: true)
			if manager.fileExists(atPath: profiles.path) {
				let values = try profiles.resourceValues(forKeys: [.isSymbolicLinkKey])
				if values.isSymbolicLink == true {
					try manager.removeItem(at: profiles)
				} else {
					for item in try manager
						.contentsOfDirectory(at: profiles, includingPropertiesForKeys: nil)
					{
						try ProfileCleanup.remove(
							item,
							inside: profiles,
						)
					}
				}
			}
			defaults.removeObject(forKey: indexKey)
		}
	#endif

	private func remove(_ url: URL, inside directory: URL) throws {
		try ProfileCleanup.remove(url, inside: directory)
	}
}
