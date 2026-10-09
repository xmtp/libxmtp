package org.xmtp.android.example.messenger
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.nio.file.Files

class SessionBoundaryTest {
    @get:Rule val temp = TemporaryFolder()

    @Test fun profileReplacementAndSignOutRejectOldCompletions() {
        val fence = SessionFence()
        val first =
            checkNotNull(
                fence
                    .replace("a"),
            )
        assertTrue(
            fence
                .accepts(first),
        )
        val second =
            checkNotNull(
                fence
                    .replace("b"),
            )
        assertFalse(
            fence
                .accepts(first),
        )
        assertTrue(
            fence
                .accepts(second),
        )
        fence
            .replace(null)
        assertFalse(
            fence
                .accepts(second),
        )
        val delayed =
            fence
                .reserve()
        val latest =
            fence
                .reserve()
        val current =
            checkNotNull(
                fence
                    .bind(
                        "c",
                        latest,
                    ),
            )
        assertNull(
            fence
                .bind(
                    "old",
                    delayed,
                ),
        )
        assertTrue(
            fence
                .accepts(current),
        )
    }

    @Test fun coldResetRemovesDatabaseSidecarsAndOwnedFilesOnly() {
        val selected =
            File(
                temp.root,
                "profiles/a",
            ).apply {
                mkdirs()
            }
        val database =
            File(
                selected,
                "sdk/db",
            ).apply {
                parentFile
                    .mkdirs()
                writeText("db")
            }
        File("${database.path}-wal")
            .writeText("wal")
        File("${database.path}-shm")
            .writeText("shm")
        File(
            selected,
            "attachment",
        ).writeText("plaintext")
        val other =
            File(
                temp.root,
                "profiles/b/keep",
            ).apply {
                parentFile
                    .mkdirs()
                writeText("other")
            }
        val export =
            File(
                temp.root,
                "exports/a",
            ).apply {
                mkdirs()
            }
        File(
            export,
            "opened",
        ).writeText("plaintext")
        val record =
            ResetRecord(
                "a",
                database.path,
                listOf(
                    selected.path,
                    export.path,
                ),
                ResetPhase.STOPPING,
            )
        val cleanup =
            ResetCleanup(
                temp.root,
            )
        cleanup
            .removeDatabase(
                record,
                true,
            )
        assertFalse(
            database
                .exists(),
        )
        assertFalse(
            File("${database.path}-wal")
                .exists(),
        )
        // Repeat covers death after database removal and before the phase write.
        cleanup
            .removeDatabase(
                record,
                true,
            )
        cleanup
            .removeFiles(record)
        assertFalse(
            selected
                .exists(),
        )
        assertFalse(
            export
                .exists(),
        )
        assertEquals(
            "other",
            other
                .readText(),
        )
    }

    @Test fun openOwnerAndEscapingOrSymlinkPathsStopReset() {
        val directory =
            File(
                temp.root,
                "profile",
            ).apply {
                mkdirs()
            }
        val database =
            File(
                directory,
                "db",
            ).apply {
                writeText("db")
            }
        val record =
            ResetRecord(
                "a",
                database.path,
                listOf(
                    directory.path,
                ),
                ResetPhase.STOPPING,
            )
        try {
            ResetCleanup(
                temp.root,
            ).removeDatabase(
                record,
                false,
            )
            fail("Live owner must block cleanup")
        } catch (
            _: IllegalStateException,
        ) {
        }
        assertTrue(
            database
                .exists(),
        )
        val outside =
            temp
                .newFolder("outside")
        val link =
            File(
                directory,
                "link",
            )
        Files
            .createSymbolicLink(
                link
                    .toPath(),
                outside
                    .toPath(),
            )
        try {
            ResetCleanup(directory)
                .removeFiles(
                    record
                        .copy(
                            ownedFiles =
                                listOf(
                                    link.path,
                                ),
                        ),
                )
            fail("Link must be rejected")
        } catch (
            _: IllegalArgumentException,
        ) {
        }
        assertTrue(
            outside
                .exists(),
        )
        try {
            ResetCleanup(directory)
                .removeDatabase(
                    record
                        .copy(
                            ownedDatabasePath =
                                outside.path,
                        ),
                    true,
                )
            fail("Unowned path must be rejected")
        } catch (
            _: IllegalArgumentException,
        ) {
        }
    }
}
