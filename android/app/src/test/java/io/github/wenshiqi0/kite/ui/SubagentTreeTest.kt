package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.SubagentInfo
import org.junit.Assert.*
import org.junit.Test

class SubagentTreeTest {
    private fun agent(id: String, parent: String, status: String = "working") = SubagentInfo(id, parent, id, "task/$id", status, 0)

    @Test fun parentEdgesDefineTheTreeEvenWhenInputIsOutOfOrder() {
        val rows = subagentRows(listOf(agent("grandchild", "child"), agent("child", "root"), agent("sibling", "root")), emptySet())
        assertEquals(listOf("child", "grandchild", "sibling"), rows.map { it.agent.id })
        assertEquals(listOf(listOf(true), listOf(true, false), listOf(false)), rows.map { it.branches })
        assertTrue(rows[0].hasChildren)
        assertFalse(rows[1].hasChildren)
    }

    @Test fun collapsingOneBranchPreservesItsSiblingAndTheirStates() {
        val rows = subagentRows(listOf(agent("child", "root", "done"), agent("grandchild", "child"), agent("sibling", "root", "failed")), setOf("child"))
        assertEquals(listOf("child", "sibling"), rows.map { it.agent.id })
        assertEquals(SessionStatus.Completed, sessionStatus(rows[0].agent.status, true))
        assertEquals(SessionStatus.Failed, sessionStatus(rows[1].agent.status, true))
        assertEquals(SessionStatus.Disconnected, sessionStatus("working", false))
        assertEquals(SessionStatus.Stopped, sessionStatus("stopped", true))
    }

    @Test fun malformedCyclesAndDuplicateNodesDoNotRepeatOrInventRoots() {
        assertTrue(subagentRows(listOf(agent("a", "b"), agent("b", "a")), emptySet()).isEmpty())
        assertEquals(1, subagentRows(listOf(agent("a", "root"), agent("a", "root")), emptySet()).size)
    }
}
