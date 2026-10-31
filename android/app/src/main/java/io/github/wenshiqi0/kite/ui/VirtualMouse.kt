package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
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
import androidx.compose.ui.geometry.Offset
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
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** The fullscreen mouse moves an explicit source-pixel hotspot, never a trackpad. */
@Composable
fun VirtualMouse(vm:HostCheckViewModel,surface:VideoSurface?,width:Dp,height:Dp,safeTop:Dp,safeBottom:Dp,enabled:Boolean,bounds:(List<Rect>)->Unit) {
    val label=stringResource(R.string.virtual_mouse)
    val expandLabel=stringResource(R.string.expand_virtual_mouse)
    val collapseLabel=stringResource(R.string.collapse_virtual_mouse)
    val moveLabel=stringResource(R.string.move_virtual_mouse)
    val density=LocalDensity.current
    var expanded by rememberSaveable{mutableStateOf(false)}
    var x by rememberSaveable{mutableFloatStateOf(.82f)}
    var y by rememberSaveable{mutableFloatStateOf(.68f)}
    val full=expanded&&width>=192.dp&&height-safeTop-safeBottom>=160.dp
    val w=if(full)176.dp else 48.dp
    val h=if(full)144.dp else 48.dp
    // Reserve the close target on the right, leaving the arrow clear of the body.
    val bodyX=0.dp
    val closeX=128.dp
    val rangeX=(width-w-16.dp).coerceAtLeast(0.dp)
    val rangeY=(height-safeTop-safeBottom-h-16.dp).coerceAtLeast(0.dp)
    val move:(Offset)->Unit={amount->
        x=(x+amount.x/with(density){rangeX.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
        y=(y+amount.y/with(density){rangeY.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
        // The control can stop at an edge; repeated swipes still reach every source pixel.
        if(full&&enabled)surface?.moveVirtualMouse(amount.x,amount.y)
    }
    val drag=Modifier.pointerInput(vm.target,rangeX,rangeY,full,enabled){
        detectDragGestures{change,amount->
            change.consume()
            move(amount)
        }
    }
    DisposableEffect(full,vm.target,surface){
        if(full)with(density){surface?.openVirtualMouse((8.dp+rangeX*x+bodyX-24.dp).toPx(),(safeTop+8.dp+rangeY*y).toPx())}
        onDispose{vm.releaseMouseButtons();surface?.closeVirtualMouse()}
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
            Box(Modifier.size(48.dp).then(drag).semantics{contentDescription=expandLabel;onClick{expanded=true;true}}.pointerInput(vm.target){detectTapGestures(onTap={expanded=true})},contentAlignment=Alignment.Center){
                Box(Modifier.size(28.dp).background(KiteColors.Background,CircleShape).border(1.dp,KiteColors.Line,CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Mouse,KiteColors.Text,Modifier.size(16.dp))}
            }
        }else{
            val shape=RoundedCornerShape(topStart=48.dp,topEnd=48.dp,bottomStart=36.dp,bottomEnd=36.dp)
            Column(Modifier.absoluteOffset(bodyX,0.dp).size(128.dp,144.dp).clip(shape).background(KiteColors.Mouse).border(1.dp,KiteColors.MouseLine,shape)){
                Row(Modifier.fillMaxWidth().height(76.dp)){
                    FullMouseButton(vm,surface,1,enabled,move,Modifier.width(44.dp).fillMaxHeight())
                    Box(Modifier.width(40.dp).fillMaxHeight().border(.5.dp,KiteColors.MouseLine),contentAlignment=Alignment.Center){
                        MouseWheel(vm,enabled,Modifier.fillMaxSize(),showIcon=false,beforeScroll={surface?.prepareVirtualMouse()==true})
                        Box(Modifier.width(24.dp).height(44.dp).border(1.dp,KiteColors.MouseLine,RoundedCornerShape(10.dp)),contentAlignment=Alignment.Center){ProductIcon(Mark.Wheel,KiteColors.Text,Modifier.size(16.dp))}
                    }
                    FullMouseButton(vm,surface,3,enabled,move,Modifier.width(44.dp).fillMaxHeight())
                }
                HorizontalDivider(color=KiteColors.MouseLine,thickness=1.dp)
                Box(Modifier.fillMaxWidth().weight(1f).then(drag).semantics{contentDescription=moveLabel},contentAlignment=Alignment.Center){
                    Column(verticalArrangement=Arrangement.spacedBy(4.dp)){
                        repeat(3){Box(Modifier.size(18.dp,2.dp).background(KiteColors.MouseLine,RoundedCornerShape(1.dp)))}
                    }
                }
            }
            IconButton(onClick={vm.releaseMouseButtons();expanded=false},modifier=Modifier.absoluteOffset(closeX,0.dp).size(48.dp).semantics{contentDescription=collapseLabel}){
                Box(Modifier.size(28.dp).background(KiteColors.Background,CircleShape),contentAlignment=Alignment.Center){ProductIcon(Mark.Close,KiteColors.Text,Modifier.size(16.dp))}
            }
        }
    }
}
@Composable
private fun FullMouseButton(vm:HostCheckViewModel,surface:VideoSurface?,button:Int,enabled:Boolean,move:(Offset)->Unit,modifier:Modifier) {
    val label=stringResource(if(button==1)R.string.mouse_left else R.string.mouse_right)
    val down=vm.heldMouseButtons and (1 shl button)!=0
    val currentMove by rememberUpdatedState(move)
    Box(modifier.background(if(down)KiteColors.AccentSurface else KiteColors.Mouse).semantics{
        contentDescription=label;onClick{if(enabled&&surface?.prepareVirtualMouse()==true){if(button==1)vm.leftClick()else vm.rightClick();true}else false}
    }.pointerInput(vm.target,surface,button,enabled){
        if(enabled)awaitEachGesture{
            val first=awaitFirstDown(requireUnconsumed=false)
            first.consume()
            if(surface?.prepareVirtualMouse()!=true)return@awaitEachGesture
            vm.mouseButton(button,true)
            try{
                var pending=Offset.Zero
                var moved=false
                do{
                    val event=awaitPointerEvent()
                    val point=event.changes.firstOrNull{it.id==first.id}?:break
                    // Compose transforms both positions into the current local space.
                    // A saved prior position would subtract the widget's own movement.
                    val delta=point.position-point.previousPosition;pending+=delta
                    if(point.pressed&&(moved||pending.getDistance()>viewConfiguration.touchSlop)){
                        currentMove(if(moved)delta else pending);moved=true
                    }
                    point.consume()
                }while(point.pressed)
            }finally{vm.mouseButton(button,false)}
        }
    })
}
