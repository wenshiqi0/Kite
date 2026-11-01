package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.AgentInfo
import org.junit.Assert.*
import org.junit.Test

class SessionInitializationTest {
    private fun agent(launching: Boolean = false) = AgentInfo("owned",null,"codex",null,"working","owned:p1",null,true,launching,!launching)
    @Test fun newSessionCannotUseThePreviousReadySessionToSubmit() {
        val previous = agent()
        val creating = sessionStartupPhase(true,true,previous,true,false)
        assertEquals(SessionStartupPhase.INITIALIZING,creating)
        assertFalse(creating.canSubmit)
        val selecting = sessionStartupPhase(true,true,null,false,false)
        assertEquals(SessionStartupPhase.INITIALIZING,selecting)
        assertFalse(selecting.canSubmit)
    }
    @Test fun launchingAndChatConnectionShareOneWaitingPhaseUntilReady() {
        val launching = sessionStartupPhase(false,true,agent(true),true,false)
        val connecting = sessionStartupPhase(false,true,agent(),false,false)
        assertEquals(SessionStartupPhase.INITIALIZING,launching)
        assertEquals(launching,connecting)
        assertFalse(launching.canSubmit)
        assertFalse(connecting.canSubmit)
        assertTrue(sessionStartupPhase(false,true,agent(),true,false).canSubmit)
    }
    @Test fun failureAndDisconnectionDoNotAppearAsSuccessfulInitialization() {
        val failure = sessionStartupPhase(false,true,agent(),false,true)
        assertEquals(SessionStartupPhase.FAILED,failure)
        assertFalse(failure.canSubmit)
        val disconnected = sessionStartupPhase(false,false,agent(),true,false)
        assertEquals(SessionStartupPhase.INACTIVE,disconnected)
        assertFalse(disconnected.canSubmit)
    }
    @Test fun dshWaitsForItsOwnManagedConnectionAndClaudeCanUseTextWithoutIt() {
        val dsh = agent().copy(agent = "dsh")
        assertFalse(sessionStartupPhase(false,true,dsh,false,false,managedChat=true).canSubmit)
        assertTrue(sessionStartupPhase(false,true,dsh,true,false,managedChat=true).canSubmit)
        val claude = agent().copy(agent = "claude")
        assertTrue(sessionStartupPhase(false,true,claude,false,false,managedChat=false).canSubmit)
        assertFalse(sessionStartupPhase(false,false,dsh,true,false,managedChat=true).canSubmit)
    }
}
