package io.github.wenshiqi0.kite.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.view.ViewTreeObserver
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.DesktopStatus
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withTimeoutOrNull

@Composable fun ClipboardSyncSetting(vm: HostCheckViewModel) {
    val label=stringResource(R.string.clipboard_sync)
    Row(Modifier.fillMaxWidth().padding(vertical=8.dp),horizontalArrangement=Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f)) {
            Text(label)
            Text(stringResource(R.string.clipboard_sync_description),style=MaterialTheme.typography.bodySmall)
            if(vm.engine.clipboardProblem!=null)Text(stringResource(R.string.clipboard_sync_unavailable),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
        }
        Switch(checked=vm.engine.clipboardSyncEnabled,onCheckedChange=vm.engine::setClipboardSync,modifier=Modifier.semantics { contentDescription=label })
    }
}

/** Platform clipboard/focus adapter. Rust owns direction, deduplication and SSH I/O. */
@Composable fun ClipboardBridge(vm: HostCheckViewModel, blocked: Boolean) {
    val state by vm.engine.state.collectAsState()
    val context=LocalContext.current
    val view=LocalView.current
    val clipboard=remember(context){context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager}
    val events=remember { Channel<Unit>(Channel.CONFLATED) }
    var focused by remember(view){mutableStateOf(view.hasWindowFocus())}
    DisposableEffect(view,clipboard) {
        val focus=ViewTreeObserver.OnWindowFocusChangeListener { focused=it;events.trySend(Unit) }
        val changed=ClipboardManager.OnPrimaryClipChangedListener { events.trySend(Unit) }
        view.viewTreeObserver.addOnWindowFocusChangeListener(focus);clipboard.addPrimaryClipChangedListener(changed)
        onDispose {view.viewTreeObserver.removeOnWindowFocusChangeListener(focus);clipboard.removePrimaryClipChangedListener(changed)}
    }
    val host=state?.selectedHost
    val enabled=vm.engine.clipboardSyncEnabled && focused && !blocked && !vm.inPip && !vm.pipTransition && state?.desktop?.takeover==true && state?.desktop?.status==DesktopStatus.STREAMING
    val label=stringResource(R.string.clipboard_sync)
    fun localText(): String? = clipboard.primaryClip?.takeIf {it.itemCount>0}?.getItemAt(0)?.text?.toString()
    LaunchedEffect(host,enabled) {
        if(!enabled || host==null)return@LaunchedEffect
        var backoff=500L
        while(isActive) {
            val before=localText()
            try {
                val incoming=vm.engine.core.synchronizeClipboard(host,before)
                if(incoming!=null && view.hasWindowFocus() && vm.engine.state.value?.selectedHost==host && vm.engine.state.value?.desktop?.takeover==true && vm.engine.clipboardSyncEnabled && localText()==before) {
                    clipboard.setPrimaryClip(ClipData.newPlainText(label,incoming))
                }
                vm.engine.clipboardProblem=null;backoff=500L
            } catch(e:CancellationException) {throw e}
            catch(e:Exception) {vm.engine.clipboardProblem=e.javaClass.simpleName;withTimeoutOrNull(backoff){events.receive()};backoff=(backoff*2).coerceAtMost(8000);continue}
            withTimeoutOrNull(500){events.receive()}
        }
    }
}
