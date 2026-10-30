package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.media.NativeMedia
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Quiet right-hand controls matching the fullscreen mouse reference. */
@Composable
fun FullscreenRail(
    vm:HostCheckViewModel,height:Dp,keyboard:()->Unit,chat:()->Unit,rotate:()->Unit,
    help:()->Unit,pip:()->Unit,modifier:Modifier=Modifier,
) {
    val state by vm.engine.state.collectAsState()
    val operating=state?.desktop?.takeover==true
    val enabled=state?.desktop?.status==DesktopStatus.STREAMING
    var menu by remember{mutableStateOf(false)}
    Box(modifier.width(64.dp).height(height).background(KiteColors.Background)){
        Column(Modifier.align(Alignment.Center).heightIn(max=height).verticalScroll(rememberScrollState()).padding(vertical=8.dp),verticalArrangement=Arrangement.spacedBy(8.dp),horizontalAlignment=Alignment.CenterHorizontally){
            Box{
                RailAction(Mark.Settings,if(operating)"操作" else "只看",true){menu=true}
                DropdownMenu(menu,{menu=false}){
                    DropdownMenuItem(text={Text("只看")},leadingIcon={ProductIcon(Mark.Eye)},onClick={menu=false;vm.takeOver(false)})
                    DropdownMenuItem(text={Text("操作")},leadingIcon={ProductIcon(Mark.Pointer)},enabled=enabled,onClick={menu=false;vm.takeOver(true)})
                    state?.selectedAgent?.let { id ->
                        DropdownMenuItem(text={Text(if(state?.agentPause==io.github.wenshiqi0.kite.core.AgentPauseStatus.PAUSED)"继续 agent" else "暂停 agent")},onClick={menu=false;vm.pauseAgent(id)})
                    }
                    HorizontalDivider()
                    Row(Modifier.padding(horizontal=12.dp),verticalAlignment=Alignment.CenterVertically){ZoomControl(vm)}
                    DropdownMenuItem(text={Text("旋转屏幕")},leadingIcon={ProductIcon(Mark.Rotate)},onClick={menu=false;rotate()})
                    DropdownMenuItem(text={Text("手势说明")},onClick={menu=false;help()})
                    DropdownMenuItem(text={Text("画中画")},onClick={menu=false;pip()})
                    DropdownMenuItem(text={Text("关闭桌面")},onClick={menu=false;vm.closeDesktop()})
                }
            }
            RailAction(Mark.Keyboard,"键盘",operating,keyboard)
            RailAction(Mark.Monitor,"显示桌面",operating){vm.desktopChord(91,68,8)}
            RailAction(Mark.Windows,"切换窗口",operating){vm.desktopChord(164,9,4)}
            RailAction(Mark.Chat,"聊天",true,chat)
        }
    }
}
@Composable
private fun RailAction(mark:Mark,label:String,enabled:Boolean,action:()->Unit) {
    Column(Modifier.size(56.dp,52.dp).clickable(enabled=enabled,onClick=action).semantics{contentDescription=label},
        horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.Center){
        val color=if(enabled)KiteColors.Text else KiteColors.Muted.copy(alpha=.4f)
        ProductIcon(mark,color,Modifier.size(21.dp));Spacer(Modifier.height(4.dp));Text(label,color=color,fontSize=10.sp)
    }
}
@Composable
fun RemoteKeyStrip(vm:HostCheckViewModel,modifier:Modifier=Modifier) {
    Row(modifier.background(KiteColors.Background.copy(alpha=.94f)).padding(horizontal=6.dp),horizontalArrangement=Arrangement.spacedBy(4.dp)){
        @Composable fun Key(label:String,active:Boolean=false,action:()->Unit){
            TextButton(onClick=action,modifier=Modifier.height(48.dp).widthIn(min=48.dp),colors=ButtonDefaults.textButtonColors(contentColor=if(active)KiteColors.Accent else KiteColors.Text)){Text(label,fontSize=12.sp)}
        }
        Key("Esc"){vm.shortcut(27)}
        Key("Tab"){vm.shortcut(9)}
        Key("Ctrl",vm.ctrlHeld){vm.modifier(true)}
        Key("Meta",vm.metaHeld){vm.modifier(false)}
        Key("粘贴"){if(!vm.pasteSynchronized())vm.shortcut(86,true)}
        Key("↵"){vm.shortcut(13)}
    }
}
