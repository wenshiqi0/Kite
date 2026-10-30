package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.media.NativeMedia
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Explicit opt-in only. Requires the isolated 1080p fixture, never a user's desktop. */
class LiveDesktopInputTest {
    @Test fun committedImeTextReachesTheIsolatedDesktopWithoutATextDialog() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_live")=="true")
        val session=requireNotNull(args.getString("qa_session"))
        require(session.startsWith("kite-alpha12-history-"))
        val test=InstrumentationRegistry.getInstrumentation()
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        test.runOnMainSync{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(condition:()->Boolean){
            val deadline=SystemClock.uptimeMillis()+30000
            while(!condition()&&SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100)
            assertTrue("Isolated fixture did not become ready",condition())
        }
        waitFor{vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        val state=requireNotNull(vm.engine.state.value)
        val host=state.hosts.single{it.id==state.selectedHost}
        assertEquals("The test must target the loopback QA relay", "10.0.2.2",host.host)
        assertEquals(22222,host.port.toInt())
        assertEquals(48200,host.sunshinePort.toInt())
        test.runOnMainSync{vm.openSession(session)}
        waitFor{vm.engine.state.value?.selectedSession==session&&!vm.sessionBusy}
        test.runOnMainSync{vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
        waitFor{vm.engine.state.value?.desktop?.status==DesktopStatus.STREAMING}
        assertEquals(1920,vm.engine.state.value!!.desktop.sourceWidth.toInt())
        assertEquals(1080,vm.engine.state.value!!.desktop.sourceHeight.toInt())
        test.runOnMainSync{vm.takeOver(true)}
        waitFor{vm.engine.state.value?.desktop?.takeover==true}
        SystemClock.sleep(200)
        fun surface(view:View):VideoSurface? {
            if(view is VideoSurface)return view
            if(view is ViewGroup)for(i in 0 until view.childCount)surface(view.getChildAt(i))?.let{return it}
            return null
        }
        lateinit var video:VideoSurface
        test.runOnMainSync{
            video=requireNotNull(surface(activity.window.decorView))
            assertTrue(NativeMedia.mousePosition(video.target,200,940,1920,1080))
            assertTrue(NativeMedia.mouseButton(video.target,1,true))
            assertTrue(NativeMedia.mouseButton(video.target,1,false))
            assertTrue(video.openRemoteKeyboard())
        }
        SystemClock.sleep(300)
        test.runOnMainSync{
            val info=EditorInfo()
            val connection=requireNotNull(video.onCreateInputConnection(info))
            assertTrue(info.imeOptions and EditorInfo.IME_FLAG_NO_EXTRACT_UI!=0)
            assertTrue(info.imeOptions and EditorInfo.IME_FLAG_NO_FULLSCREEN!=0)
            connection.setComposingText("pin",1)
            connection.commitText("Kite ",1)
            connection.commitText("远程桌面 29",1)
        }
        // Exercise the real 200 ms scheduler, JNI sender and Sunshine text path.
        SystemClock.sleep(600)
        assertTrue(video.hasFocus())
        assertTrue(video.onCheckIsTextEditor())
    }
}
