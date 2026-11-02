package io.github.wenshiqi0.kite

import android.app.NotificationManager
import android.content.Intent
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.data.ResourceNotifications
import io.github.wenshiqi0.kite.ui.HostResourcesCard
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference

/** Synthetic resource records only. No provider prompt or live limit change. */
class HostResourcesPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private val gib = 1073741824uL
    private fun profile() = HostResourcesInfo(
        available=true,problem=null,sampledAtMs=1u,cpuPercent=72.5,cpuCores=16u,
        loadOne=2.1,loadFive=1.4,loadFifteen=0.8,memoryTotalBytes=20uL*gib,memoryAvailableBytes=12uL*gib,
        swapTotalBytes=4uL*gib,swapUsedBytes=gib/2uL,memoryPressureSome=0.0,memoryPressureFull=0.0,
        agentMemoryUsedBytes=2uL*gib,agentReclaimableBytes=gib/4uL,agentMemoryLimitBytes=10uL*gib,
        agentMigrationReservedBytes=0uL,agentStartupReservedBytes=0uL,agentKernelLimitBytes=10uL*gib,
        agentCpuPercent=15.0,agentProcessCount=10u,agentOomKills=0uL,limitEnforced=true,
        memoryPressure=HostMemoryPressure.NORMAL,reason=MemoryWarningReason.NONE,canStartAgents=true,
        processes=listOf(ResourceProcessInfo(42u,"fixture-worker",gib,10.0,true)),
    )
    private fun main(block:()->Unit) { test.runOnMainSync(block) }
    private fun find(text:String?=null,editable:Boolean=false):AccessibilityNodeInfo? {
        fun walk(node:AccessibilityNodeInfo?):AccessibilityNodeInfo? {
            node?:return null
            if(text!=null&&node.text?.toString()?.contains(text)==true||editable&&node.isEditable)return node
            for(i in 0 until node.childCount)walk(node.getChild(i))?.let{return it}
            return null
        }
        if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow)
    }
    private fun waitFor(label:String,condition:()->Boolean){val end=SystemClock.uptimeMillis()+7000;while(!condition()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(50);assertTrue(label,condition())}
    private fun click(node:AccessibilityNodeInfo){var n:AccessibilityNodeInfo?=node;while(n!=null&&!n.isClickable)n=n.parent;assertTrue(requireNotNull(n).performAction(AccessibilityNodeInfo.ACTION_CLICK));test.waitForIdleSync()}
    private fun overlay(content: @Composable () -> Unit,check:()->Unit){
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root=activity.findViewById<FrameLayout>(android.R.id.content);val original=mutableListOf<View>();lateinit var panel:ComposeView
        try{
            main{for(i in 0 until root.childCount)root.getChildAt(i).let{original.add(it);it.visibility=View.INVISIBLE};panel=ComposeView(activity).apply{setContent{KiteTheme{Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())){content()}}}};root.addView(panel,FrameLayout.LayoutParams(-1,-1))}
            test.waitForIdleSync();check()
        }finally{main{panel.disposeComposition();root.removeView(panel);original.forEach{it.visibility=View.VISIBLE};activity.finish()}}
    }
    private fun capture(name:String){val bitmap=requireNotNull(test.uiAutomation.takeScreenshot());File(test.targetContext.getExternalFilesDir(null),name).outputStream().use{bitmap.compress(Bitmap.CompressFormat.PNG,100,it)};bitmap.recycle()}

    @Test fun metricsAndEditableBudgetRenderAndSubmitBinaryGiB(){
        val saved=AtomicReference<ULong?>(null)
        overlay({HostResourcesCard(profile(),true,false,{}, {saved.set(it)})}){
            waitFor("Resource overview"){find("Agent 内存预算")!=null}
            assertNotNull(find("72.5%"));assertNotNull(find("2.0 GiB / 10.0 GiB"));assertNotNull(find("系统内存上限已生效"))
            capture("host-resources-normal.png")
            click(requireNotNull(find("调整 Agent 内存上限")))
            waitFor("Budget input"){find(editable=true)!=null}
            val args=Bundle().apply{putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,"6")}
            assertTrue(requireNotNull(find(editable=true)).performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,args));test.waitForIdleSync()
            waitFor("Edited budget text"){find(editable=true)?.text?.toString()=="6"}
            capture("host-resources-limit-dialog.png")
            click(requireNotNull(find("保存上限")));waitFor("Exact binary budget callback: ${saved.get()}"){saved.get()==6uL*gib}
        }
    }
    @Test fun criticalMemoryShowsPausedStartsAndOfflineHasNoInventedMetrics(){
        val critical=profile().copy(agentMemoryUsedBytes=19uL*gib/2uL,memoryPressure=HostMemoryPressure.CRITICAL,reason=MemoryWarningReason.AGENT_BUDGET,canStartAgents=false)
        overlay({HostResourcesCard(critical,true,false,{}, {})}){
            waitFor("Critical warning"){find("Agent 内存接近上限")!=null};assertNotNull(find("已暂停新建"));capture("host-resources-critical.png")
        }
        overlay({HostResourcesCard(null,false,false,{}, {})}){
            waitFor("Disconnected state"){find("连接这台主机后查看实时资源")!=null};assertNull(find("0.0 GiB"));assertNull(find("上限已生效"))
        }
    }
    @Test fun backgroundWarningUsesTheRealNotificationManager(){
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val engine=(test.targetContext.applicationContext as KiteApplication).engine
        val host="resource-qa-"+UUID.randomUUID();val manager=test.targetContext.getSystemService(NotificationManager::class.java)
        try{
            waitFor("Core initialization"){engine.state.value?.initialized==true}
            val state=requireNotNull(engine.state.value).copy(selectedHost=host,connection=ConnectionStatus.CONNECTED,
                hostResources=profile().copy(memoryPressure=HostMemoryPressure.CRITICAL,reason=MemoryWarningReason.AGENT_BUDGET,canStartAgents=false))
            ResourceNotifications(test.targetContext).observe(state,false)
            waitFor("System resource warning"){manager.activeNotifications.any{it.tag=="resource-$host"}}
            val posted=manager.activeNotifications.first{it.tag=="resource-$host"}.notification
            assertNotNull(posted.contentIntent);assertEquals("Agent 内存接近上限",posted.extras.getString("android.title"))
        }finally{manager.cancel("resource-$host",3);test.targetContext.getSharedPreferences("host-resource-alerts",0).edit().remove("$host/time").remove("$host/level").apply();main{activity.finish()}}
    }
}
