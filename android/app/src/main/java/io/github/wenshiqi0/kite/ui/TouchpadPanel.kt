package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import kotlin.math.abs

@Composable
fun TouchpadPanel(vm:HostCheckViewModel,height:Dp,enabled:Boolean,close:()->Unit) {
    DisposableEffect(vm.target) { onDispose { vm.releaseMouseButtons() } }
    Column(Modifier.fillMaxWidth().height(height).background(Color(0xFFF0F2EC))) {
        Box(Modifier.fillMaxWidth().weight(1f)) {
            RelativeMouseArea(vm,enabled,Modifier.fillMaxSize())
            Text(stringResource(R.string.trackpad),fontSize=12.sp,color=Color(0xFF737B71),modifier=Modifier.padding(14.dp))
            IconButton(onClick=close,modifier=Modifier.align(Alignment.TopEnd).size(48.dp).semantics{contentDescription="收起触控板"}) {
                ProductIcon(Mark.Chevron,modifier=Modifier.size(20.dp))
            }
        }
        HorizontalDivider(color=Color(0xFFDCE1D7))
        Row(Modifier.fillMaxWidth().height(48.dp).background(Color.White)) {
            RemoteMouseButton(vm,1,enabled,Modifier.weight(1f))
            MouseWheel(vm,enabled,Modifier.width(56.dp).fillMaxHeight())
            RemoteMouseButton(vm,3,enabled,Modifier.weight(1f))
        }
    }
}

@Composable
fun RelativeMouseArea(vm:HostCheckViewModel,enabled:Boolean,modifier:Modifier=Modifier) {
    val density=LocalDensity.current.density
    val label=stringResource(R.string.mouse_move_area)
    Box(modifier.semantics{contentDescription=label}.pointerInput(vm.target,enabled) {
        if(!enabled)return@pointerInput
        awaitEachGesture {
            val first=awaitFirstDown(requireUnconsumed=false)
            var last=first.position
            var pending=Offset.Zero
            var moved=false
            var multiple=false
            val downAt=first.uptimeMillis
            first.consume()
            do {
                val event=awaitPointerEvent()
                val points=event.changes.filter{it.pressed}
                if(points.size>1) {
                    val center=Offset(points.sumOf{it.position.x.toDouble()}.toFloat()/points.size,points.sumOf{it.position.y.toDouble()}.toFloat()/points.size)
                    if(!multiple){last=center;pending=Offset.Zero;multiple=true}
                    val delta=center-last
                    pending+=delta
                    if(moved||abs(pending.y)>viewConfiguration.touchSlop) {
                        vm.wheelMouse(if(moved)delta.y else pending.y,density);moved=true
                    }
                    last=center
                } else if(!multiple) {
                    event.changes.firstOrNull{it.id==first.id}?.let { change ->
                        val delta=change.position-last;pending+=delta
                        if(moved||pending.getDistance()>viewConfiguration.touchSlop) {
                            val move=if(moved)delta else pending
                            vm.moveMouse(move.x,move.y,density);moved=true
                        }
                        last=change.position
                        if(!change.pressed&&!moved&&vm.heldMouseButtons==0)vm.leftClick()
                    }
                }
                event.changes.forEach{it.consume()}
                if(points.isEmpty()&&multiple&&!moved&&event.changes.maxOf{it.uptimeMillis}-downAt<400&&vm.heldMouseButtons==0)vm.rightClick()
            } while(event.changes.any{it.pressed})
        }
    })
}

@Composable
fun RemoteMouseButton(vm:HostCheckViewModel,button:Int,enabled:Boolean,modifier:Modifier=Modifier,compact:Boolean=false) {
    val label=stringResource(if(button==1)R.string.mouse_left else R.string.mouse_right)
    val down=vm.heldMouseButtons and (1 shl button)!=0
    Box(modifier.height(48.dp).background(if(down)Color(0xFFDCEBFC)else Color.White).semantics {
        contentDescription=label;role=Role.Button
        onClick {if(enabled){if(button==1)vm.leftClick()else vm.rightClick()};enabled}
    }.pointerInput(vm.target,button,enabled) {
        if(enabled)detectTapGestures(onPress={vm.mouseButton(button,true);try{tryAwaitRelease()}finally{vm.mouseButton(button,false)}})
    },contentAlignment=Alignment.Center) {
        Row(verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(6.dp)) {
            if(!compact)ProductIcon(Mark.Mouse,modifier=Modifier.size(16.dp))
            Text(if(compact)if(button==1)"左" else "右" else label,fontSize=12.sp,color=Color(0xFF626B60))
        }
    }
}

@Composable
fun MouseWheel(vm:HostCheckViewModel,enabled:Boolean,modifier:Modifier=Modifier) {
    val density=LocalDensity.current.density
    val label=stringResource(R.string.mouse_wheel)
    Box(modifier.background(Color(0xFFE8EDE4)).semantics{contentDescription=label}.pointerInput(vm.target,enabled) {
        if(enabled)detectDragGestures {change,amount->change.consume();vm.wheelMouse(amount.y,density)}
    },contentAlignment=Alignment.Center) {ProductIcon(Mark.Wheel,modifier=Modifier.size(20.dp))}
}
