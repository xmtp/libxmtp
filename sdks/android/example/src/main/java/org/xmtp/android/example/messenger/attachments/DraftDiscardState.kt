package org.xmtp.android.example.messenger.attachments

/** A completed discard is retained only while a snapshot can still refer to it. */
internal class DraftDiscardState {
    private val readers = mutableMapOf<String, Int>()
    private val markers = mutableMapOf<String, Boolean>()

    @Synchronized fun retainSnapshots(ids: List<String>) {
        ids.forEach { readers[it] = (readers[it] ?: 0) + 1 }
    }

    @Synchronized fun releaseSnapshots(ids: List<String>) {
        ids.forEach { id ->
            val count = checkNotNull(readers[id])
            if (count == 1) readers.remove(id) else readers[id] = count - 1
            retire(id)
        }
    }

    @Synchronized fun beginDiscard(id: String): Boolean {
        if (id in markers) return false
        markers[id] = false
        return true
    }

    @Synchronized fun finishDiscard(
        id: String,
        complete: Boolean,
    ) {
        if (complete) {
            markers[id] = true
            retire(id)
        } else {
            markers.remove(id)
        }
    }

    @Synchronized fun isDiscarded(id: String) = id in markers

    @Synchronized fun admitCard(
        id: String,
        change: () -> Unit,
    ) {
        if (id !in markers) change()
    }

    @Synchronized fun retainedForProfile(profile: String) = markers.keys.count { it.startsWith("$profile/") }

    private fun retire(id: String) {
        if (markers[id] == true && (readers[id] ?: 0) == 0) markers.remove(id)
    }
}
