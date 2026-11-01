package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AgentInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

enum class SessionStartupPhase {
    INITIALIZING, READY, FAILED, INACTIVE;
    val canSubmit: Boolean get() = this == READY
}

fun sessionStartupPhase(creating: Boolean, connected: Boolean, agent: AgentInfo?, composerReady: Boolean, failed: Boolean): SessionStartupPhase = when {
    creating -> SessionStartupPhase.INITIALIZING
    !connected -> SessionStartupPhase.INACTIVE
    failed -> SessionStartupPhase.FAILED
    agent == null -> SessionStartupPhase.INACTIVE
    agent.launchPending || agent.agent == "codex" && !composerReady -> SessionStartupPhase.INITIALIZING
    else -> SessionStartupPhase.READY
}

@Composable
fun SessionInitializationMessage(modifier: Modifier = Modifier, failed: Boolean = false, retryEnabled: Boolean = true, onRetry: () -> Unit = {}) {
    Column(modifier.padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (!failed) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = KiteColors.Muted)
        Text(stringResource(if(failed)R.string.session_initialization_failed else R.string.session_initializing), color = KiteColors.Muted, fontSize = 14.sp)
        if (failed) TextButton(onClick = onRetry, enabled = retryEnabled) { Text(stringResource(R.string.retry)) }
    }
}
