package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostSetupScreen(vm:HostCheckViewModel,onBack:()->Unit){
    if(vm.hostEditorVisible){HostEditorScreen(vm){vm.hostEditorVisible=false};return}
    val state by vm.engine.state.collectAsState()
    val platformError by vm.engine.platformError.collectAsState()
    var keyVisible by remember{mutableStateOf(false)}
    Scaffold(topBar={TopAppBar(title={Text("主机")},navigationIcon={IconButton(onClick=onBack,modifier=Modifier.semantics{contentDescription="返回工作区"}){ProductIcon(Mark.Back)}},actions={
        TextButton(onClick=vm::newHost,enabled=!vm.savingHost&&state?.connection!=ConnectionStatus.CONNECTING){ProductIcon(Mark.Plus,modifier=Modifier.size(18.dp));Spacer(Modifier.width(4.dp));Text(stringResource(R.string.add_host))}
    })}){padding->
        Column(Modifier.padding(padding).fillMaxSize()){
            LazyColumn(Modifier.weight(1f).fillMaxWidth(),contentPadding=PaddingValues(16.dp),verticalArrangement=Arrangement.spacedBy(12.dp)){
                items(state?.hosts.orEmpty(),key={it.id},contentType={"host"}){host->
                    val connected=state?.selectedHost==host.id&&state?.connection==ConnectionStatus.CONNECTED
                    val connecting=state?.selectedHost==host.id&&state?.connection==ConnectionStatus.CONNECTING
                    Card(onClick={vm.chooseHost(host)},enabled=!vm.savingHost&&!connecting,modifier=Modifier.fillMaxWidth().semantics{contentDescription="编辑主机 "+host.host},colors=CardDefaults.cardColors(containerColor=if(connected)KiteColors.AccentSurface else KiteColors.Surface)){
                        Column(Modifier.padding(16.dp)){
                            Row(verticalAlignment=Alignment.CenterVertically){
                                ProductIcon(Mark.Monitor,modifier=Modifier.size(24.dp))
                                Column(Modifier.weight(1f).padding(start=12.dp)){
                                    Text(host.host,fontSize=16.sp,maxLines=1,overflow=TextOverflow.Ellipsis)
                                    Text(host.username+" · SSH "+host.port,fontSize=12.sp,color=KiteColors.Muted)
                                }
                                ProductIcon(Mark.Settings,modifier=Modifier.size(18.dp))
                            }
                            Row(Modifier.fillMaxWidth().padding(top=8.dp),verticalAlignment=Alignment.CenterVertically){
                                Text(if(connected)"已连接"else if(connecting)"连接中…"else"已保存",fontSize=12.sp,color=KiteColors.Muted,modifier=Modifier.weight(1f))
                                TextButton(onClick={if(connected)onBack()else vm.connectSaved(host)},enabled=!connecting&&!vm.savingHost){Text(if(connected)"进入工作区"else"连接",fontSize=13.sp)}
                            }
                        }
                    }
                }
                if(state?.hosts.isNullOrEmpty())item{
                    Column(Modifier.fillMaxWidth().padding(vertical=40.dp),horizontalAlignment=Alignment.CenterHorizontally){
                        ProductIcon(Mark.Monitor,modifier=Modifier.size(32.dp));Spacer(Modifier.height(12.dp));Text("添加一台主机，连接你的 agent 与桌面。")
                        TextButton(onClick=vm::newHost){Text(stringResource(R.string.add_host))}
                    }
                }
                state?.failure?.let{failure->item{FailureCard(failure,if(failure.kind==FailureKind.HOST_KEY_CHANGED)({state?.selectedHost?.let(vm::forget)})else null)}}
                vm.localError?.let{error->item{Text(error,color=MaterialTheme.colorScheme.error)}}
                platformError?.let{error->item{DiagnosticCard(error,vm::clearLocalError)}}
            }
            Column(Modifier.fillMaxWidth().padding(horizontal=16.dp)){
                ClipboardSyncSetting(vm)
                Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically){
                    TextButton(onClick={keyVisible=true}){Text(stringResource(R.string.device_key_title))}
                    Spacer(Modifier.weight(1f));state?.let{CopyDiagnosticsButton(it)}
                }
            }
        }
    }
    if(keyVisible)state?.deviceKey?.let{key->AlertDialog(onDismissRequest={keyVisible=false},title={Text(stringResource(R.string.device_key_title))},text={DeviceKeyCard(key)},confirmButton={TextButton(onClick={keyVisible=false}){Text(stringResource(R.string.dismiss))}})}
    state?.pendingHostKey?.let{pending->AlertDialog(onDismissRequest=vm::disconnect,title={Text(stringResource(R.string.host_key_title))},text={Column{Text(pending.address);Text(pending.fingerprint,fontFamily=FontFamily.Monospace);Text(stringResource(R.string.host_key_hint))}},confirmButton={TextButton(onClick={vm.trust(pending.hostId,pending.fingerprint)}){Text(stringResource(R.string.host_key_trust))}},dismissButton={TextButton(onClick=vm::disconnect){Text(stringResource(R.string.cancel))}})}
}
