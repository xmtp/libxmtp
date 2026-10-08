import Foundation
import XCTest
import XmtpSdk

#if os(macOS)
	final class LegacyMigrationTests: XCTestCase {
		// verifies: MIG-001, MIG-002, MIG-003
		func testEncryptedWalMigration() async throws {
			var root = URL(fileURLWithPath: #filePath)
			for _ in 0 ..< 5 {
				root.deleteLastPathComponent()
			}
			let source = root.appendingPathComponent("crates/xmtp_legacy_migration/fixtures/encrypted.db3").path
			let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
			try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
			defer { try? FileManager.default.removeItem(at: directory) }
			let database = directory.appendingPathComponent("legacy.db3").path
			var before: [String: Data] = [:]
			for suffix in ["", "-wal", ".sqlcipher_salt"] {
				try FileManager.default.copyItem(atPath: source + suffix, toPath: database + suffix)
				before[suffix] = try Data(contentsOf: URL(fileURLWithPath: database + suffix))
			}
			let output = directory.appendingPathComponent("history.xmtp").path
			let report = try await prepareMigrationArchive(args: PrepareMigrationArchiveArgs(
				databasePath: database, databaseKey: Data(repeating: 0x11, count: 32),
				archiveKey: Data(repeating: 7, count: 32),
				outputPath: output,
			))
			XCTAssertEqual(report.groupCount, 2)
			XCTAssertEqual(report.messageCount, 4)
			XCTAssertEqual(report.consentCount, 1)
			XCTAssertEqual(report.archivePath, output)
			XCTAssertTrue(FileManager.default.fileExists(atPath: report.archivePath))
			for (suffix, bytes) in before {
				XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: database + suffix)), bytes)
			}
		}
	}
#endif
