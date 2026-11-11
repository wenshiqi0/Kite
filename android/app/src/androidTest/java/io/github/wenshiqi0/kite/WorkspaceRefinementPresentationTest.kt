package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.*
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Synthetic display data only; no user conversation is submitted or persisted. */
class WorkspaceRefinementPresentationTest {
    @Test fun toolMediaUsesFullWidthAndImagesFormCompactAlbums() {
        val context = test.targetContext
        val conversation = "owned-chat-album-layout"
        fun sha(bytes: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        val directory = File(context.filesDir, "message-attachments/${sha(conversation.toByteArray())}").apply { mkdirs() }
        val orange = android.graphics.Color.rgb(220, 130, 40)
        fun image(id: String, name: String, width: Int, height: Int, color: Int): AttachmentInfo {
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
            if (height > width * 2) android.graphics.Canvas(bitmap).drawRect(0f, height * .8f, width.toFloat(), height.toFloat(), android.graphics.Paint().apply { this.color = orange })
            val bytes = java.io.ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
            bitmap.recycle()
            File(directory, id).writeBytes(bytes)
            File(directory, "$id.sha256").writeText(sha(bytes))
            return AttachmentInfo(id, name, "image/png", bytes.size.toULong(), false)
        }
        val long = image("d".repeat(32), "owned-long.png", 360, 1200, android.graphics.Color.rgb(45, 110, 190))
        val photos = listOf(
            image("e".repeat(32), "owned-red.png", 400, 400, android.graphics.Color.rgb(180, 70, 70)),
            image("f".repeat(32), "owned-green.png", 400, 400, android.graphics.Color.rgb(50, 150, 100)),
            image("9".repeat(32), "owned-blue.png", 400, 400, android.graphics.Color.rgb(50, 100, 180)),
        )
        fun tool(id: String) = ItemInfo(id, ItemRole.TOOL, listOf(BlockInfo.Tool(ToolInfo("Bash", "Owned tool result", "exec_command", "{}", "ok", true, false, false))), false, null)
        val pure = tool("owned-pure")
        val mixed = tool("owned-mixed").let { it.copy(blocks = it.blocks + BlockInfo.Attachment(long)) }
        val album = ItemInfo("owned-album", ItemRole.ASSISTANT,
            listOf(BlockInfo.Attachment(photos[0]), BlockInfo.Text("\n\n"), BlockInfo.Attachment(photos[1]), BlockInfo.Text("\n"), BlockInfo.Attachment(photos[2])), false, null)
        try {
            overlay({
                val activity = androidx.compose.ui.platform.LocalContext.current as MainActivity
                val vm = androidx.compose.runtime.remember(activity) { ViewModelProvider(activity)[HostCheckViewModel::class.java] }
                Column(Modifier.fillMaxWidth().safeDrawingPadding().padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ChatEntryContent(ChatEntry.Work(listOf(pure))) {}
                    ChatEntryContent(ChatEntry.Message(mixed)) { files -> MessageAttachments(vm, files, {}, {}, conversation) }
                    ChatEntryContent(ChatEntry.Message(album)) { files -> MessageAttachments(vm, files, {}, {}, conversation) }
                }
            }) {
                waitFor("Long thumbnail and album load") { (listOf(long) + photos).all { find(description = it.name) != null } }
                val longBounds = bounds(long.name)
                val cards = mutableListOf<Rect>()
                val label = context.getString(R.string.expand_tool_group, 1)
                fun collect(node: AccessibilityNodeInfo?) {
                    node ?: return
                    if (node.contentDescription?.toString() == label) cards.add(Rect().also { node.getBoundsInScreen(it) })
                    for (index in 0 until node.childCount) collect(node.getChild(index))
                }
                collect(test.uiAutomation.rootInActiveWindow)
                assertEquals(2, cards.size)
                assertEquals(cards[0].left, cards[1].left)
                assertEquals(cards[0].width(), cards[1].width())
                assertEquals(cards[1].left, longBounds.left)
                assertEquals(cards[1].width(), longBounds.width())
                assertTrue(longBounds.height() <= 241 * context.resources.displayMetrics.density)
                val tiles = photos.map { bounds(it.name) }
                assertEquals(tiles[0].top, tiles[1].top)
                assertEquals(tiles[0].top, tiles[2].top)
                assertTrue(kotlin.math.abs(tiles[0].width() - tiles[1].width()) <= 1)
                assertTrue(kotlin.math.abs(tiles[0].width() - tiles[0].height()) <= 1)
                (listOf(long) + photos).forEach { assertNull(find(text = it.name)) }
                screenshot("compact-tool-image-and-album.png")
                assertTrue(actionNode(requireNotNull(find(description = long.name)), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Full image preview opens") { find(text = context.getString(R.string.attachment_save)) != null }
                waitFor("The original image bottom is visible in full preview") {
                    test.uiAutomation.takeScreenshot().let { capture ->
                        val pixels = IntArray(capture.width * capture.height)
                        capture.getPixels(pixels, 0, capture.width, 0, 0, capture.width, capture.height)
                        val visible = pixels.count { it == orange } > 1000
                        capture.recycle()
                        visible
                    }
                }
                screenshot("long-image-full-preview.png")
            }
        } finally { directory.deleteRecursively() }
    }
    @Test fun nativeAudioVideoAndTextAttachmentsOpenWithoutAutoplay() {
        // MediaController is a separate native window above the Compose dialog.
        val service = test.uiAutomation.serviceInfo
        val originalFlags = service.flags
        service.flags = service.flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
        test.uiAutomation.serviceInfo = service
        val context = test.targetContext
        val conversation = "owned-multimodal-items/playback"
        fun sha(bytes: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        val directory = File(context.filesDir, "message-attachments/${sha(conversation.toByteArray())}").apply { mkdirs() }
        val wave = java.nio.ByteBuffer.allocate(44 + 32000).order(java.nio.ByteOrder.LITTLE_ENDIAN).apply {
            put("RIFF".toByteArray()); putInt(32036); put("WAVEfmt ".toByteArray()); putInt(16); putShort(1); putShort(1)
            putInt(8000); putInt(16000); putShort(2); putShort(16); put("data".toByteArray()); putInt(32000)
            for (index in 0 until 16000) putShort((kotlin.math.sin(index * 2.0 * Math.PI * 220.0 / 8000) * 1500).toInt().toShort())
        }.array()
        val video = test.context.assets.open("multimodal-owned.mp4").use { it.readBytes() }
        val sources = listOf(Triple("owned-audio.wav", "audio/wav", wave), Triple("owned-video.mp4", "video/mp4", video), Triple("owned-notes.md", "text/markdown", "# Owned Markdown attachment\n\n**中文内容**\n\n- First item\n- 第二行".toByteArray()))
        val files = sources.mapIndexed { index, (name, mime, bytes) ->
            val id = (index + 1).toString().repeat(32)
            File(directory, id).writeBytes(bytes); File(directory, "$id.sha256").writeText(sha(bytes))
            AttachmentInfo(id, name, mime, bytes.size.toULong(), false)
        }
        try {
            overlay({
                val activity = androidx.compose.ui.platform.LocalContext.current as MainActivity
                val vm = androidx.compose.runtime.remember(activity) { ViewModelProvider(activity)[HostCheckViewModel::class.java] }
                Column(Modifier.fillMaxWidth().safeDrawingPadding().padding(16.dp)) { MessageAttachments(vm, files, {}, {}, conversation) }
            }) {
                fun click(label: String) {
                    waitFor("Control appears: $label") { find(text = label) != null }
                    assertTrue(actionNode(requireNotNull(find(text = label)), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
                }
                click("owned-audio.wav")
                waitFor("Audio prepares") { find(text = "0:00 / 0:02") != null && find(text = context.getString(R.string.attachment_audio_play)) != null }
                click(context.getString(R.string.attachment_audio_play))
                waitFor("Audio plays after a tap") { find(text = context.getString(R.string.attachment_audio_pause)) != null }
                click(context.getString(R.string.attachment_audio_pause))
                screenshot("unified-native-audio.png")
                click(context.getString(R.string.dismiss))
                waitFor("Audio closes") { find(text = context.getString(R.string.attachment_save)) == null }
                click("owned-video.mp4")
                waitFor("Video has visible controls") {
                    val button = find(text = context.getString(R.string.attachment_video_play))
                    button != null && runCatching { actionNode(button, AccessibilityNodeInfo.ACTION_CLICK).isEnabled }.getOrDefault(false) && find(text = "0:00 / 0:04") != null
                }
                assertNull(find(text = context.getString(R.string.attachment_video_pause)))
                click(context.getString(R.string.attachment_video_play))
                waitFor("Video plays through the visible button") { find(text = context.getString(R.string.attachment_video_pause)) != null }
                waitFor("Video playback advances") { find(text = "0:01 / 0:04") != null || find(text = "0:02 / 0:04") != null }
                screenshot("unified-native-video.png")
                click(context.getString(R.string.attachment_video_pause))
                click(context.getString(R.string.dismiss))
                waitFor("Video closes") { find(text = context.getString(R.string.attachment_save)) == null }
                click("owned-notes.md")
                waitFor("Text preview opens") { find(text = "Owned Markdown attachment") != null }
                screenshot("unified-native-file.png")
            }
        } finally {
            directory.deleteRecursively()
            service.flags = originalFlags
            test.uiAutomation.serviceInfo = service
        }
    }
    @Test fun humanAndAgentShareContentRendererInsideDistinctBubbles() {
        val context = test.targetContext
        val conversation = "owned-multimodal-items/native"
        fun sha(bytes: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        val bitmap = Bitmap.createBitmap(480, 96, Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.rgb(45, 110, 190)) }
        val bytes = java.io.ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        bitmap.recycle()
        val namespace = sha(conversation.toByteArray())
        val uploadedId = "1234567890abcdef1234567890abcdef"
        val generatedId = sha(bytes)
        val uploads = File(context.filesDir, "message-attachments/$namespace").apply { mkdirs() }
        val generated = File(context.filesDir, "generated-images/$namespace").apply { mkdirs() }
        File(uploads, uploadedId).writeBytes(bytes)
        File(uploads, "$uploadedId.sha256").writeText(generatedId)
        File(generated, generatedId).writeBytes(bytes)
        val files = listOf(AttachmentInfo(uploadedId, "human-photo.png", "image/png", bytes.size.toULong(), false),
            AttachmentInfo(generatedId, "agent-photo.png", "image/png", bytes.size.toULong(), true))
        lateinit var nativeRoot: View
        try {
            overlay({
                val activity = androidx.compose.ui.platform.LocalContext.current as MainActivity
                val vm = androidx.compose.runtime.remember(activity) { ViewModelProvider(activity)[HostCheckViewModel::class.java] }
                nativeRoot = activity.window.decorView
                Column(Modifier.fillMaxWidth().safeDrawingPadding().padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    files.forEachIndexed { index, file ->
                        val message = ItemInfo("owned-$index", if (index == 0) ItemRole.USER else ItemRole.ASSISTANT,
                            listOf(BlockInfo.Text("**统一排版** · `相同内容块`"), BlockInfo.Attachment(file)), false, null)
                        ChatEntryContent(ChatEntry.Message(message)) { attachments -> MessageAttachments(vm, attachments, {}, {}, conversation) }
                    }
                }
            }) {
                waitFor("Both native images load") { find(description = "human-photo.png") != null && find(description = "agent-photo.png") != null }
                val human = bounds("human-photo.png"); val agent = bounds("agent-photo.png")
                assertTrue("User bubble stays right of the agent bubble", human.left > agent.left)
                assertTrue("Sender padding remains distinct around the same renderer", human.width() > agent.width())
                test.uiAutomation.takeScreenshot().let { capture ->
                    assertEquals(KiteColors.UserBubble.toArgb(), capture.getPixel(human.left - 8, human.centerY()))
                    assertEquals(KiteColors.Surface.toArgb(), capture.getPixel(agent.left - 8, agent.centerY()))
                    capture.recycle()
                }
                main {
                    val views = mutableListOf<MarkdownTextView>()
                    fun walk(view: View) {
                        if (view is MarkdownTextView && view.isShown) views.add(view)
                        if (view is android.view.ViewGroup) for (index in 0 until view.childCount) walk(view.getChildAt(index))
                    }
                    walk(nativeRoot)
                    assertEquals(2, views.size)
                    assertEquals(views[0].text.toString(), views[1].text.toString())
                    assertEquals(views[0].currentTextColor, views[1].currentTextColor)
                    assertEquals(views[0].textSize, views[1].textSize)
                    assertFalse(views[0].text.contains("**"))
                }
                screenshot("shared-renderer-distinct-bubbles.png")
                files.forEach { file ->
                    val node = requireNotNull(find(description = file.name))
                    assertTrue(actionNode(node, AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
                    waitFor("Shared preview controls") { find(text = context.getString(R.string.attachment_save)) != null && find(text = context.getString(R.string.dismiss)) != null }
                    screenshot("unified-preview-${file.name}.png")
                    assertTrue(actionNode(requireNotNull(find(text = context.getString(R.string.dismiss))), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
                    waitFor("Preview closes") { find(text = context.getString(R.string.attachment_save)) == null }
                }
            }
        } finally { uploads.deleteRecursively(); generated.deleteRecursively() }
    }
    @Test fun questionsCanCloseMidFormKeepDraftsAndLeaveTheWorkspaceComposerEnabled() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("qa_native_history") == "true")
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        main { vm = ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        waitFor("Core initialized") { vm.engine.state.value?.initialized == true }
        val saved = requireNotNull(vm.engine.state.value)
        assumeTrue("Only an empty isolated app may use this fixture", saved.hosts.isEmpty())
        @Suppress("UNCHECKED_CAST")
        val display = vm.engine.javaClass.getDeclaredField("current").apply { isAccessible = true }.get(vm.engine) as kotlinx.coroutines.flow.MutableStateFlow<CoreSnapshot?>
        val agent = AgentInfo("owned-question-agent", null, "codex", "Owned questions", "blocked", "%owned", null, false, false, true)
        val base = saved.copy(connection = ConnectionStatus.CONNECTED, selectedHost = "owned-question-host",
            selectedSession = "owned-question-session", selectedAgent = agent.id, agents = listOf(agent),
            providers = listOf(ProviderInfo("codex", true, false, false, false, false, false, false, false)),
            agentPause = AgentPauseStatus.RUNNING, responsePhase = ResponsePhase.WAITING,
            composer = saved.composer.copy(ready = true, loading = false, details = ""),
            sessions = listOf(SessionInfo("owned-question-session", "codex", "Owned questions", "", null, false, 0, agent.id, "question", true, emptyList(), false)))
        fun clickText(text: String) = assertTrue(actionNode(requireNotNull(find(text = text)), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
        try {
            for (blocking in listOf(false, true)) {
                val question = QuestionInfo("owned-question-$blocking", blocking, listOf(
                    QuestionFieldInfo("colour", "颜色", "Owned colour question", true, false, listOf(QuestionOptionInfo("蓝色", ""))),
                    QuestionFieldInfo("note", "说明", "Owned note question", true, false, emptyList()),
                ))
                main { vm.settingsVisible = false; vm.draft = "继续，先按默认方案处理"; display.value = base.copy(question = question) }
                waitFor("The real workspace displays the question") { find(text = "Owned colour question") != null }
                clickText("蓝色"); clickText("下一个")
                waitFor("Second question opens") { find(text = "Owned note question") != null }
                val answer = actionNode(requireNotNull(find(description = "回答：说明")), AccessibilityNodeInfo.ACTION_SET_TEXT)
                assertTrue(answer.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
                    putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "保留未提交草稿")
                }))
                clickText("稍后回答")
                waitFor("Closing hides the unfinished form") { find(text = "Owned note question") == null }
                assertEquals("蓝色", vm.questionValues(question.id)["colour"])
                assertEquals("保留未提交草稿", vm.questionValues(question.id)["note"])
                val send = actionNode(requireNotNull(find(description = "发送消息")), AccessibilityNodeInfo.ACTION_CLICK)
                assertTrue("A pending question must allow a normal follow-up", send.isEnabled)
                screenshot("question-closed-$blocking.png")
                clickText("回答问题")
                waitFor("Reopening restores the same page and draft") { find(text = "Owned note question") != null && find(text = "保留未提交草稿") != null }
                assertNotNull(find(text = "2 / 2"))
                // The closed form did not submit any answer or mutate the provider snapshot.
                assertEquals(question.id, display.value?.question?.id)
                screenshot("question-reopened-$blocking.png")
                clickText("回答问题")
                waitFor("The answer sheet opens independently") { find(text = "Owned colour question") != null }
                screenshot("question-sheet-$blocking.png")
                clickText("稍后回答")
                waitFor("Closing the sheet returns to the composer") { find(text = "Owned note question") != null && find(description = "发送消息") != null }
                assertEquals("保留未提交草稿", vm.questionValues(question.id)["note"])
                main { display.value = base.copy(question = null) }
                waitFor("Resolved question leaves the workspace") { find(text = "Owned note question") == null }
            }
        } finally { main { display.value = saved; activity.finish() } }
    }

    @Test fun subagentHistorySharesGlobalInstructionsSkillCardsAndMessageBubblesWithMainChat() {
        val global = "# AGENTS.md instructions\n\n<INSTRUCTIONS>\nGlobal instruction fixture body\n</INSTRUCTIONS>"
        val messages = listOf(
            ItemInfo("global", ItemRole.USER, listOf(BlockInfo.Text(global)), false, null),
            ItemInfo("skill", ItemRole.USER, listOf(BlockInfo.SkillContext("report", "/skills/report/SKILL.md", "使用同一套消息渲染。")), false, null),
            ItemInfo("user", ItemRole.USER, listOf(BlockInfo.Text("保留用户的补充要求。")), false, null),
            ItemInfo("reply", ItemRole.ASSISTANT, listOf(BlockInfo.Text("Inspection result is ready")), false, null),
        )
        val child = SubagentInfo("child", "parent", "Worker", "research", "done", 0)
        val inspecting = androidx.compose.runtime.mutableStateOf(false)
        fun checkSharedMessages() {
            waitFor("global instruction notice") { find(text = "已加载全局指令") != null }
            assertNull(find(text = "# AGENTS.md instructions"))
            assertNull(find(text = "Global instruction fixture body"))
            assertNotNull(find(text = "Skill 指令"))
            assertNotNull(find(text = "保留用户的补充要求。"))
            assertNotNull(find(text = "Inspection result is ready"))
            assertNull(find(text = "使用同一套消息渲染。"))
        }
        overlay({
            Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                messages.forEach { ChatEntryContent(ChatEntry.Message(it)) {} }
                androidx.compose.material3.TextButton(onClick = { inspecting.value = true }) {
                    androidx.compose.material3.Text("Inspect shared history fixture")
                }
            }
            if (inspecting.value) AssociatedTaskInspector(child, true, load = {
                SubagentInspectionInfo(messages, false)
            }, dismiss = { inspecting.value = false })
        }) {
            checkSharedMessages()
            screenshot("main-shared-message-rendering.png")
            actionNode(requireNotNull(find(text = "Inspect shared history fixture")), AccessibilityNodeInfo.ACTION_CLICK)
                .performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("child history opens") { find(text = "只读记录") != null }
            checkSharedMessages()
            actionNode(requireNotNull(find(description = "展开 report 的 Skill 指令")), AccessibilityNodeInfo.ACTION_CLICK)
                .performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("child Skill expands") { find(text = "使用同一套消息渲染。") != null }
            assertNotNull(find(text = "来源：/skills/report/SKILL.md"))
            assertNull(find(text = "Global instruction fixture body"))
            screenshot("subagent-shared-message-rendering.png")
        }
    }

    @Test fun completedSubagentOpensReadonlyRecordsAndRefreshDoesNotChangeItsParent() {
        val child = SubagentInfo("child", "parent", "Worker", "research", "done", 0)
        val selected = androidx.compose.runtime.mutableStateOf<SubagentInfo?>(null)
        val reads = java.util.concurrent.atomic.AtomicInteger()
        overlay({
            Column(Modifier.fillMaxWidth().padding(20.dp)) {
                androidx.compose.material3.Text("Parent remains selected")
                AssociatedTasks("inspection-fixture", listOf(child), false, true, inspect = { selected.value = it })
                selected.value?.let { target ->
                    AssociatedTaskInspector(target, true, load = {
                        reads.incrementAndGet()
                        SubagentInspectionInfo(listOf(ItemInfo("reply", ItemRole.ASSISTANT,
                            listOf(BlockInfo.Text("Inspection result is ready")), false, null)), true)
                    }, dismiss = { selected.value = null })
                }
            }
        }) {
            waitFor("folded tree states completion") { find(text = "1 个已完成") != null }
            assertNull(find(text = "Worker"))
            actionNode(requireNotNull(find(text = "关联任务 · 1")), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("worker row is visible") { find(text = "Worker") != null }
            actionNode(requireNotNull(find(text = "Worker")), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("the leaf opens its readonly history") { find(text = "Inspection result is ready") != null }
            assertEquals(1, reads.get())
            assertNotNull(find(text = "只读 · 最近记录"))
            screenshot("subagent-readonly-inspection.png")
            actionNode(requireNotNull(find(text = "刷新记录")), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("refresh reads the same history") { reads.get() == 2 }
            assertEquals("child", selected.value?.id)
            actionNode(requireNotNull(find(description = "关闭")), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK)
            waitFor("closing returns to the parent") { selected.value == null && find(text = "Parent remains selected") != null }
        }
    }

    private val test get()=InstrumentationRegistry.getInstrumentation()
    private fun main(block:()->Unit){var error:Throwable?=null;test.runOnMainSync{try{block()}catch(e:Throwable){error=e}};error?.let{throw it}}
    private fun waitFor(label:String,condition:()->Boolean){val end=SystemClock.uptimeMillis()+15000;while(!condition()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(80);assertTrue(label,condition())}
    private fun find(description:String?=null,text:String?=null):AccessibilityNodeInfo? {
        fun walk(node:AccessibilityNodeInfo?):AccessibilityNodeInfo?{
            node?:return null
            if(node.isVisibleToUser && (description!=null&&node.contentDescription?.toString()==description || text!=null&&node.text?.toString()?.contains(text)==true))return node
            for(i in 0 until node.childCount)walk(node.getChild(i))?.let{return it}
            return null
        }
        if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow) ?: test.uiAutomation.windows.firstNotNullOfOrNull { walk(it.root) }
    }
    private fun bounds(description:String)=Rect().also{requireNotNull(find(description=description)).getBoundsInScreen(it)}
    private fun actionNode(node:AccessibilityNodeInfo,action:Int):AccessibilityNodeInfo {
        var current:AccessibilityNodeInfo?=node
        while(current!=null){if(current.actionList.any{it.id==action})return current;current=current.parent}
        error("No actionable ancestor for ${node.contentDescription}: $action")
    }
    private fun gesture(rect:Rect,dy:Float){
        val t=SystemClock.uptimeMillis();val start=rect.exactCenterY()-dy/2
        fun send(action:Int,y:Float){val event=MotionEvent.obtain(t,SystemClock.uptimeMillis(),action,rect.exactCenterX(),y,0);event.source=InputDevice.SOURCE_TOUCHSCREEN;try{assertTrue(test.uiAutomation.injectInputEvent(event,true))}finally{event.recycle()}}
        send(MotionEvent.ACTION_DOWN,start)
        for(i in 1..10){SystemClock.sleep(24);send(MotionEvent.ACTION_MOVE,start+dy*i/10)}
        // Pause before lifting so this checks scroll distance, not an uncontrolled fling.
        SystemClock.sleep(160);send(MotionEvent.ACTION_UP,start+dy);test.waitForIdleSync();SystemClock.sleep(180)
    }
    private fun screenshot(name:String,crop:Rect?=null){
        val full=test.uiAutomation.takeScreenshot();val image=if(crop==null)full else Bitmap.createBitmap(full,crop.left,crop.top,crop.width(),crop.height())
        File(test.targetContext.getExternalFilesDir(null),name).outputStream().use{image.compress(Bitmap.CompressFormat.PNG,100,it)}
        if(image!==full)image.recycle();full.recycle()
    }
    private fun overlay(content: @Composable () -> Unit,check:()->Unit){
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root=activity.findViewById<FrameLayout>(android.R.id.content);val original=mutableListOf<View>();lateinit var panel:ComposeView
        try{
            main{for(i in 0 until root.childCount){root.getChildAt(i).let{original.add(it);it.visibility=View.INVISIBLE}};panel=ComposeView(activity).apply{setContent{KiteTheme{Box(Modifier.fillMaxSize().background(KiteColors.Background)){content()}}}};root.addView(panel,FrameLayout.LayoutParams(-1,-1))}
            test.waitForIdleSync();SystemClock.sleep(300);check()
        }catch(error:Throwable){
            runCatching { screenshot("workspace-refinement-failure.png") }
            throw error
        }finally{main{panel.disposeComposition();root.removeView(panel);original.forEach{it.visibility=View.VISIBLE};activity.finish()}}
    }
    private fun usage(status:UsageStatus=UsageStatus.READY)=AccountUsageInfo(status,if(status==UsageStatus.UNAVAILABLE)UsageUnavailable.NO_DATA else UsageUnavailable.NONE,false,false,"pro",
        if(status==UsageStatus.UNAVAILABLE)emptyList()else listOf(UsageBucketInfo("codex",null,UsageWindowInfo(73u,300uL,System.currentTimeMillis()/1000+7200),UsageWindowInfo(52u,10080uL,System.currentTimeMillis()/1000+259200),null,false,false)),true,null,System.currentTimeMillis())

    @Test fun streamingRepliesRetainTheirViewAndFollowOnlyWhileReadingTheTail() {
        val body = androidx.compose.runtime.mutableStateOf("STREAM 检查进度\n\n" + (1..32).joinToString("\n") { "第 $it 行：已读取配置，继续检查。" })
        val streaming = androidx.compose.runtime.mutableStateOf(true)
        val following = androidx.compose.runtime.mutableStateOf(true)
        val feedback = androidx.compose.runtime.mutableStateOf(true)
        lateinit var list: LazyListState
        lateinit var scope: CoroutineScope
        lateinit var nativeRoot: View
        fun atTail(): Boolean {
            var result = false
            main {
                val layout = list.layoutInfo
                val last = layout.visibleItemsInfo.lastOrNull()
                result = last?.key == "conversation-tail" && !list.canScrollForward &&
                    last.offset + last.size + layout.afterContentPadding <= layout.viewportEndOffset
            }
            return result
        }
        fun nativeReply(): MarkdownTextView? {
            fun visit(view: View): MarkdownTextView? {
                if (view is MarkdownTextView && view.text.startsWith("STREAM")) return view
                if (view is android.view.ViewGroup) for (i in 0 until view.childCount) visit(view.getChildAt(i))?.let { return it }
                return null
            }
            var result: MarkdownTextView? = null
            main { result = visit(nativeRoot) }
            return result
        }
        overlay({
            nativeRoot = androidx.compose.ui.platform.LocalView.current
            list = rememberLazyListState()
            scope = rememberCoroutineScope()
            ChatTailFollow(list, "owned-streaming-fixture") { following.value }
            Column(Modifier.fillMaxSize().padding(horizontal = 12.dp, vertical = 32.dp)) {
                LazyColumn(Modifier.fillMaxWidth().height(420.dp), state = list,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(40, key = { "history:$it" }) { androidx.compose.material3.Text("历史消息 $it\n保留阅读位置。") }
                    item(key = "streaming-reply") {
                        androidx.compose.material3.Surface(color = KiteColors.Surface, modifier = Modifier.widthIn(max = 340.dp)) {
                            MarkdownMessage(body.value, streaming = streaming.value, modifier = Modifier.padding(horizontal = 15.dp, vertical = 11.dp))
                        }
                    }
                    if (feedback.value) item(key = "response-feedback") { ResponseFeedback(ResponsePhase.RESPONDING, false) }
                    item(key = "conversation-tail") { Spacer(Modifier.height(8.dp)) }
                }
            }
        }) {
            waitFor("Initial long reply reaches the measured tail", ::atTail)
            val original = requireNotNull(nativeReply())
            val rendered = original.text
            repeat(6) { step ->
                main { streaming.value = step % 2 != 0 }
                test.waitForIdleSync(); SystemClock.sleep(60)
                main { assertSame("Phase changes retain selectable text", rendered, original.text) }
                assertSame(original, nativeReply())
            }
            val draws = java.util.concurrent.atomic.AtomicInteger()
            val missing = java.util.concurrent.atomic.AtomicInteger()
            val drawnText = java.util.concurrent.atomic.AtomicReference("")
            val observer = android.view.ViewTreeObserver.OnDrawListener {
                draws.incrementAndGet()
                drawnText.set(original.text.toString())
                if (!original.isShown || original.height == 0) missing.incrementAndGet()
            }
            main { nativeRoot.viewTreeObserver.addOnDrawListener(observer) }
            try {
                repeat(24) { step ->
                    val marker = "新增第 $step 行：回复持续生成，保持底部稳定。"
                    val next = body.value + "\n" + marker
                    main { body.value = next }
                    test.waitForIdleSync()
                    waitFor("Growing reply is drawn at its measured tail") {
                        var measured = false
                        main { measured = original.text.contains(marker) && original.layout?.let { it.height <= original.height } == true }
                        measured && atTail() && drawnText.get().contains(marker)
                    }
                    assertSame("Streaming must not recreate the native reply", original, nativeReply())
                }
                assertTrue("Each streaming update reaches an Android draw", draws.get() >= 24)
                assertEquals("A streaming draw must retain the visible reply", 0, missing.get())
                SystemClock.sleep(160)
                screenshot("streaming-chat-stable-tail.png")
            } finally {
                main { nativeRoot.viewTreeObserver.removeOnDrawListener(observer) }
            }
            main { following.value = false; scope.launch { list.scrollToItem(9, 11) } }
            waitFor("Reader can inspect earlier messages") { list.firstVisibleItemIndex == 9 && list.firstVisibleItemScrollOffset == 11 }
            main { body.value += "\n后台继续生成。" }
            test.waitForIdleSync(); SystemClock.sleep(160)
            assertEquals(9, list.firstVisibleItemIndex)
            assertEquals(11, list.firstVisibleItemScrollOffset)
            main { following.value = true }
            waitFor("Explicit return to latest resumes following", ::atTail)
            main { body.value = "STREAM **完成**"; streaming.value = false; feedback.value = false }
            test.waitForIdleSync()
            waitFor("Completion and reply shrink settle at the tail", ::atTail)
            assertEquals("STREAM 完成", nativeReply()?.text?.toString())
        }
    }

    @Test fun composerStopsActiveWorkAndKeepsTheNormalSendActionWhenIdle() {
        val working = androidx.compose.runtime.mutableStateOf(false)
        val hasInput = androidx.compose.runtime.mutableStateOf(false)
        val ready = androidx.compose.runtime.mutableStateOf(false)
        val interrupting = androidx.compose.runtime.mutableStateOf(false)
        val sends = java.util.concurrent.atomic.AtomicInteger()
        val stops = java.util.concurrent.atomic.AtomicInteger()
        val send = test.targetContext.getString(R.string.send_prompt)
        val stop = test.targetContext.getString(R.string.interrupt_agent)
        fun button(description: String) = actionNode(requireNotNull(find(description=description)), AccessibilityNodeInfo.ACTION_CLICK)
        fun unavailable(description: String): Boolean {
            var node=find(description=description) ?: return false
            while(true) {
                if(!node.isEnabled)return true
                node=node.parent ?: return false
            }
        }
        overlay({
            Box(Modifier.fillMaxSize(),contentAlignment=androidx.compose.ui.Alignment.BottomCenter) {
                Row(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=32.dp),verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    ChatDraftField(if(hasInput.value)"追加指令" else "",{},Modifier.weight(1f),enabled=ready.value)
                    ComposerActionButton(working.value,hasInput.value,ready.value,ready.value,false,interrupting.value,{sends.incrementAndGet()},{stops.incrementAndGet()})
                }
            }
        }) {
            waitFor("Initialization shows a normal disabled send action"){find(description=send)!=null}
            assertTrue(unavailable(send))
            assertNull(find(description=stop))
            screenshot("composer-initializing-send.png")
            main { ready.value=true }
            waitFor("Idle empty input retains send"){find(description=send)!=null}
            assertTrue(unavailable(send))
            screenshot("composer-idle-send.png")
            main { working.value=true }
            waitFor("Active empty input offers interruption"){find(description=stop)!=null}
            assertTrue(button(stop).isEnabled)
            assertTrue(button(stop).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Stop invokes only the interrupt callback"){stops.get()==1}
            assertEquals(0,sends.get())
            screenshot("composer-working-stop.png")
            main { interrupting.value=true }
            waitFor("In-flight interruption is disabled"){unavailable(stop)}
            gesture(bounds(stop),0f)
            assertEquals("A real duplicate tap cannot resubmit interruption",1,stops.get())
            main { interrupting.value=false; hasInput.value=true }
            waitFor("Drafts can still send follow-up instructions during work"){find(description=send)!=null}
            assertTrue(button(send).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Follow-up submits once"){sends.get()==1}
            assertEquals(1,stops.get())
            main { working.value=false; hasInput.value=false }
            waitFor("Completion restores the disabled empty send action"){unavailable(send)}
            assertNull(find(description=stop))
        }
    }

    @Test fun deletionNamesTheSessionAndRequiresAnExplicitChoice() {
        val deletes=java.util.concurrent.atomic.AtomicInteger()
        val dismisses=java.util.concurrent.atomic.AtomicInteger()
        overlay({DeleteSessionDialog("待删除的会话",true,{deletes.incrementAndGet()},{dismisses.incrementAndGet()})}) {
            waitFor("Deletion identifies the session"){find(text="待删除的会话")!=null}
            assertNotNull(find(text="立即停止"))
            assertNotNull(find(text="释放资源"))
            assertEquals(0,deletes.get())
            assertTrue(actionNode(requireNotNull(find(text=test.targetContext.getString(R.string.cancel))),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Cancel only dismisses"){dismisses.get()==1}
            assertEquals(0,deletes.get())
        }
    }

    @Test fun archivedReleaseUsesOneActionAndShowsProgressAndResult() {
        val calls = java.util.concurrent.atomic.AtomicInteger()
        overlay({ArchivedReleaseControls(true,false,null){calls.incrementAndGet()}}) {
            waitFor("Archive release action appears"){find(text="释放后台进程")!=null}
            assertTrue(actionNode(requireNotNull(find(text="释放后台进程")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Release action submitted once"){calls.get()==1}
            screenshot("archived-release-action.png")
        }
        overlay({ArchivedReleaseControls(true,true,null){calls.incrementAndGet()}}) {
            waitFor("Release progress appears"){find(text="正在释放")!=null}
            screenshot("archived-release-progress.png")
        }
        val result=test.targetContext.getString(R.string.archived_release_result,3,1,0)
        overlay({ArchivedReleaseControls(false,false,result){calls.incrementAndGet()}}) {
            waitFor("Release result preserves session history"){find(text="已释放 3 个会话后台")!=null}
            assertNotNull(find(text="会话记录已保留"))
            screenshot("archived-release-result.png")
        }
    }

    @Test fun verifiedDshIsSelectableAndUnavailableProvidersAreAbsent() {
        val selected=java.util.concurrent.atomic.AtomicReference("")
        val dsh=ProviderInfo("dsh",true,true,true,false,false,false,false,true)
        val ply=dsh.copy(id="ply")
        overlay({HarnessChoices(listOf(dsh,ply),"dsh",selected::set)}) {
            waitFor("Verified DSH appears"){find(text="DSH")!=null}
            assertNull(find(text="Codex"));assertNull(find(text="Claude"));assertNull(find(text="Ply"))
            assertTrue(actionNode(requireNotNull(find(text="DSH")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Dropdown contains verified Ply"){find(text="Ply")!=null}
            screenshot("verified-harness-dropdown.png")
            assertTrue(actionNode(requireNotNull(find(text="Ply")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Stable provider ID submitted"){selected.get()=="ply"}
            screenshot("dsh-verified-provider.png")
        }
        overlay({HarnessChoices(emptyList(),"dsh",selected::set)}) {
            waitFor("Empty host catalogue"){find(text="当前主机没有可用的智能体")!=null}
            assertNull(find(text="DSH"));assertNull(find(text="Ply"))
            screenshot("dsh-unavailable-hidden.png")
        }
    }

    @Test fun harnessModelsAndEffortsUseNativeChoices() {
        val nativeModels=listOf(
            ChatModelInfo("flash","Flash","",listOf("off","low","high","max"),"high",listOf("text","image")),
            ChatModelInfo("plain","Plain","",emptyList(),"",listOf("text")))
        val current=androidx.compose.runtime.mutableStateOf(ComposerInfo(true,false,"",nativeModels,"flash","high",emptyList(),usage(),SkillDraftInfo(emptyList(),emptyList(),true,false,""),emptyList()))
        val selected=java.util.concurrent.atomic.AtomicReference("")
        overlay({HarnessModelChoices(current.value,true){model,effort->selected.set("$model:$effort");current.value=current.value.copy(model=model,effort=effort)}}) {
            waitFor("Native Flash and effort available"){find(text="Flash")!=null&&find(text="深入思考")!=null}
            assertTrue(actionNode(requireNotNull(find(text="深入思考")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Native maximum effort is offered"){find(text="最大思考")!=null}
            screenshot("harness-native-effort-menu.png")
            assertTrue(actionNode(requireNotNull(find(text="最大思考")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Exact native effort submitted"){selected.get()=="flash:max"}
            waitFor("Model selector returns after effort menu closes"){find(text="Flash")!=null}
            assertTrue(actionNode(requireNotNull(find(text="Flash")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Native model dropdown"){find(text="Plain")!=null}
            assertTrue(actionNode(requireNotNull(find(text="Plain")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Non-reasoning model has empty native effort"){selected.get()=="plain:"}
            waitFor("Effort menu closes for non-reasoning model"){find(text="Plain")!=null&&find(text="最大思考")==null}
            assertNull(find(text="深入思考"))
            screenshot("harness-model-without-effort.png")
        }
    }

    @Test fun approvalShowsActualOperationAndSendsOneExplicitChoice() {
        val selected=java.util.concurrent.atomic.AtomicInteger(-1)
        val approval=ApprovalInfo("owned-approval","",null,"df -h\nfree -h","/home/owned",listOf(ApprovalChoice("Yes"),ApprovalChoice("No")),"bash",emptyList())
        overlay({Box(Modifier.padding(16.dp)){ApprovalCard(approval,true,selected::set)}}) {
            waitFor("Readable operation"){find(text="运行命令")!=null&&find(text="df -h")!=null}
            assertNotNull(find(text="free -h"));assertNotNull(find(text="工作目录：/home/owned"));assertNotNull(find(text="允许一次"))
            assertNull(find(text="{\"command\""));screenshot("readable-command-approval.png")
            assertTrue(actionNode(requireNotNull(find(text="允许一次")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Exact allow choice"){selected.get()==0}
        }
        val file=approval.copy(command=null,operation="read",details=listOf(ApprovalDetailInfo("path","/home/owned/note.txt")))
        overlay({Box(Modifier.padding(16.dp)){ApprovalCard(file,true,selected::set)}}) {
            waitFor("File approval"){find(text="读取文件")!=null&&find(text="/home/owned/note.txt")!=null}
            screenshot("readable-file-approval.png")
            assertTrue(actionNode(requireNotNull(find(text="拒绝")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Exact reject choice"){selected.get()==1}
        }
    }

    @Test fun permissionMenuShowsNativeOptionsAndSendsExactPolicy() {
        val settings=androidx.compose.runtime.mutableStateOf(listOf(SessionSettingInfo("approval_mode","Tool approval","ask",listOf(SettingChoiceInfo("ask","Ask"),SettingChoiceInfo("risk","Risk"),SettingChoiceInfo("bypass","Bypass")))))
        val selected=java.util.concurrent.atomic.AtomicReference("")
        overlay({androidx.compose.material3.AlertDialog(onDismissRequest={},title={androidx.compose.material3.Text(test.targetContext.getString(R.string.chat_model_settings))},text={SessionPermissionControls(settings.value,true){id,value->selected.set("$id:$value");settings.value=settings.value.map{it.copy(currentValue=value)}}},confirmButton={androidx.compose.material3.TextButton(onClick={}){androidx.compose.material3.Text("关闭")}})}) {
            waitFor("Permission setting"){find(text="每次询问")!=null}
            assertNotNull(find(text="从下一条消息起生效"))
            assertTrue(actionNode(requireNotNull(find(text="每次询问")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Actual native choices"){find(text="全部允许")!=null&&find(text="按风险策略询问")!=null}
            screenshot("permission-policy-menu.png")
            assertTrue(actionNode(requireNotNull(find(text="按风险策略询问")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Native policy value submitted"){selected.get()=="approval_mode:risk"}
            waitFor("Native choice retained"){find(text="由已配置的风险检查")!=null}
            screenshot("permission-policy-selected.png")
        }
    }

    @Test fun modelPanelUsageShowsIndependentPeriodsAndMissingDataIsNotZero() {
        overlay({AccountUsageDialog(usage(),"codex",true,{},{})}) {
            waitFor("5-hour quota"){find(text="5小时 · 剩余 73%")!=null}
            assertNotNull(find(text="7天 · 剩余 52%"));assertNotNull(find(text="重置"))
            screenshot("quota-owned-windows.png")
        }
        overlay({AccountUsageDialog(usage(UsageStatus.UNAVAILABLE),"codex",true,{},{})}) {
            waitFor("No data explanation"){find(text="未提供可显示")!=null}
            assertNull(find(text="剩余 0%"));assertNull(find(text="剩余 100%"))
            screenshot("quota-owned-unavailable.png")
        }
        overlay({AccountUsageAccess(usage()) {}}) {
            waitFor("Model picker quota access"){find(text="套餐余量")!=null}
            assertNotNull(find(text="5小时 73%"));assertNotNull(find(text="7天 52%"))
            screenshot("quota-owned-model-access.png")
        }
    }

    @Test fun resetCardCountAndCountdownRemainSeparateFromMissingExpiry() {
        val at=System.currentTimeMillis()/1000+2*86400+3*3600+300
        overlay({AccountUsageDialog(usage().copy(resetCredits=UsageResetCreditsInfo(2uL,at,true)),"codex",true,{},{})}) {
            waitFor("Reset-card count"){find(text="可用重置卡：2 次")!=null}
            assertNotNull(find(text="最早到期还剩 2天3小时"))
            assertNotNull(find(text="到期"))
            screenshot("quota-owned-reset-cards.png")
        }
        overlay({AccountUsageDialog(usage().copy(resetCredits=UsageResetCreditsInfo(3uL,null,false)),"codex",true,{},{})}) {
            waitFor("Known count without expiration"){find(text="可用重置卡：3 次")!=null}
            assertNotNull(find(text="到期时间暂未提供"))
            assertNull(find(text="无到期限制"))
        }
    }

    @Test fun commentaryIsVisibleWhileToolDetailsStayCollapsed() {
        fun text(id:String,body:String,progress:Boolean)=ItemInfo(id,ItemRole.ASSISTANT,listOf(BlockInfo.Text(body)),progress,null)
        val tool=ItemInfo("owned-call",ItemRole.TOOL,listOf(BlockInfo.Tool(ToolInfo("Bash","读取配置","exec_command","owned-argument","owned-output",true,false,false))),false,null)
        val rows=chatEntries(listOf(text("before","已经定位问题，接下来读取配置",true),tool,text("after","配置中发现一个需要你注意的结论",true),text("final","检查完成",false)))
        overlay({Column { rows.forEach { row -> when(row) {
            is ChatEntry.Message -> MarkdownMessage(row.message.text)
            is ChatEntry.Work -> ToolGroupCard(row,false,{},{})
        } } }}) {
            waitFor("Commentary before tools"){find(text="已经定位问题，接下来读取配置")!=null}
            assertNotNull(find(text="配置中发现一个需要你注意的结论"))
            assertNotNull(find(text="检查完成"))
            assertNotNull(find(text="1 次工具调用"))
            assertNull(find(text="owned-argument"));assertNull(find(text="owned-output"))
            screenshot("commentary-visible-tools-collapsed.png")
        }
    }

    @Test fun historicalTurnErrorsShowTheCauseAndRecoveryHint() {
        val reason = "server_overloaded: Selected model is at capacity. Please try a different model."
        overlay({Column(Modifier.padding(16.dp)) {
            TurnFailureMessage(reason)
            MarkdownMessage("下一轮可以继续，上一轮的失败原因仍然保留。")
        }}) {
            waitFor("Historical capacity failure remains readable"){find(text="server_overloaded")!=null}
            assertNotNull(find(text=test.targetContext.getString(R.string.response_model_capacity)))
            assertNotNull(find(text=test.targetContext.getString(R.string.response_model_capacity_hint)))
            assertNotNull(find(text="上一轮的失败原因仍然保留"))
            screenshot("codex-historical-capacity-error.png")
        }
        overlay({TurnFailureMessage("")}) {
            waitFor("Missing provider details still produce a visible failure"){find(text=test.targetContext.getString(R.string.response_failure_no_details))!=null}
        }
    }

    @Test fun mixedItemBlocksRenderInOrderWithoutHidingTextOrTools() {
        val file = AttachmentInfo("owned-file", "owned-attachment.txt", "text/plain", 5uL, false)
        fun tool(summary: String) = BlockInfo.Tool(ToolInfo("Bash", summary, "exec_command", "owned-argument", "owned-output", true, false, false))
        val item = ItemInfo("owned-mixed", ItemRole.USER, listOf(
            BlockInfo.Text("owned-before"), BlockInfo.Attachment(file),
            BlockInfo.Text("owned-between"),
            BlockInfo.QuestionReply(QuestionReplyInfo("q", "owned-question", "owned-answer")),
            BlockInfo.Text("owned-after"), tool("owned-tool-one"),
            BlockInfo.Text("owned-end"), tool("owned-tool-two"),
        ), false, null)
        overlay({ Column(Modifier.safeDrawingPadding().padding(12.dp)) { ItemContent(item) { files -> files.forEach { attachment -> androidx.compose.material3.Text(attachment.name) } } } }) {
            val order = listOf("owned-before", file.name, "owned-between", "owned-question", "owned-answer", "owned-after", "owned-tool-one", "owned-end", "owned-tool-two")
            waitFor("Every mixed block is visible") { order.all { find(text=it)!=null } }
            val positions = order.map { text -> Rect().also { requireNotNull(find(text=text)).getBoundsInScreen(it) }.top }
            assertTrue("Block order is retained: $positions", positions.zipWithNext().all { (a,b) -> a < b })
            assertNull(find(text="owned-argument"));assertNull(find(text="owned-output"))
            screenshot("mixed-item-block-order.png")
        }
    }

    @Test fun initializationShowsOneSimpleStatusAndKeepsDraftEditingLocked() {
        val changes=java.util.concurrent.atomic.AtomicInteger()
        overlay({ Box(Modifier.fillMaxSize(),contentAlignment=androidx.compose.ui.Alignment.Center) {
            Column(horizontalAlignment=androidx.compose.ui.Alignment.CenterHorizontally) {
                SessionInitializationMessage()
                ChatDraftField("",{changes.incrementAndGet()},Modifier.padding(20.dp),enabled=false)
            }
        } }) {
            waitFor("Plain initialization status"){find(text="正在初始化会话…")!=null}
            assertNull(find(text="正在准备 agent"));assertNull(find(text="记录同步"));assertNull(find(text="重试准备"))
            val input=requireNotNull(find(description="消息输入框"))
            val arguments=Bundle().apply{putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,"must not replace the draft")}
            fun attemptEdit(node:AccessibilityNodeInfo){
                if(node.actionList.any{it.id==AccessibilityNodeInfo.ACTION_SET_TEXT})assertFalse("Disabled editor must reject text changes",node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,arguments))
                for(i in 0 until node.childCount)node.getChild(i)?.let(::attemptEdit)
            }
            attemptEdit(input)
            assertEquals("Initialization cannot mutate the preserved draft",0,changes.get())
            screenshot("session-initialization.png")
        }
        val retries=java.util.concurrent.atomic.AtomicInteger()
        overlay({ Box(Modifier.fillMaxSize(),contentAlignment=androidx.compose.ui.Alignment.Center) { SessionInitializationMessage(failed=true,onRetry={retries.incrementAndGet()}) } }) {
            waitFor("Initialization failure remains actionable"){find(text="会话初始化未完成")!=null}
            assertNull(find(text="正在初始化会话…"))
            assertTrue(actionNode(requireNotNull(find(text="重试")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Retry invoked once"){retries.get()==1}
            screenshot("session-initialization-failure.png")
        }
    }

    @Test fun latestButtonUsesOnlyAnIconAndRetainsItsAccessibleTouchTarget() {
        val clicks = java.util.concurrent.atomic.AtomicInteger()
        overlay({ Box(Modifier.fillMaxSize(), contentAlignment=androidx.compose.ui.Alignment.Center) { LatestMessageButton({clicks.incrementAndGet()}) } }) {
            waitFor("Accessible latest action"){find(description="回到最新消息")!=null}
            assertNull(find(text="回到最新"))
            val box=bounds("回到最新消息")
            val density=test.targetContext.resources.displayMetrics.density
            assertEquals(48f,box.width()/density,1f)
            assertEquals(48f,box.height()/density,1f)
            assertTrue(actionNode(requireNotNull(find(description="回到最新消息")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Jump action fires once"){clicks.get()==1}
            screenshot("latest-icon-only.png",box)
        }
    }

    @Test fun answeredQuestionsKeepPromptsAboveDistinctAnswerPanels() {
        val replies = listOf(
            QuestionReplyInfo("owned-q1", "你希望更新到哪个 `main` 分支？", "更新最新版本"),
            QuestionReplyInfo("owned-q2", "是否保留现有 SSH 信任记录？", "保留，另外检查新版本安装。\n这是一段多行回答。"),
        )
        overlay({Column(Modifier.padding(20.dp)) { AnsweredQuestions(replies) }}) {
            waitFor("Question and answer labels"){find(text="模型提问")!=null && find(text="你的回答")!=null}
            assertNotNull(find(text="更新最新版本"))
            val question = requireNotNull(find(text="是否保留现有 SSH 信任记录"))
            val answer = requireNotNull(find(text="保留，另外检查新版本安装"))
            val q = Rect().also { question.getBoundsInScreen(it) }
            val a = Rect().also { answer.getBoundsInScreen(it) }
            assertTrue("Question must stay above its answer", q.bottom < a.top)
            val bitmap = test.uiAutomation.takeScreenshot()
            val x = (q.left + 2 * test.targetContext.resources.displayMetrics.density).toInt()
            assertEquals(KiteColors.Surface.toArgb(), bitmap.getPixel(x, q.top - 2))
            assertEquals(KiteColors.UserBubble.toArgb(), bitmap.getPixel(x, a.top - 2))
            bitmap.recycle()
            screenshot("question-answer-paired-panels.png")
        }
    }

    @Test fun pendingQuestionsReplaceThinkingWithAnExplicitWaitingLabel() {
        overlay({ResponseFeedback(responsePresentationPhase(ResponsePhase.THINKING,false,true,false),false)}) {
            waitFor("Waiting for an answer"){find(text="等待你的回答")!=null}
            assertNull(find(text="正在思考"))
            screenshot("question-waiting-instead-of-thinking.png")
        }
    }

    @Test fun actualWorkspaceKeepsSmallDragsQuietAndComposerHasBottomSpace() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("qa_native_history")=="true")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        main{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        waitFor("Core initialized"){vm.engine.state.value?.initialized==true}
        runBlocking{vm.engine.core.disconnect()}
        main{vm.closeDesktop();vm.settingsVisible=false;vm.draft=""}
        test.waitForIdleSync();SystemClock.sleep(500)
        val delta=runBlocking{vm.engine.core.conversationSince("owned-native-refinement",0uL)}
        val context=requireNotNull(vm.engine.state.value).conversationContext
        val messages=(0 until 501).map{i->ItemInfo("owned-native-$i",ItemRole.ASSISTANT,listOf(BlockInfo.Text(if(i==499)(1..28).joinToString("\n\n"){"Owned final paragraph $it for scroll-margin acceptance."}else if(i==500)"Owned tail status" else"Owned history row $i\nSecond line")),false,null)}
        try{
            main{vm.engine.timeline.prepareContext(context);vm.engine.timeline.apply(ConversationDeltaInfo(context,100000uL,true,listOf(ConversationEditInfo(0uL,0uL,messages))))}
            waitFor("History visible"){find(description="聊天记录")!=null&&find(description="消息输入框")!=null}
            test.waitForIdleSync();SystemClock.sleep(800)
            val density=test.targetContext.resources.displayMetrics.density
            val field=Rect().also{actionNode(requireNotNull(find(description="消息输入框")),AccessibilityNodeInfo.ACTION_SET_TEXT).getBoundsInScreen(it)};val metrics=test.targetContext.resources.displayMetrics
            assertTrue("Empty input too tall: $field",field.height()/density<=58f)
            assertTrue("Composer touches display bottom: $field",metrics.heightPixels-field.bottom>=10*density)
            screenshot("composer-owned-empty.png",Rect(0,(field.top-8*density).toInt().coerceAtLeast(0),metrics.widthPixels,metrics.heightPixels))
            assertNull(find(description="回到最新消息"))
            val chat=bounds("聊天记录")
            gesture(chat,0f);assertNull(find(description="回到最新消息"))
            gesture(chat,36*density);assertNull(find(description="回到最新消息"))
            val grown=messages[499].copy(blocks=listOf(BlockInfo.Text(messages[499].text+"\n\n"+(1..12).joinToString("\n\n"){"Owned streamed paragraph $it grows while reading."})))
            main{vm.engine.timeline.apply(ConversationDeltaInfo(context,100001uL,false,listOf(ConversationEditInfo(499uL,1uL,listOf(grown)))))}
            waitFor("Latest after a paused reply grows beyond the viewport"){find(description="回到最新消息")!=null}
            assertTrue(actionNode(requireNotNull(find(description="回到最新消息")),AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Latest hidden after streamed update"){find(description="回到最新消息")==null}
            test.waitForIdleSync();SystemClock.sleep(400)
            gesture(chat,320*density)
            waitFor("Latest button after meaningful drag"){find(description="回到最新消息")!=null}
            val latest=actionNode(requireNotNull(find(description="回到最新消息")),AccessibilityNodeInfo.ACTION_CLICK);assertTrue(latest.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Latest hidden after returning"){find(description="回到最新消息")==null}
            assertEquals(501,vm.engine.timeline.messages.size)
            val longDraft=(1..9).joinToString("\n"){"Owned editable draft line $it"}
            val node=actionNode(requireNotNull(find(description="消息输入框")),AccessibilityNodeInfo.ACTION_SET_TEXT)
            assertTrue(node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,Bundle().apply{putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,longDraft)}))
            waitFor("Direct editor updates draft"){vm.draft==longDraft}
            assertTrue(bounds("消息输入框").height()/density<=134f)
            File(test.targetContext.filesDir,"workspace-refinement-validation.json").writeText("""{"records":501,"touchDoesNotShowLatest":true,"smallDragDoesNotShowLatest":true,"streamingGrowthShowsLatest":true,"largeDragShowsLatest":true,"returnHidesLatest":true,"dataRetained":true,"emptyInputDp":${field.height()/density},"bottomGapDp":${(metrics.heightPixels-field.bottom)/density},"nativeDraftInput":true}""")
        }finally{
            main{vm.draft="";vm.engine.timeline.prepareContext(delta.context);vm.engine.timeline.apply(delta);activity.finish()}
        }
    }
}
