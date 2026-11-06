package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
import android.os.Build
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.*
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.launch
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Synthetic display snapshots only; no host, provider turn or credentials. */
class ChatStabilityPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun main(block: () -> Unit) {
        var error: Throwable?=null
        test.runOnMainSync{try{block()}catch(caught:Throwable){error=caught}}
        error?.let{throw it}
    }
    private fun waitFor(label: String, condition: () -> Boolean) {
        val end=SystemClock.uptimeMillis()+15000
        while(!condition() && SystemClock.uptimeMillis()<end) SystemClock.sleep(50)
        assertTrue(label,condition())
    }
    private fun reply(view: View): MarkdownTextView? {
        if(view is MarkdownTextView && view.text.startsWith("Owned stable tail")) return view
        if(view is ViewGroup) for(i in 0 until view.childCount) reply(view.getChildAt(i))?.let{return it}
        return null
    }
    private fun node(text: String): AccessibilityNodeInfo? {
        fun walk(current: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            current?:return null
            if(current.isVisibleToUser && current.text?.contains(text)==true)return current
            for(i in 0 until current.childCount)walk(current.getChild(i))?.let{return it}
            return null
        }
        if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow)
    }

    @Test fun viewportResizesResolveTheTailBeforeDrawingAndKeepHistoryAnchored() {
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root=activity.findViewById<FrameLayout>(android.R.id.content)
        val originalViews=mutableListOf<View>()
        lateinit var panel: ComposeView
        lateinit var list: LazyListState
        lateinit var scope: kotlinx.coroutines.CoroutineScope
        val height=mutableStateOf(420.dp)
        val following=mutableStateOf(true)
        var bottom=0f
        var observer: ViewTreeObserver.OnDrawListener?=null
        val gaps=mutableListOf<Int>()
        var missing=0
        try {
            main {
                for(i in 0 until root.childCount)root.getChildAt(i).let{originalViews+=it;it.visibility=View.INVISIBLE}
                panel=ComposeView(activity).apply{setContent{KiteTheme{
                    list=rememberLazyListState()
                    scope=rememberCoroutineScope()
                    ChatTailFollow(list,"owned-viewport"){following.value}
                    Column(Modifier.fillMaxSize().padding(24.dp)) {
                        ChatViewport(list,"owned-viewport",{following.value},Modifier.fillMaxWidth().height(height.value).onGloballyPositioned{bottom=it.boundsInWindow().bottom}) {
                            LazyColumn(Modifier.fillMaxSize(),state=list,contentPadding=PaddingValues(vertical=10.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                                items(40,key={"owned-history-$it"}){Text("Owned viewport history $it\nRetain the reading anchor.")}
                                item(key="owned-reply"){MarkdownMessage("Owned stable tail\n\nViewport resize validation.")}
                                item(key="conversation-tail"){Spacer(Modifier.height(8.dp))}
                            }
                        }
                    }
                }}}
                root.addView(panel,FrameLayout.LayoutParams(-1,-1))
            }
            waitFor("The viewport follows its measured tail"){!list.canScrollForward && list.layoutInfo.visibleItemsInfo.lastOrNull()?.key=="conversation-tail"}
            SystemClock.sleep(200)
            lateinit var native: MarkdownTextView
            var expected=0
            fun gap()=bottom.toInt()-(IntArray(2).also(native::getLocationInWindow)[1]+native.height)
            main {
                native=requireNotNull(reply(panel));expected=gap()
                observer=ViewTreeObserver.OnDrawListener {
                    if(reply(panel)!==native || !native.isShown || native.height==0)missing++
                    gaps+=gap()
                }.also{root.viewTreeObserver.addOnDrawListener(it)}
            }
            for(next in listOf(300,460,280,400,420)) {
                main {height.value=next.dp}
                SystemClock.sleep(120);main{root.invalidate()};test.waitForIdleSync()
            }
            main {
                File(test.targetContext.getExternalFilesDir(null),"chat-viewport-draws.json").writeText("""{"expectedGap":$expected,"drawnGaps":$gaps}""")
                assertTrue(gaps.size>=5)
                assertEquals("Viewport changes retain the native reply in every draw",0,missing)
                assertTrue("Every resized frame must already be at the tail: $gaps",gaps.all{kotlin.math.abs(it-expected)<=1})
                observer?.let{root.viewTreeObserver.removeOnDrawListener(it)};observer=null
                following.value=false;scope.launch{list.scrollToItem(5,11)}
            }
            waitFor("History is positioned"){list.firstVisibleItemIndex==5 && list.firstVisibleItemScrollOffset==11}
            for(next in listOf(300,460,280,420)) {
                main {height.value=next.dp}
                test.waitForIdleSync();SystemClock.sleep(100)
                assertEquals(5,list.firstVisibleItemIndex)
                assertEquals(11,list.firstVisibleItemScrollOffset)
            }
        } finally {
            main {
                observer?.let{root.viewTreeObserver.removeOnDrawListener(it)}
                panel.disposeComposition();root.removeView(panel);originalViews.forEach{it.visibility=View.VISIBLE};activity.finish()
            }
        }
    }

    @Test fun reconnectFailuresKeepTheSameReplyAndPositionOnEveryDraw() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("qa_native_history")=="true")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        main { vm=ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        waitFor("Core initialized"){vm.engine.state.value?.initialized==true}
        val saved=requireNotNull(vm.engine.state.value)
        assumeTrue("Only an empty isolated app may use this fixture",saved.hosts.isEmpty())
        val history=runBlocking{vm.engine.core.conversationSince(saved.conversationContext,0uL)}
        @Suppress("UNCHECKED_CAST")
        val display=vm.engine.javaClass.getDeclaredField("current").apply{isAccessible=true}.get(vm.engine) as MutableStateFlow<CoreSnapshot?>
        val base=saved.copy(connection=ConnectionStatus.RECONNECTING,connectionStep=ConnectionStep.SSH,
            selectedHost="owned-stable-host",selectedSession="owned-stable-session",failure=null,
            sessions=listOf(SessionInfo("owned-stable-session","codex","Owned retry conversation","",null,false,0,null,"saved",true,emptyList(),false)))
        val records=(0..60).map { index->ItemInfo("owned-stable-$index",ItemRole.ASSISTANT,
            listOf(BlockInfo.Text(if(index==60)"Owned stable tail\n\nRetain this reply while transport retries." else "Owned retry history $index\n\n"+(1..4).joinToString("\n\n"){"Owned paragraph $it."})),false,null) }
        val root=activity.findViewById<View>(android.R.id.content)
        val positions=mutableListOf<Int>()
        var missing=0
        lateinit var original: MarkdownTextView
        var expected=0
        var observer: ViewTreeObserver.OnDrawListener?=null
        try {
            main {
                vm.settingsVisible=false
                display.value=base
                vm.engine.timeline.prepareContext(saved.conversationContext)
                vm.engine.timeline.apply(ConversationDeltaInfo(saved.conversationContext,100000uL,true,listOf(ConversationEditInfo(0uL,0uL,records))))
            }
            waitFor("The cached tail is visible"){var shown=false;main{shown=reply(root)?.isShown==true};shown}
            SystemClock.sleep(500)
            main {
                original=requireNotNull(reply(root))
                expected=IntArray(2).also(original::getLocationOnScreen)[1]
                observer=ViewTreeObserver.OnDrawListener {
                    positions+=IntArray(2).also(original::getLocationOnScreen)[1]
                    if(reply(root)!==original || !original.isShown || original.height==0) missing++
                }.also{root.viewTreeObserver.addOnDrawListener(it)}
            }
            repeat(12){ step->
                val failure=if(step%3==0)null else FailureInfo(FailureKind.CONNECTION,
                    (1..if(step%3==1)1 else 4).joinToString("\n"){"Owned connection timeout detail $it"},"","","")
                main {display.value=base.copy(failure=failure,connectionStep=if(step%2==0)ConnectionStep.SSH else ConnectionStep.IDLE)}
                SystemClock.sleep(100)
                main {root.invalidate()}
                test.waitForIdleSync()
            }
            main {
                File(test.targetContext.getExternalFilesDir(null),"chat-reconnect-draws.json").writeText("""{"initialTop":$expected,"drawnTops":$positions,"missingDraws":$missing}""")
                assertTrue("Every retry reaches an Android draw",positions.size>=12)
                assertEquals("Cached replies must never disappear during retries",0,missing)
                assertTrue("Retry facts must not move the chat: $positions",positions.all{kotlin.math.abs(it-expected)<=1})
            }
            val shot=test.uiAutomation.takeScreenshot()
            File(test.targetContext.getExternalFilesDir(null),"chat-reconnect-stable.png").outputStream().use{shot.compress(Bitmap.CompressFormat.PNG,100,it)}
            shot.recycle()
            main {observer?.let{root.viewTreeObserver.removeOnDrawListener(it)};observer=null}
            var action=requireNotNull(node("详情"))
            while(!action.isClickable)action=requireNotNull(action.parent)
            assertTrue(action.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("The retry cause remains readable in diagnostics"){node("Owned connection timeout detail")!=null}
        } finally {
            main {
                observer?.let{root.viewTreeObserver.removeOnDrawListener(it)}
                display.value=saved
                vm.engine.timeline.prepareContext(history.context);vm.engine.timeline.apply(history)
                activity.finish()
            }
        }
    }
}
