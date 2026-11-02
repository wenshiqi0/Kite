package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable
fun ComposerActionButton(
    working: Boolean,
    hasInput: Boolean,
    canSend: Boolean,
    canInterrupt: Boolean,
    sending: Boolean,
    interrupting: Boolean,
    onSend: () -> Unit,
    onInterrupt: () -> Unit,
) {
    val interrupt = !hasInput && (working || interrupting)
    val busy = if (interrupt) interrupting else sending
    val enabled = !busy && if (interrupt) canInterrupt else hasInput && canSend
    val description = stringResource(if (interrupt) R.string.interrupt_agent else R.string.send_prompt)
    IconButton(
        onClick = if (interrupt) onInterrupt else onSend,
        enabled = enabled,
        modifier = Modifier.semantics { contentDescription = description },
    ) {
        if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        else ProductIcon(if (interrupt) Mark.Stop else Mark.Send, if (enabled) KiteColors.Accent else KiteColors.Muted)
    }
}
