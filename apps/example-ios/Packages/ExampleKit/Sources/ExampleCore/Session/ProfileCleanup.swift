import Foundation

public enum ProfileCleanup {
	public static func remove(_ url: URL, inside root: URL) throws {
		let manager = FileManager.default
		guard url.standardizedFileURL.path.hasPrefix(root.standardizedFileURL.path + "/"),
		      url.deletingLastPathComponent().resolvingSymlinksInPath().path == url.deletingLastPathComponent()
		      .standardizedFileURL.path else { throw SessionError.message("Unsafe profile path") }
		let link: Bool
		do { link = try url.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink == true } catch {
			if !manager.fileExists(atPath: url.path) {
				return
			}; throw error
		}
		if link || manager.fileExists(atPath: url.path) {
			try manager.removeItem(at: url)
		}
	}
}
