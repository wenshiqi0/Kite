package io.github.wenshiqi0.kite.media

import android.os.Handler
import android.os.Looper

/** One ordered media input path. Fullscreen enables the quiet-window sampler. */
object DesktopInput {
    private val handler = Handler(Looper.getMainLooper())
    private var target = 0L
    private var buffer: RemoteInputBuffer? = null

    @Synchronized fun configure(id: Long, enabled: Boolean, failure: () -> Unit) {
        if (enabled && id != 0L && id == target && buffer != null) return
        buffer?.cancel(); buffer = null; target = 0
        if (enabled && id != 0L) {
            target = id
            buffer = RemoteInputBuffer({ delay, task ->
                val run = Runnable { task() }; handler.postDelayed(run, delay)
                val cancel: () -> Unit = { handler.removeCallbacks(run) }
                cancel
            }, { dispatch(id, it) }, { NativeMedia.releaseInput(id) }, failure)
        }
    }
    @Synchronized fun buffered(id: Long) = id != 0L && id == target && buffer != null
    @Synchronized fun activity(id: Long) { if (id == target) buffer?.activity() }
    @Synchronized private fun submit(id: Long, event: DesktopEvent): Boolean {
        if (id == 0L) return false
        return if (id == target && buffer != null) buffer!!.append(event) else dispatch(id, event)
    }
    private fun dispatch(id: Long, event: DesktopEvent): Boolean = when (event) {
        is DesktopEvent.Position -> NativeMedia.mousePosition(id, event.x, event.y, event.width, event.height)
        is DesktopEvent.Move -> pieces(event.x, event.y) { x, y -> NativeMedia.mouseMove(id, x, y) }
        is DesktopEvent.Button -> NativeMedia.mouseButton(id, event.button, event.down)
        is DesktopEvent.Scroll -> pieces(event.x, event.y) { x, y -> NativeMedia.scroll(id, x, y) }
        is DesktopEvent.Key -> NativeMedia.key(id, event.key, event.down, event.modifiers)
        is DesktopEvent.Text -> NativeMedia.text(id, event.text)
    }
    private fun pieces(horizontal: Int, vertical: Int, send: (Int, Int) -> Boolean): Boolean {
        var x = horizontal; var y = vertical
        while (x != 0 || y != 0) {
            val dx = x.coerceIn(-32767, 32767); val dy = y.coerceIn(-32767, 32767)
            if (!send(dx, dy)) return false
            x -= dx; y -= dy
        }
        return true
    }
    @Synchronized fun releaseInput(id: Long) {
        if (id == target && buffer != null) buffer!!.cancel() else if (id != 0L) NativeMedia.releaseInput(id)
    }
    fun mousePosition(id: Long, x: Int, y: Int, width: Int, height: Int) = submit(id, DesktopEvent.Position(x, y, width, height))
    fun mouseMove(id: Long, x: Int, y: Int) = submit(id, DesktopEvent.Move(x, y))
    fun mouseButton(id: Long, button: Int, down: Boolean) = submit(id, DesktopEvent.Button(button, down))
    fun scroll(id: Long, x: Int, y: Int) = submit(id, DesktopEvent.Scroll(x, y))
    fun key(id: Long, key: Int, down: Boolean, modifiers: Int) = submit(id, DesktopEvent.Key(key, down, modifiers))
    fun text(id: Long, text: String) = submit(id, DesktopEvent.Text(text))
}
