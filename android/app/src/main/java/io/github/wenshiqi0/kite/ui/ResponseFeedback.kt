package io.github.wenshiqi0.kite.ui

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.*
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ResponsePhase

@Composable
fun ResponseFeedback(phase: ResponsePhase, sending: Boolean) {
    val current = if (sending && phase == ResponsePhase.IDLE) ResponsePhase.SENDING else phase
    val label = when (current) { ResponsePhase.SENDING -> R.string.response_sending; ResponsePhase.THINKING -> R.string.response_thinking; ResponsePhase.TOOL -> R.string.response_tools; ResponsePhase.RESPONDING -> R.string.response_replying; ResponsePhase.FAILED -> R.string.response_failed; else -> return }
    Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
        if (current != ResponsePhase.FAILED) {
            val transition = rememberInfiniteTransition(label = "response feedback")
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                repeat(3) { index ->
                    val alpha by transition.animateFloat(initialValue = .25f, targetValue = 1f, animationSpec = infiniteRepeatable(tween(420), RepeatMode.Reverse, StartOffset(index * 140)), label = "response dot $index")
                    Box(Modifier.size(5.dp).alpha(alpha).background(Color(0xFF2D85D8), CircleShape))
                }
            }
            Spacer(Modifier.width(10.dp))
        }
        Text(stringResource(label), color = Color(0xFF888982), fontSize = 12.sp)
    }
}
