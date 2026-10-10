package org.xmtp.android.example.messenger.attachments

import org.junit.Assert.*
import org.junit.Test

class DraftDiscardStateTest {
    @Test fun completedDiscardsDoNotKeepQuiescentProfileOrDraftIds() {
        val state = DraftDiscardState()
        repeat(10_000) { index ->
            val id = "profile/draft-$index"
            assertTrue(state.beginDiscard(id))
            state.finishDiscard(id, true)
        }
        assertEquals("Completed quiescent discards do not accumulate", 0, state.retainedForProfile("profile"))
    }

    @Test fun aCompletedDiscardRemainsUntilAllSnapshotReadersFinish() {
        val state = DraftDiscardState()
        val id = "profile/draft"
        state.retainSnapshots(listOf(id))
        state.retainSnapshots(listOf(id))
        assertTrue(state.beginDiscard(id))
        state.finishDiscard(id, true)
        var staleCardAdded = false
        state.admitCard(id) { staleCardAdded = true }
        assertFalse("A stale recovery snapshot cannot add its discarded card", staleCardAdded)
        state.releaseSnapshots(listOf(id))
        assertTrue("Another coordinator still owns the old snapshot", state.isDiscarded(id))
        state.releaseSnapshots(listOf(id))
        assertEquals("The final reader retires the successful discard", 0, state.retainedForProfile("profile"))
    }

    @Test fun aFailedDiscardClearsItsMarkerEvenWhileOtherSnapshotsExist() {
        val state = DraftDiscardState()
        val id = "profile/draft"
        state.retainSnapshots(listOf(id))
        assertTrue(state.beginDiscard(id))
        state.finishDiscard(id, false)
        assertFalse(state.isDiscarded(id))
        assertTrue("A restored owner can retry after discard failure", state.beginDiscard(id))
        state.finishDiscard(id, true)
        assertTrue(state.isDiscarded(id))
        state.releaseSnapshots(listOf(id))
        assertEquals(0, state.retainedForProfile("profile"))
    }
}
