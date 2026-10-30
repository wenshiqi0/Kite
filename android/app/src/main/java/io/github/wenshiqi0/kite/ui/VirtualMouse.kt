package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInParent
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R

/** Buttons and wheel stay reachable while the rest of the canvas accepts direct touch. */
@Composable
fun VirtualMouse(vm: HostCheckViewModel, width: Dp, height: Dp, bounds: (Rect) -> Unit) {
    val density = LocalDensity.current
    var expanded by rememberSaveable { mutableStateOf(true) }
    var x by rememberSaveable { mutableFloatStateOf(.95f) }
    var y by rememberSaveable { mutableFloatStateOf(.94f) }
    val full = expanded && width >= 190.dp && height >= 172.dp
    val w = if (full) 170.dp else 46.dp
    val h = if (full) 162.dp else 46.dp
    val rangeX = (width - w - 16.dp).coerceAtLeast(0.dp)
    val rangeY = (height - h - 16.dp).coerceAtLeast(0.dp)
    val label = stringResource(R.string.virtual_mouse)
    DisposableEffect(vm.target) { onDispose { vm.releaseMouseButtons(); bounds(Rect.Zero) } }
    Surface(color = Color(0xFFF8F9F7), shadowElevation = 5.dp, shape = RoundedCornerShape(21.dp), border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFCDD5D9)), modifier = Modifier.offset { IntOffset(with(density){ (8.dp + rangeX*x).roundToPx() }, with(density){ (8.dp + rangeY*y).roundToPx() }) }.size(w,h).onGloballyPositioned { bounds(it.boundsInParent()) }.semantics { contentDescription = label }) {
        if (!full) IconButton(onClick = { expanded = true; if(width < 190.dp || height < 172.dp)vm.changeLayout(DesktopLayout.Fullscreen) }, modifier = Modifier.fillMaxSize()) { ProductIcon(Mark.Pointer) }
        else Column {
            Row(Modifier.fillMaxWidth().height(32.dp).pointerInput(rangeX,rangeY) {
                detectDragGestures { change, amount -> change.consume()
                    x = (x + amount.x / with(density){rangeX.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
                    y = (y + amount.y / with(density){rangeY.toPx().coerceAtLeast(1f)}).coerceIn(0f,1f)
                }
            }.padding(start=13.dp), verticalAlignment = Alignment.CenterVertically) {
                ProductIcon(Mark.More, modifier=Modifier.size(17.dp));Text(label,fontSize=11.sp,modifier=Modifier.weight(1f).padding(start=6.dp))
                IconButton(onClick={ vm.releaseMouseButtons();expanded=false },modifier=Modifier.size(32.dp)) { ProductIcon(Mark.Shrink,modifier=Modifier.size(15.dp)) }
            }
            Row(Modifier.fillMaxWidth().height(59.dp).padding(horizontal=6.dp), horizontalArrangement=Arrangement.spacedBy(3.dp)) {
                MouseButton(vm,1,R.string.mouse_left,Modifier.weight(1f))
                val wheel = stringResource(R.string.mouse_wheel)
                Box(Modifier.width(42.dp).fillMaxHeight().background(Color(0xFFE9ECE8),RoundedCornerShape(12.dp)).semantics { contentDescription=wheel }.pointerInput(vm.target) {
                    detectDragGestures { change, amount -> change.consume();vm.wheelMouse(amount.y,density.density) }
                },contentAlignment=Alignment.Center) { Text("↕",fontSize=22.sp,color=Color(0xFF696F69)) }
                MouseButton(vm,3,R.string.mouse_right,Modifier.weight(1f))
            }
            val move = stringResource(R.string.mouse_move_area)
            Box(Modifier.fillMaxWidth().weight(1f).padding(6.dp).background(Color(0xFFF0F2EE),RoundedCornerShape(13.dp)).semantics { contentDescription=move }.pointerInput(vm.target) {
                detectDragGestures { change, amount -> change.consume();vm.moveMouse(amount.x,amount.y,density.density) }
            },contentAlignment=Alignment.Center) { Row(verticalAlignment=Alignment.CenterVertically) { ProductIcon(Mark.Pointer,Color(0xFF828A82),Modifier.size(20.dp));Text(stringResource(R.string.move_mouse),fontSize=11.sp,color=Color(0xFF828A82),modifier=Modifier.padding(start=6.dp)) } }
        }
    }
}

@Composable private fun MouseButton(vm: HostCheckViewModel, button: Int, label: Int, modifier: Modifier) {
    val description=stringResource(label);val down=vm.heldMouseButtons and (1 shl button)!=0
    Box(modifier.fillMaxHeight().background(if(down)Color(0xFFD9E9FB) else Color.White,RoundedCornerShape(12.dp)).semantics { contentDescription=description }.pointerInput(vm.target,button) {
        detectTapGestures(onPress={ vm.mouseButton(button,true);try { tryAwaitRelease() } finally { vm.mouseButton(button,false) } })
    },contentAlignment=Alignment.Center) { Text(description,fontSize=12.sp,color=Color(0xFF414941)) }
}
