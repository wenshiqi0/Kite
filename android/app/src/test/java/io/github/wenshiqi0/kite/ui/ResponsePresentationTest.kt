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
}
