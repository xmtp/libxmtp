import kotlinx.coroutines.runBlocking
import uniffi.xmtp_sdk.PrepareMigrationArchiveArgs
import uniffi.xmtp_sdk.prepareMigrationArchive
import java.nio.file.Files
import java.nio.file.Path

fun main(args: Array<String>) =
    runBlocking {
        val directory = Files.createTempDirectory("xmtp-migration-kotlin-")
        try {
            val database = directory.resolve("legacy.db3")
            for (suffix in listOf("", "-wal", ".sqlcipher_salt")) {
                Files.copy(Path.of(args[0] + suffix), Path.of(database.toString() + suffix))
            }
            val report =
                prepareMigrationArchive(
                    PrepareMigrationArchiveArgs(
                        databasePath = database.toString(),
                        databaseKey = ByteArray(32) { 0x11 },
                        archiveKey = ByteArray(32) { 7 },
                        outputPath = directory.resolve("history.xmtp").toString(),
                    ),
                )
            check(report.groupCount == 2uL && report.messageCount == 4uL && report.consentCount == 1uL)
            check(Files.exists(Path.of(report.archivePath)))
            println("Kotlin module: real encrypted WAL migration and ULong report counts passed")
        } finally {
            Files.walk(directory).use { paths -> paths.sorted(Comparator.reverseOrder()).forEach(Files::delete) }
        }
    }
