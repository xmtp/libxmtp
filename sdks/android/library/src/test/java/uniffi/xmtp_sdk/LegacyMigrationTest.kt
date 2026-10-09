package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path

class LegacyMigrationTest {
    // verifies: MIG-001, MIG-002, MIG-003
    @Test
    fun encryptedWalMigration() =
        runBlocking {
            val fixture = Path.of(requireNotNull(System.getProperty("xmtp.legacy.fixtures")), "encrypted.db3")
            val directory = Files.createTempDirectory("xmtp-migration-kotlin-")
            try {
                val database = directory.resolve("legacy.db3")
                val before = mutableMapOf<String, ByteArray>()
                for (suffix in listOf("", "-wal", ".sqlcipher_salt")) {
                    val destination = Path.of(database.toString() + suffix)
                    Files.copy(Path.of(fixture.toString() + suffix), destination)
                    before[suffix] = Files.readAllBytes(destination)
                }
                val output = directory.resolve("history.xmtp").toString()
                val report =
                    prepareMigrationArchive(
                        PrepareMigrationArchiveArgs(
                            databasePath = database.toString(),
                            databaseKey = ByteArray(32) { 0x11 },
                            archiveKey = ByteArray(32) { 7 },
                            outputPath = output,
                        ),
                    )
                assertEquals(2uL, report.groupCount)
                assertEquals(4uL, report.messageCount)
                assertEquals(1uL, report.consentCount)
                assertEquals(output, report.archivePath)
                assertTrue(Files.exists(Path.of(report.archivePath)))
                for ((suffix, bytes) in before) {
                    assertArrayEquals(bytes, Files.readAllBytes(Path.of(database.toString() + suffix)))
                }
            } finally {
                Files.walk(directory).use { paths -> paths.sorted(Comparator.reverseOrder()).forEach(Files::delete) }
            }
        }
}
