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
            if(description!=null&&node.contentDescription?.toString()==description || text!=null&&node.text?.toString()?.contains(text)==true)return node
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
        }finally{main{panel.disposeComposition();root.removeView(panel);original.forEach{it.visibility=View.VISIBLE};activity.finish()}}
    }
    private fun usage(status:UsageStatus=UsageStatus.READY)=AccountUsageInfo(status,if(status==UsageStatus.UNAVAILABLE)UsageUnavailable.NO_DATA else UsageUnavailable.NONE,false,false,"pro",
        if(status==UsageStatus.UNAVAILABLE)emptyList()else listOf(UsageBucketInfo("codex",null,UsageWindowInfo(73u,300uL,System.currentTimeMillis()/1000+7200),UsageWindowInfo(52u,10080uL,System.currentTimeMillis()/1000+259200),null,false,false)),true,null,System.currentTimeMillis())

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
        fun text(id:String,body:String,progress:Boolean)=MessageInfo(emptyList(),id,MessageKind.ASSISTANT,body,progress,null)
        val tool=MessageInfo(emptyList(),"owned-call",MessageKind.TOOL,"",false,ToolInfo("Bash","读取配置","exec_command","owned-argument","owned-output",true,false,false))
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
        val messages=(0 until 501).map{i->MessageInfo(emptyList(),"owned-native-$i",MessageKind.ASSISTANT,if(i==499)(1..28).joinToString("\n\n"){"Owned final paragraph $it for scroll-margin acceptance."}else if(i==500)"Owned tail status" else"Owned history row $i\nSecond line",false,null)}
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
