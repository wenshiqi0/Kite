package io.github.wenshiqi0.kite

import android.graphics.Bitmap
import android.graphics.Canvas
import android.content.Intent
import android.graphics.Paint
import android.text.Spanned
import android.text.style.URLSpan
import android.view.View
import android.widget.FrameLayout
import android.widget.ScrollView
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.media.DesktopViewport
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.media.RemoteInputConnection
import io.github.wenshiqi0.kite.media.RemoteTextBuffer
import android.view.MotionEvent
import android.view.inputmethod.EditorInfo
import android.os.SystemClock
import io.github.wenshiqi0.kite.ui.MarkdownRenderer
import io.github.wenshiqi0.kite.ui.MarkdownTextView
import io.github.wenshiqi0.kite.ui.markdownLabel
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.FontFamily
import java.io.File
import org.junit.Assert.*
import org.junit.Test
import io.noties.markwon.ext.tables.TableRowSpan

/** Exercises Android's real measure/layout and spans, which host JVM stubs cannot. */
class NativePresentationTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private fun onMain(block: () -> Unit) {
        var failure: Throwable? = null
        instrumentation.runOnMainSync { try { block() } catch (error: Throwable) { failure = error } }
        failure?.let { throw it }
    }
    private fun exact(n: Int) = View.MeasureSpec.makeMeasureSpec(n, View.MeasureSpec.EXACTLY)
    private fun layout(view: View, width: Int, height: Int) {
        view.measure(exact(width), exact(height)); view.layout(0, 0, width, height)
    }

    @Test fun firstSplitMeasuresVideoInTheSameLayoutPass() = onMain {
        val parent = VideoSurface(instrumentation.targetContext, { fail("Unattached view cannot create a renderer") }, {})
        val child = parent.getChildAt(0)
        layout(parent, 1080, 764)
        assertEquals(1080, child.width)
        assertEquals(608, child.height)
        assertEquals(0, parent.target.toInt())
        // Mode changes, divider movement and keyboard sizing need no second pass.
        for ((w, h) in listOf(1080 to 410, 1080 to 930, 2316 to 800, 1080 to 764)) {
            layout(parent, w, h)
            val expected = DesktopViewport().apply { resize(w.toFloat(), h.toFloat()) }
            for (zoom in listOf(1f, 2f, 4f)) {
                parent.zoom = zoom; expected.zoomAt(zoom)
                val src = expected.viewAt(1000f, 600f)
                val x = child.left + child.translationX + child.width * child.scaleX * 1000f / 1920
                val y = child.top + child.translationY + child.height * child.scaleY * 600f / 1080
                assertEquals(src.x, x, .01f); assertEquals(src.y, y, .01f)
            }
            parent.zoom = 1f
        }
        parent.zoom = 4f; parent.pip = true
        assertEquals(1080f, child.width * child.scaleX, .01f)
        assertEquals(607.5f, child.height * child.scaleY, .01f)
        parent.pip = false
        assertEquals(4320f, child.width * child.scaleX, .01f)
        parent.setSourceSize(1080, 1920)
        layout(parent, 1080, 764)
        assertEquals(764, child.height)
        assertEquals(430, child.width)
    }

    @Test fun markdownCreatesRichSpansAndRemeasuresStreamingText() = onMain {
        val view = MarkdownTextView(instrumentation.targetContext)
        FrameLayout(instrumentation.targetContext).addView(view, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.WRAP_CONTENT))
        val short = "## 进度\n\n正在 **检查** `build`。"
        view.bind(short)
        measureText(view, 900)
        val height = view.measuredHeight
        val original = view.text
        view.bind(short)
        assertSame("Unchanged snapshots must not replace selectable text", original, view.text)
        val source = """
            # 部署结果

            已完成 **富文本回复**，支持 *重点* 和 ~~旧内容~~。

            ## 下一步

            1. 打开 `Kite`。
            2. 查看桌面与回复。

            - [x] 保留源分辨率
            - [ ] 验证手机横屏

            > 引用：操作和观看共用桌面坐标系。

            ```kotlin
            val desktop = "1920 × 1080"
            println(desktop)
            ```

            | 模式 | 输入 |
            | --- | --- |
            | 只看 | 缩放和平移 |
            | 操作 | 远端坐标映射 |

            [查看说明](https://example.com/docs)
        """.trimIndent()
        view.bind(source)
        measureText(view, 900)
        assertTrue("Streaming completion must expand the native view", view.measuredHeight > height)
        assertTrue(view.isTextSelectable)
        val text = view.text as Spanned
        assertFalse(text.toString().contains("**"))
        assertFalse(text.toString().contains("```"))
        assertFalse(text.toString().contains("https://example.com"))
        val spans = text.getSpans(0, text.length, Any::class.java).map { it.javaClass.simpleName }
        for (name in listOf("Heading", "StrongEmphasis", "Emphasis", "Code", "BlockQuote", "Table", "Link", "TaskList", "Strikethrough")) {
            assertTrue("Missing $name span: $spans", spans.any { it.contains(name) })
        }
        val file = File(instrumentation.targetContext.getExternalFilesDir(null), "markdown-validation.png")
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap); canvas.drawColor(0xFFF5F4EF.toInt()); view.draw(canvas)
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }; bitmap.recycle()
        measureText(view, 420)
        assertTrue(view.measuredHeight > height)
        assertEquals(420, view.width)
        view.bind("**完成**"); measureText(view, 420)
        assertTrue("Replacing a reply must shrink its old height", view.measuredHeight < height)
    }

    @Test fun onlyExplicitExternalLinkSchemesOpen() = onMain {
        for (link in listOf("https://example.com/a?q=1", "HTTP://example.com", "mailto:test@example.com")) assertTrue(link, MarkdownRenderer.isExternalLink(link))
        for (link in listOf("/home/me/file", "file:///sdcard/a", "content://a", "intent://a", "javascript:alert(1)", "https:", "#section")) assertFalse(link, MarkdownRenderer.isExternalLink(link))
        val text = MarkdownRenderer.get(instrumentation.targetContext).toMarkdown("[local](/home/me/file) [blocked](intent://launch) [web](https://example.com)")
        val links = text.getSpans(0, text.length, URLSpan::class.java)
        assertEquals(1, links.size)
        assertEquals("https://example.com", links.single().url)
        assertEquals("local blocked web", text.toString())
    }

    @Test fun tablesSettleOnScreenWithoutResizeOrTouch() {
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        lateinit var overlay: ScrollView
        lateinit var view: MarkdownTextView
        var added = false
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        try {
            onMain {
                overlay = ScrollView(activity).apply { setBackgroundColor(0xFFF5F4EF.toInt()) }
                view = MarkdownTextView(activity)
                val pad = (20 * activity.resources.displayMetrics.density).toInt()
                view.setPadding(pad, pad, pad, pad)
                view.bind("# 富文本回复\n\n**粗体**、*强调* 和 `行内代码`。\n\n> 引用内容\n\n1. 支持列表\n2. 保留文字选择\n\n```kotlin\nval size = \"1920 × 1080\"\n```\n\n| 模式 | 输入方式 | 画面 |\n| --- | --- | --- |\n| 只看 | 双指缩放和平移 | 保留源分辨率 |\n| 操作 | 将触摸映射到远端坐标 | 放大后仍可操作 |\n\n[查看说明](https://example.com/docs)")
                overlay.addView(view, FrameLayout.LayoutParams((320 * activity.resources.displayMetrics.density).toInt(), FrameLayout.LayoutParams.WRAP_CONTENT))
                root.addView(overlay, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
                added = true
            }
            // Tables compute cell layouts on draw, then schedule their own native
            // remeasure. The attached view must settle without a gesture or resize.
            instrumentation.uiAutomation.waitForIdle(200, 4000)
            instrumentation.waitForIdleSync()
            onMain {
                val text = view.text as Spanned
                val rows = text.getSpans(0, text.length, TableRowSpan::class.java)
                assertEquals(3, rows.size)
                for (row in rows) {
                    val fm = Paint.FontMetricsInt()
                    row.getSize(view.paint, text, text.getSpanStart(row), text.getSpanEnd(row), fm)
                    assertTrue("Table row must include its measured cell content", fm.descent - fm.ascent > view.paint.fontSpacing)
                }
            }
            val bitmap = instrumentation.uiAutomation.takeScreenshot()
            File(instrumentation.targetContext.getExternalFilesDir(null), "markdown-onscreen.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
        } finally {
            onMain { if (added) root.removeView(overlay); activity.finish() }
        }
    }

    private fun measureText(view: View, width: Int) {
        view.measure(exact(width), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
    }

    @Test fun choiceLabelsRenderEmphasisAndPreserveLiteralCode() = onMain {
        val raw = "**推荐** · *快速* · `a*b` · [文档](https://example.com)"
        val label = markdownLabel(instrumentation.targetContext, raw)
        assertEquals("推荐 · 快速 · a*b · 文档", label.text.replace("\u00a0",""))
        assertTrue(label.spanStyles.any { it.item.fontWeight == FontWeight.Bold && label.text.substring(it.start,it.end)=="推荐" })
        assertTrue(label.spanStyles.any { it.item.fontFamily == FontFamily.Monospace && label.text.substring(it.start,it.end).trim()=="a*b" })
        assertEquals("**原样**", markdownLabel(instrumentation.targetContext, "\\*\\*原样\\*\\*").text)
    }
    @Test fun streamingStrongTextDoesNotExposeItsLeadingMarkerOrAlterCode() = onMain {
        val view=MarkdownTextView(instrumentation.targetContext)
        FrameLayout(instrumentation.targetContext).addView(view,FrameLayout.LayoutParams(-1,-2))
        view.bind("**正在输出",true)
        assertEquals("正在输出",view.text.toString())
        assertTrue((view.text as Spanned).getSpans(0,view.text.length,Any::class.java).any {it.javaClass.simpleName.contains("StrongEmphasis")})
        view.bind("**正在输出**",false);assertEquals("正在输出",view.text.toString())
        view.bind("```\n**原样",true);assertTrue(view.text.toString().contains("**原样"))
        view.bind("\\*\\*原样",true);assertEquals("**原样",view.text.toString())
    }
    @Test fun zoomed_one_finger_drag_pans_the_viewport_without_a_second_finger()=onMain{
        val parent=VideoSurface(instrumentation.targetContext,{fail("No renderer expected")},{})
        layout(parent,1080,764);parent.takeover=true;parent.trackpad=false;parent.zoom=3f
        val child=parent.getChildAt(0);val before=child.translationX;val t=SystemClock.uptimeMillis()
        fun touch(action:Int,x:Float){val event=MotionEvent.obtain(t,SystemClock.uptimeMillis(),action,x,350f,0);try{parent.onTouchEvent(event)}finally{event.recycle()}}
        touch(MotionEvent.ACTION_DOWN,400f);touch(MotionEvent.ACTION_MOVE,600f);touch(MotionEvent.ACTION_UP,600f)
        assertEquals(before+200f,child.translationX,.5f)
    }
    @Test fun remote_ime_holds_composition_and_flushes_committed_text_before_enter()=onMain{
        val events=mutableListOf<String>();val tasks=mutableListOf<()->Unit>()
        val buffer=RemoteTextBuffer({events.add("text:"+it);true},{tasks.add(it)},{fail("send failed")})
        val connection=RemoteInputConnection(View(instrumentation.targetContext),buffer,{events.add("key:"+it);true},{true})
        connection.setComposingText("n",1);connection.setComposingText("ni",1)
        assertTrue(events.isEmpty());assertTrue(tasks.isEmpty())
        connection.commitText("你",1);connection.commitText("a",1)
        connection.deleteSurroundingText(1,0)
        assertTrue(events.isEmpty());assertEquals(1,tasks.size)
        connection.performEditorAction(EditorInfo.IME_ACTION_DONE)
        assertEquals(listOf("text:你","key:13"),events)
        connection.deleteSurroundingText(1,0)
        assertEquals("key:8",events.last())
        connection.commitText("x",1);connection.setComposingText("pin",1);connection.closeConnection()
        assertEquals("text:x",events.last());assertFalse(events.any{it.contains("pin")})
        tasks.forEach{it()};assertEquals(4,events.size)
    }
}
