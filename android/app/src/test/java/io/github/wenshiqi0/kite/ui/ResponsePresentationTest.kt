package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ResponsePhase
import org.junit.Assert.*
import org.junit.Test

class ResponsePresentationTest {
    @Test fun pendingQuestionAlwaysHasAttentionPriorityOverProviderWork() {
        for (phase in listOf(ResponsePhase.THINKING,ResponsePhase.TOOL,ResponsePhase.RESPONDING,ResponsePhase.IDLE,ResponsePhase.WAITING)) {
            assertEquals(ResponsePhase.WAITING,responsePresentationPhase(phase,false,true,false))
        }
        assertEquals(ResponsePhase.WAITING,responsePresentationPhase(ResponsePhase.THINKING,false,false,true))
    }
    @Test fun answeringOrResolvingThePromptRestoresTheActualWorkPhase() {
        assertEquals(ResponsePhase.THINKING,responsePresentationPhase(ResponsePhase.THINKING,false,false,false))
        assertEquals(ResponsePhase.RESPONDING,responsePresentationPhase(ResponsePhase.RESPONDING,false,false,false))
        assertEquals(ResponsePhase.SENDING,responsePresentationPhase(ResponsePhase.IDLE,true,false,false))
        assertEquals(ResponsePhase.IDLE,responsePresentationPhase(ResponsePhase.IDLE,false,false,false))
    }
    @Test fun terminalFailureShowsTheActualReasonAndHidesStaleErrorsDuringWork() {
        val error="serverOverloaded: Selected model is at capacity. Please try a different model."
        assertEquals(error,responseFailureDetails(ResponsePhase.FAILED,error))
        assertNull(responseFailureDetails(ResponsePhase.THINKING,error))
        assertNull(responseFailureDetails(ResponsePhase.IDLE,error))
        assertNull(responseFailureDetails(ResponsePhase.FAILED,"  "))
    }
    @Test fun modelCapacityKeepsItsSpecificPresentation() {
        assertTrue(responseIsModelCapacity("serverOverloaded: unavailable"))
        assertTrue(responseIsModelCapacity("server_overloaded: unavailable"))
        assertTrue(responseIsModelCapacity("Selected model is at capacity. Please try a different model."))
        assertFalse(responseIsModelCapacity("chat connection closed"))
    }
}
