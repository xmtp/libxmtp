package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Keeps metadata for the current file view. This class does not own SDK files. */
internal class VerifiedAttachmentCache<T> {
    private val entries = LinkedHashMap<String, T>()
    private var generation = 0L
    private val retainedState = MutableStateFlow<Set<String>>(emptySet())
    val retained = retainedState.asStateFlow()

    @Synchronized fun generation() = generation

    @Synchronized fun admit(
        generation: Long,
        id: String,
        value: T,
        accepts: () -> Boolean,
    ): Boolean {
        if (generation != this.generation || !accepts()) return false
        entries.remove(id)
        entries[id] = value
        while (entries.size > MAX_RECORDS) entries.remove(entries.keys.first())
        retainedState.value = entries.keys.toSet()
        return true
    }

    @Synchronized operator fun get(id: String): T? {
        val value = entries.remove(id) ?: return null
        entries[id] = value
        return value
    }

    @Synchronized fun clear() {
        generation += 1
        entries.clear()
        retainedState.value = emptySet()
    }

    companion object {
        const val MAX_RECORDS = 16
    }
}
