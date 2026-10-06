import Foundation
import XmtpMigration

@main
struct MigrationConformance {
    static func main() async throws {
        let source = CommandLine.arguments[1]
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("legacy.db3").path
        for suffix in ["", "-wal", ".sqlcipher_salt"] {
            try FileManager.default.copyItem(atPath: source + suffix, toPath: database + suffix)
        }
        let report = try await prepareMigrationArchive(args: PrepareMigrationArchiveArgs(
            databasePath: database, databaseKey: Data(repeating: 0x11, count: 32),
            archiveKey: Data(repeating: 7, count: 32),
            outputPath: directory.appendingPathComponent("history.xmtp").path
        ))
        precondition(report.groupCount == 2 && report.messageCount == 4 && report.consentCount == 1)
        precondition(FileManager.default.fileExists(atPath: report.archivePath))
        print("Swift package: real encrypted WAL migration and UInt64 report counts passed")
    }
}
