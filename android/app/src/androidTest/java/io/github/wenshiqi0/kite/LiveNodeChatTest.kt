package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.MessageKind
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** One deliberately owned model turn. Never selects a pre-existing provider conversation. */
class LiveNodeChatTest {
    @Test fun aNewOwnedConversationRepliesThroughThePersistentLinuxNode() {
        val args = InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_live_node") == "true")
        val cwd = requireNotNull(args.getString("qa_cwd"))
        require(cwd.startsWith("/home/wenshiqi/.cache/kite-alpha17-qa-20261031/"))
        val test = InstrumentationRegistry.getInstrumentation()
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        test.runOnMainSync { vm = ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        fun waitFor(label: String, condition: () -> Boolean) {
            val until = SystemClock.uptimeMillis() + 90000
            while (!condition() && SystemClock.uptimeMillis() < until) SystemClock.sleep(100)
            assertTrue(label + ": " + vm.engine.state.value?.failure?.details.orEmpty() + " " + vm.engine.state.value?.composer?.details.orEmpty(), condition())
        }
        waitFor("connected") { vm.engine.state.value?.connection == ConnectionStatus.CONNECTED }
        val host = vm.engine.state.value!!.hosts.single { it.id == vm.engine.state.value!!.selectedHost }
        assertEquals("10.0.2.2", host.host); assertEquals(22223, host.port.toInt()); assertEquals(48200, host.sunshinePort.toInt())
        val title = "Kite Node Alpha17 owned validation"
        val resume = args.getString("qa_resume_session")
        val session = if (resume == null) {
            val before = vm.engine.state.value!!.sessions.map { it.id }.toSet()
            test.runOnMainSync { vm.createSession("codex", cwd, title) }
            waitFor("owned session created") { vm.engine.state.value?.sessions?.any { it.id !in before && it.title == title } == true }
            vm.engine.state.value!!.sessions.single { it.id !in before && it.title == title }
        } else {
            waitFor("owned existing session indexed") { vm.engine.state.value?.sessions?.any { it.id == resume && it.title == title && it.cwd == cwd } == true }
            val owned = vm.engine.state.value!!.sessions.single { it.id == resume && it.title == title && it.cwd == cwd }
            test.runOnMainSync { vm.openSession(owned.id) }; owned
        }
        val record = JSONObject().put("sessionId", session.id).put("cwd", cwd).put("owned", true)
        val output = File(test.targetContext.getExternalFilesDir(null), "node-chat-validation.json")
        output.writeText(record.toString(2))
        waitFor("owned composer ready") { vm.engine.state.value?.selectedSession == session.id && !vm.sessionBusy && vm.engine.state.value?.composer?.ready == true }
        val state = vm.engine.state.value!!
        val agent = requireNotNull(state.selectedAgent)
        assertEquals(session.id, state.selectedSession)
        val model = state.composer.models.firstOrNull { it.model.contains("luna") || it.model.contains("mini") } ?: state.composer.models.first()
        val effort = listOf("none", "minimal", "low", "medium").firstOrNull { it in model.efforts } ?: model.defaultEffort
        test.runOnMainSync { vm.selectModel(agent, model.model, effort) }
        waitFor("model selection") { vm.engine.state.value?.composer?.model == model.model && vm.engine.state.value?.composer?.effort == effort }
        val token = "KITE_NODE_ALPHA17_OWNED_OK"
        val oldReplies = vm.engine.timeline.messages.count { it.kind == MessageKind.ASSISTANT && it.text.trim() == token }
        if (resume != null) assertTrue("Restored provider history must contain the previous reply", oldReplies > 0)
        test.runOnMainSync {
            assertEquals(session.id, vm.engine.state.value!!.selectedSession)
            vm.draft = if (resume == null) "Reply with exactly $token. Do not use tools, read files, or run commands."
                else "Reply with exactly the validation token from your previous assistant response. Do not use tools, read files, or run commands."
            vm.sendPrompt(agent)
        }
        record.put("turnSubmitted", true).put("model", model.model).put("effort", effort)
        output.writeText(record.toString(2))
        waitFor("owned model response") { vm.engine.timeline.messages.count { it.kind == MessageKind.ASSISTANT && it.text.trim() == token } > oldReplies }
        record.put("status", "PASS").put("reply", token).put("previousContextRecalled", resume != null)
        output.writeText(record.toString(2))
    }
}
