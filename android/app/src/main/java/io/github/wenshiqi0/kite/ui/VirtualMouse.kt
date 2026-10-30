package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
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
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
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

/** Fullscreen control: workspace coordinates, small visual, explicit hit regions. */
@Composable
fun VirtualMouse(vm:HostCheckViewModel,width:Dp,height:Dp,safeTop:Dp,safeBottom:Dp,enabled:Boolean,bounds:(List<Rect>)->Unit) {
    val density=LocalDensity.current
    var expanded by rememberSaveable{mutableStateOf(false)}
    var x by rememberSaveable{mutableFloatStateOf(.88f)}
    var y by rememberSaveable{mutableFloatStateOf(.78f)}
    val full=expanded&&width>=208.dp&&height-safeTop-safeBottom>=128.dp
    val w=if(full)192.dp else 48.dp
    val h=if(full)112.dp else 48.dp
    val closeLeft=x>.5f
    val bodyX=if(full&&closeLeft)48.dp else 0.dp
    val closeX=if(closeLeft)0.dp else 144.dp
    val rangeX=(width-w-16.dp).coerceAtLeast(0.dp)
    val rangeY=(height-safeTop-safeBottom-h-16.dp).coerceAtLeast(0.dp)
    val label=stringResource(R.string.virtual_mouse)
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
            bounds(listOf(Rect(b.left+bx,b.top,b.left+bx+with(density){144.dp.toPx()},b.bottom),Rect(b.left+cx,b.top,b.left+cx+with(density){48.dp.toPx()},b.top+with(density){48.dp.toPx()})))
        }else bounds(listOf(b))
    }.semantics{contentDescription=label}){
        if(!full){
            Box(Modifier.size(48.dp).then(drag).semantics{contentDescription="展开鼠标";onClick{expanded=true;true}}.pointerInput(vm.target){detectTapGestures(onTap={expanded=true})},contentAlignment=Alignment.Center){
                Box(Modifier.size(32.dp).background(Color(0xFF202B32),CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Mouse,Color.White,Modifier.size(18.dp))}
            }
        }else{
            Surface(Modifier.absoluteOffset(bodyX,0.dp).size(144.dp,112.dp),color=Color(0xFFEDF1E9),shape=RoundedCornerShape(topStart=40.dp,topEnd=40.dp,bottomStart=28.dp,bottomEnd=28.dp),shadowElevation=2.dp){
                Column{
                    Row(Modifier.fillMaxWidth().height(48.dp)){
                        RemoteMouseButton(vm,1,enabled,Modifier.width(48.dp),compact=true)
                        MouseWheel(vm,enabled,Modifier.width(48.dp).height(48.dp))
                        RemoteMouseButton(vm,3,enabled,Modifier.width(48.dp),compact=true)
                    }
                    HorizontalDivider(color=Color(0xFFD4DCD0))
                    Box(Modifier.fillMaxWidth().weight(1f)){
                        RelativeMouseArea(vm,enabled,Modifier.fillMaxSize())
                        ProductIcon(Mark.Pointer,Color(0xFF899585),Modifier.align(Alignment.CenterStart).padding(start=28.dp).size(18.dp))
                        Box(Modifier.align(Alignment.CenterEnd).size(48.dp).then(drag).semantics{contentDescription="拖动鼠标位置"},contentAlignment=Alignment.Center){ProductIcon(Mark.Pan,modifier=Modifier.size(18.dp))}
                    }
                }
            }
            IconButton(onClick={vm.releaseMouseButtons();expanded=false},modifier=Modifier.absoluteOffset(closeX,0.dp).size(48.dp).semantics{contentDescription="收起鼠标"}){
                Box(Modifier.size(32.dp).background(Color(0xFF202B32),CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Close,Color.White,Modifier.size(18.dp))}
            }
        }
    }
}
