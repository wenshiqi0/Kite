package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInParent
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Mouse-shaped fullscreen control. Its lower grip moves the control, never a trackpad. */
@Composable
fun VirtualMouse(vm:HostCheckViewModel,width:Dp,height:Dp,safeTop:Dp,safeBottom:Dp,enabled:Boolean,bounds:(List<Rect>)->Unit) {
    val label=stringResource(R.string.virtual_mouse)
    val density=LocalDensity.current
    var expanded by rememberSaveable{mutableStateOf(false)}
    var x by rememberSaveable{mutableFloatStateOf(.82f)}
    var y by rememberSaveable{mutableFloatStateOf(.68f)}
    val full=expanded&&width>=192.dp&&height-safeTop-safeBottom>=160.dp
    val w=if(full)176.dp else 48.dp
    val h=if(full)144.dp else 48.dp
    val closeLeft=x>.5f
    val bodyX=if(full&&closeLeft)48.dp else 0.dp
    val closeX=if(closeLeft)0.dp else 128.dp
    val rangeX=(width-w-16.dp).coerceAtLeast(0.dp)
    val rangeY=(height-safeTop-safeBottom-h-16.dp).coerceAtLeast(0.dp)
    val drag=Modifier.pointerInput(vm.target,rangeX,rangeY){
        detectDragGestures{change,amount->
            change.consume()
            x=(x+amount.x/with(density){rangeX.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
            y=(y+amount.y/with(density){rangeY.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
        }
    }
    DisposableEffect(vm.target){onDispose{vm.releaseMouseButtons();bounds(emptyList())}}
    Box(Modifier.absoluteOffset{IntOffset(with(density){(8.dp+rangeX*x).roundToPx()},with(density){(safeTop+8.dp+rangeY*y).roundToPx()})}.size(w,h).onGloballyPositioned{
        val b=it.boundsInParent()
        if(full){
            val bx=with(density){bodyX.toPx()};val cx=with(density){closeX.toPx()}
            bounds(listOf(Rect(b.left+bx,b.top,b.left+bx+with(density){128.dp.toPx()},b.bottom),Rect(b.left+cx,b.top,b.left+cx+with(density){48.dp.toPx()},b.top+with(density){48.dp.toPx()})))
        }else bounds(listOf(b))
    }.semantics{contentDescription=label}){
        if(!full){
            Box(Modifier.size(48.dp).then(drag).semantics{contentDescription="展开鼠标";onClick{expanded=true;true}}.pointerInput(vm.target){detectTapGestures(onTap={expanded=true})},contentAlignment=Alignment.Center){
                Box(Modifier.size(28.dp).background(KiteColors.Background,CircleShape).border(1.dp,KiteColors.Line,CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Mouse,KiteColors.Text,Modifier.size(16.dp))}
            }
        }else{
            val shape=RoundedCornerShape(topStart=48.dp,topEnd=48.dp,bottomStart=36.dp,bottomEnd=36.dp)
            Column(Modifier.absoluteOffset(bodyX,0.dp).size(128.dp,144.dp).clip(shape).background(KiteColors.Mouse).border(1.dp,KiteColors.MouseLine,shape)){
                Row(Modifier.fillMaxWidth().height(76.dp)){
                    FullMouseButton(vm,1,enabled,Modifier.width(44.dp).fillMaxHeight())
                    Box(Modifier.width(40.dp).fillMaxHeight().border(.5.dp,KiteColors.MouseLine),contentAlignment=Alignment.Center){
                        MouseWheel(vm,enabled,Modifier.fillMaxSize(),showIcon=false)
                        Box(Modifier.width(24.dp).height(44.dp).border(1.dp,KiteColors.MouseLine,RoundedCornerShape(10.dp)),contentAlignment=Alignment.Center){ProductIcon(Mark.Wheel,KiteColors.Text,Modifier.size(16.dp))}
                    }
                    FullMouseButton(vm,3,enabled,Modifier.width(44.dp).fillMaxHeight())
                }
                HorizontalDivider(color=KiteColors.MouseLine,thickness=1.dp)
                Box(Modifier.fillMaxWidth().weight(1f).then(drag).semantics{contentDescription="拖动鼠标位置"},contentAlignment=Alignment.Center){
                    Column(verticalArrangement=Arrangement.spacedBy(4.dp)){
                        repeat(3){Box(Modifier.size(18.dp,2.dp).background(KiteColors.MouseLine,RoundedCornerShape(1.dp)))}
                    }
                }
            }
            IconButton(onClick={vm.releaseMouseButtons();expanded=false},modifier=Modifier.absoluteOffset(closeX,0.dp).size(48.dp).semantics{contentDescription="收起鼠标"}){
                Box(Modifier.size(28.dp).background(KiteColors.Background,CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Close,KiteColors.Text,Modifier.size(16.dp))}
            }
        }
    }
}
@Composable
private fun FullMouseButton(vm:HostCheckViewModel,button:Int,enabled:Boolean,modifier:Modifier) {
    val label=stringResource(if(button==1)R.string.mouse_left else R.string.mouse_right)
    val down=vm.heldMouseButtons and (1 shl button)!=0
    Box(modifier.background(if(down)KiteColors.AccentSurface else KiteColors.Mouse).semantics{
        contentDescription=label;onClick{if(enabled){if(button==1)vm.leftClick()else vm.rightClick()};enabled}
    }.pointerInput(vm.target,button,enabled){
        if(enabled)detectTapGestures(onPress={vm.mouseButton(button,true);try{tryAwaitRelease()}finally{vm.mouseButton(button,false)}})
    })
}
