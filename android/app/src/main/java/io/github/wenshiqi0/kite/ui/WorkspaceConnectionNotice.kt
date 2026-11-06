package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.CoreSnapshot
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Retry details belong in diagnostics; changing error lengths cannot resize chat. */
@Composable
internal fun WorkspaceConnectionNotice(state: CoreSnapshot, diagnostics: () -> Unit, settings: () -> Unit, disconnect: () -> Unit) {
    if(state.connection==ConnectionStatus.CONNECTED) return
    val retrying=state.connection in listOf(ConnectionStatus.CONNECTING,ConnectionStatus.RECONNECTING)
    Row(Modifier.fillMaxWidth().height(48.dp).padding(horizontal=12.dp),verticalAlignment=Alignment.CenterVertically) {
        if(retrying) {
            CircularProgressIndicator(Modifier.size(14.dp),strokeWidth=1.5.dp,color=KiteColors.Muted)
            Spacer(Modifier.width(8.dp))
        }
        Text(connectionStepLabel(state),color=KiteColors.Muted,fontSize=12.sp,maxLines=1,overflow=TextOverflow.Ellipsis,modifier=Modifier.weight(1f))
        TextButton(onClick=diagnostics){Text(stringResource(R.string.details))}
        TextButton(onClick=if(retrying)disconnect else settings){Text(stringResource(if(retrying)R.string.disconnect else R.string.connection_settings))}
    }
}
