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
import androidx.compose.runtime.Composable
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
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Synthetic display data only; no user conversation is submitted or persisted. */
class WorkspaceRefinementPresentationTest {
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
        return walk(test.uiAutomation.rootInActiveWindow)
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
        fun text(id:String,body:String,progress:Boolean)=MessageInfo(emptyList(),emptyList(),id,MessageKind.ASSISTANT,body,progress,null)
        val tool=MessageInfo(emptyList(),emptyList(),"owned-call",MessageKind.TOOL,"",false,ToolInfo("Bash","读取配置","exec_command","owned-argument","owned-output",true,false,false))
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
        val messages=(0 until 501).map{i->MessageInfo(emptyList(),emptyList(),"owned-native-$i",MessageKind.ASSISTANT,if(i==499)(1..28).joinToString("\n\n"){"Owned final paragraph $it for scroll-margin acceptance."}else if(i==500)"Owned tail status" else"Owned history row $i\nSecond line",false,null)}
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
            val grown=messages[499].copy(text=messages[499].text+"\n\n"+(1..12).joinToString("\n\n"){"Owned streamed paragraph $it grows while reading."})
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
