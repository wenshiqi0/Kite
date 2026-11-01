package io.github.wenshiqi0.kite.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class SessionStatusTest {
    @Test fun sessionsKeepExplicitAttentionAndConnectionStates() {
        assertEquals(SessionStatus.Running, sessionStatus("working", true))
        assertEquals(SessionStatus.Question, sessionStatus("question", true))
        assertEquals(SessionStatus.Attention, sessionStatus("blocked", true))
        assertEquals(SessionStatus.Starting, sessionStatus("starting", true))
        assertEquals(SessionStatus.Paused, sessionStatus("paused", true))
        assertEquals(SessionStatus.Completed, sessionStatus("done", true))
        assertEquals(SessionStatus.Idle, sessionStatus("idle", true))
        assertEquals(SessionStatus.Failed, sessionStatus("failed", true))
        assertEquals(SessionStatus.Saved, sessionStatus("saved", true))
        assertEquals(SessionStatus.Unknown, sessionStatus("future-state", true))
        for (state in listOf("working", "question", "blocked", "saved")) {
            assertEquals(SessionStatus.Disconnected, sessionStatus(state, false))
        }
    }
}
