package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp

enum class Mark { Menu, Monitor, Eye, Expand, Shrink, Split, Dock, Keyboard, Mic, Plus, Send, Close, Touchpad, Clipboard, Pan, More, Back, Terminal, Settings, Chat }

@Composable
fun ProductIcon(mark: Mark, color: Color = Color(0xFF64645F), modifier: Modifier = Modifier) {
    Canvas(modifier.size(22.dp)) {
        val unit = size.width / 24f
        val stroke = Stroke(1.7f * unit, cap = StrokeCap.Round)
        fun line(x: Float, y: Float, xx: Float, yy: Float) = drawLine(color, Offset(x * unit, y * unit), Offset(xx * unit, yy * unit), stroke.width, StrokeCap.Round)
        fun box(x: Float, y: Float, w: Float, h: Float, r: Float = 2f) = drawRoundRect(color, Offset(x * unit, y * unit), Size(w * unit, h * unit), CornerRadius(r * unit), style = stroke)
        when (mark) {
            Mark.Menu -> { line(4f, 6f, 20f, 6f); line(4f, 12f, 20f, 12f); line(4f, 18f, 20f, 18f) }
            Mark.Monitor -> { box(3f, 4f, 18f, 13f); line(12f, 17f, 12f, 21f); line(8f, 21f, 16f, 21f) }
            Mark.Terminal -> { box(3f, 4f, 18f, 16f); line(7f, 8f, 10f, 11f); line(10f, 11f, 7f, 14f); line(12f, 15f, 17f, 15f) }
            Mark.Eye -> {
                val p = Path().apply { moveTo(2f * unit, 12f * unit); cubicTo(7f * unit, 4f * unit, 17f * unit, 4f * unit, 22f * unit, 12f * unit); cubicTo(17f * unit, 20f * unit, 7f * unit, 20f * unit, 2f * unit, 12f * unit) }
                drawPath(p, color, style = stroke); drawCircle(color, 2.4f * unit, Offset(12f * unit, 12f * unit), style = stroke)
            }
            Mark.Expand, Mark.Shrink -> {
                val a = if (mark == Mark.Expand) 4f else 9f; val b = if (mark == Mark.Expand) 9f else 4f
                line(a, a, b, a); line(a, a, a, b); line(24-a, a, 24-b, a); line(24-a, a, 24-a, b)
                line(a, 24-a, b, 24-a); line(a, 24-a, a, 24-b); line(24-a, 24-a, 24-b, 24-a); line(24-a, 24-a, 24-a, 24-b)
            }
            Mark.Split -> { box(3f, 3f, 18f, 18f); line(3f, 11f, 21f, 11f) }
            Mark.Dock -> { box(3f, 4f, 18f, 16f); box(12f, 12f, 8f, 7f, 1f) }
            Mark.Keyboard -> { box(2f, 5f, 20f, 14f); for (y in listOf(9f, 12f)) for (x in listOf(6f, 10f, 14f, 18f)) drawCircle(color, .65f * unit, Offset(x * unit, y * unit)); line(7f, 16f, 17f, 16f) }
            Mark.Mic -> { box(9f, 2f, 6f, 13f, 3f); line(6f, 10f, 6f, 13f); line(18f, 10f, 18f, 13f); line(6f, 13f, 9f, 18f); line(18f, 13f, 15f, 18f); line(9f, 18f, 15f, 18f); line(12f, 18f, 12f, 22f) }
            Mark.Plus -> { line(12f, 4f, 12f, 20f); line(4f, 12f, 20f, 12f) }
            Mark.Close -> { line(6f, 6f, 18f, 18f); line(18f, 6f, 6f, 18f) }
            Mark.Send -> { line(4f, 5f, 21f, 12f); line(21f, 12f, 4f, 19f); line(4f, 19f, 7f, 12f); line(7f, 12f, 4f, 5f); line(7f, 12f, 15f, 12f) }
            Mark.Touchpad -> { box(3f, 4f, 18f, 16f); line(3f, 15f, 21f, 15f); line(12f, 15f, 12f, 20f); line(10f, 8f, 14f, 8f) }
            Mark.Clipboard -> { box(6f, 5f, 13f, 17f); box(9f, 2f, 7f, 5f, 1f) }
            Mark.Pan -> { line(12f, 2f, 12f, 22f); line(2f, 12f, 22f, 12f); line(12f, 2f, 9f, 5f); line(12f, 2f, 15f, 5f); line(2f, 12f, 5f, 9f); line(22f, 12f, 19f, 9f); line(12f, 22f, 9f, 19f) }
            Mark.More -> for (y in listOf(5f, 12f, 19f)) drawCircle(color, 1.6f * unit, Offset(12f * unit, y * unit))
            Mark.Back -> { line(15f, 4f, 7f, 12f); line(7f, 12f, 15f, 20f) }
            Mark.Settings -> { drawCircle(color, 7f * unit, center, style = stroke); drawCircle(color, 2.3f * unit, center, style = stroke); line(12f, 1f, 12f, 4f); line(12f, 20f, 12f, 23f); line(1f, 12f, 4f, 12f); line(20f, 12f, 23f, 12f) }
            Mark.Chat -> { box(3f, 4f, 18f, 13f, 3f); line(7f, 17f, 6f, 21f); line(6f, 21f, 12f, 17f); line(7f, 9f, 17f, 9f); line(7f, 13f, 14f, 13f) }
        }
    }
}
