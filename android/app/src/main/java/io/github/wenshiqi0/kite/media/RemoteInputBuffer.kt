package io.github.wenshiqi0.kite.media

/** Pointer sampling has a fixed frame deadline; text has a fixed 200ms deadline. */
sealed interface DesktopEvent {
    data class Position(val x: Int, val y: Int, val width: Int, val height: Int) : DesktopEvent
    data class Move(val x: Int, val y: Int) : DesktopEvent
    data class Button(val button: Int, val down: Boolean) : DesktopEvent
    data class Scroll(val x: Int, val y: Int) : DesktopEvent
    data class Key(val key: Int, val down: Boolean, val modifiers: Int) : DesktopEvent
    data class Text(val text: String) : DesktopEvent
}

class RemoteInputBuffer(
    private val schedule: (Long, () -> Unit) -> (() -> Unit),
    private val send: (DesktopEvent) -> Boolean,
    private val release: () -> Unit,
    private val failure: () -> Unit,
    private val now: () -> Long = { System.nanoTime() / 1_000_000 },
) {
    private val pending = mutableListOf<DesktopEvent>()
    private var cancelTask: (() -> Unit)? = null
    private var generation = 0L
    private var due: Long? = null

    @Synchronized fun append(event: DesktopEvent): Boolean {
        val last = pending.lastOrNull()
        if (event is DesktopEvent.Position && event == last) return true
        val merged = when {
            event is DesktopEvent.Position && last is DesktopEvent.Position -> event
            event is DesktopEvent.Move && last is DesktopEvent.Move -> DesktopEvent.Move(last.x + event.x, last.y + event.y)
            event is DesktopEvent.Scroll && last is DesktopEvent.Scroll -> DesktopEvent.Scroll(last.x + event.x, last.y + event.y)
            event is DesktopEvent.Scroll && last is DesktopEvent.Position && pending.size >= 3
                && pending[pending.lastIndex - 1] is DesktopEvent.Scroll && pending[pending.lastIndex - 2] == last -> {
                    pending.removeAt(pending.lastIndex)
                    val scroll = pending.last() as DesktopEvent.Scroll
                    DesktopEvent.Scroll(scroll.x + event.x, scroll.y + event.y)
                }
            event is DesktopEvent.Text && last is DesktopEvent.Text && last.text.toByteArray().size + event.text.toByteArray().size <= 4096 -> DesktopEvent.Text(last.text + event.text)
            else -> null
        }
        if (merged != null) pending[pending.lastIndex] = merged else pending.add(event)
        if (pending.size > 512 || pending.sumOf { if (it is DesktopEvent.Text) it.text.toByteArray().size else 24 } > 256 * 1024) {
            cancel(); failure(); return false
        }
        if (event is DesktopEvent.Button || event is DesktopEvent.Key) return flush()
        arm(if (event is DesktopEvent.Text) 200 else 16)
        return true
    }

    private fun arm(delay: Long) {
        val deadline = now() + delay
        if (due?.let { it <= deadline } == true) return
        cancelTask?.invoke()
        due = deadline
        val token = ++generation
        cancelTask = schedule(delay) { synchronized(this) { if (token == generation) flush() } }
    }

    @Synchronized fun flush(): Boolean {
        cancelTask?.invoke(); cancelTask = null; due = null; generation++
        val events = pending.toList(); pending.clear()
        for (event in events) if (!send(event)) { release(); failure(); return false }
        return true
    }

    @Synchronized fun cancel() {
        cancelTask?.invoke(); cancelTask = null; due = null; generation++; pending.clear(); release()
    }
}
