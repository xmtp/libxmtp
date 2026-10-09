package org.xmtp.android.example.messenger

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException

/** Cold cleanup never opens native storage. All names come from saved ownership. */
class ResetCleanup(private val ownedRoot: File) {
    private fun checked(path: String): File {
        val file = File(path).absoluteFile
        val root = ownedRoot.canonicalFile.toPath()
        require(file.toPath().normalize().startsWith(root) && file.toPath().normalize() != root)
        var parent = file
        while (parent.toPath() != root && parent.parentFile != null) {
            require(!Files.isSymbolicLink(parent.toPath())) { "Reset path contains a symbolic link" }
            parent = parent.parentFile!!
        }
        return file
    }
    private fun remove(path: File) {
        try {
            val attrs = Files.readAttributes(path.toPath(), java.nio.file.attribute.BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
            require(!attrs.isSymbolicLink) { "Reset cannot follow a symbolic link" }
            if (attrs.isDirectory) Files.newDirectoryStream(path.toPath()).use { entries -> entries.forEach { remove(it.toFile()) } }
            Files.delete(path.toPath())
        } catch (_: NoSuchFileException) { /* A completed prior phase can leave an absent owned path. */ }
    }
    fun removeDatabase(record: ResetRecord, noOwner: Boolean) {
        check(noOwner) { "A profile owner is still open" }
        val database = checked(record.ownedDatabasePath)
        remove(database)
        remove(checked("${database.path}-wal"))
        remove(checked("${database.path}-shm"))
        remove(checked("${database.path}-journal"))
    }
    fun removeFiles(record: ResetRecord) = record.ownedFiles.forEach { remove(checked(it)) }
}
