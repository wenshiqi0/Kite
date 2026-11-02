package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.*
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ResponsePhase

@Composable
fun ResponseFeedback(phase: ResponsePhase, sending: Boolean, waitingForApproval: Boolean = false, details: String = "") {
    val current = if (sending && phase == ResponsePhase.IDLE) ResponsePhase.SENDING else phase
    val failure = responseFailureDetails(current, details)
    val capacity = failure?.let(::responseIsModelCapacity) == true
    val label = when (current) { ResponsePhase.SENDING -> R.string.response_sending; ResponsePhase.THINKING -> R.string.response_thinking; ResponsePhase.TOOL -> R.string.response_tools; ResponsePhase.RESPONDING -> R.string.response_replying; ResponsePhase.FAILED -> R.string.response_failed; ResponsePhase.WAITING -> if (waitingForApproval) R.string.response_waiting_approval else R.string.response_waiting_answer; else -> return }
    Column(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (current != ResponsePhase.FAILED && current != ResponsePhase.WAITING) {
                val transition = rememberInfiniteTransition(label = "response feedback")
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    repeat(3) { index ->
                        val alpha by transition.animateFloat(initialValue = .25f, targetValue = 1f, animationSpec = infiniteRepeatable(tween(420), RepeatMode.Reverse, StartOffset(index * 140)), label = "response dot $index")
                        Box(Modifier.size(5.dp).alpha(alpha).background(KiteColors.Accent, CircleShape))
                    }
                }
                Spacer(Modifier.width(10.dp))
            }
            Text(stringResource(if (capacity) R.string.response_model_capacity else label), color = if (current == ResponsePhase.WAITING || current == ResponsePhase.FAILED) KiteColors.Warning else KiteColors.Muted, fontSize = 12.sp)
        }
        failure?.let { message -> SelectionContainer { Text(message, color = KiteColors.Warning, fontSize = 12.sp) } }
        if (current == ResponsePhase.FAILED && failure == null) Text(stringResource(R.string.response_failure_no_details), color = KiteColors.Muted, fontSize = 12.sp)
        if (capacity) Text(stringResource(R.string.response_model_capacity_hint), color = KiteColors.Muted, fontSize = 12.sp)
    }
}
