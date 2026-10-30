package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import kotlin.math.roundToInt

private val ControlInk = KiteColors.Text
private val ControlBlue = KiteColors.Accent

@Composable fun DesktopModeSwitch(operating: Boolean, enabled: Boolean, change: (Boolean) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        ControlButton(text = stringResource(if (operating) R.string.mode_operate else R.string.mode_watch), mark = if (operating) Mark.Pointer else Mark.Eye, active = operating, enabled = enabled) { menu = true }
        DropdownMenu(menu, { menu = false }) {
            listOf(false, true).forEach { mode ->
                DropdownMenuItem(text = { Column { Text(stringResource(if (mode) R.string.mode_operate else R.string.mode_watch)); Text(stringResource(if (mode) R.string.mode_operate_hint else R.string.mode_watch_hint), fontSize = 11.sp, color = ControlInk) } }, leadingIcon = { ProductIcon(if (mode) Mark.Pointer else Mark.Eye) }, trailingIcon = { if (mode == operating) ProductIcon(Mark.Check, ControlBlue, Modifier.size(18.dp)) }, onClick = { menu = false; change(mode) })
            }
        }
    }
}

@Composable private fun ControlButton(text: String? = null, mark: Mark? = null, label: Int = R.string.desktop_tools, active: Boolean = false, enabled: Boolean = true, action: () -> Unit) {
    val description = if (label == R.string.zoom_in || label == R.string.zoom_out) stringResource(label) else text ?: stringResource(label)
    Box(Modifier.height(48.dp).widthIn(min=48.dp).clickable(enabled=enabled,onClick=action).semantics { contentDescription = description },contentAlignment=Alignment.Center) {
        Row(Modifier.heightIn(min=32.dp).background(if(active)KiteColors.AccentSurface else KiteColors.Raised.copy(alpha=.92f),RoundedCornerShape(10.dp)).padding(horizontal=9.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
            if (mark != null) ProductIcon(mark, if (active) ControlBlue else ControlInk, Modifier.size(18.dp))
            if (text != null) { if (mark != null) Spacer(Modifier.width(5.dp)); Text(text, fontSize = 12.sp,color=if(active)ControlBlue else ControlInk) }
        }
    }
}

@Composable fun ZoomControl(vm: HostCheckViewModel) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        val label = stringResource(R.string.zoom_controls)
        Box(Modifier.semantics { contentDescription = label }) { ControlButton(text = "${(vm.zoom * 100).roundToInt()}%", mark = Mark.Zoom, active = vm.zoom > 1.01f || vm.panMode) { expanded = true } }
        DropdownMenu(expanded, { expanded = false }) {
            Row(Modifier.padding(horizontal = 10.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                ControlButton(text = "−", label = R.string.zoom_out) { vm.zoom = (vm.zoom / 1.5f).coerceAtLeast(1f) }
                Text("${(vm.zoom * 100).roundToInt()}%", fontSize = 13.sp, modifier = Modifier.widthIn(min = 42.dp))
                ControlButton(text = "+", label = R.string.zoom_in) { vm.zoom = (vm.zoom * 1.5f).coerceAtMost(8f) }
            }
            DropdownMenuItem(text = { Text(stringResource(R.string.fit_desktop)) }, leadingIcon = { ProductIcon(Mark.Shrink) }, onClick = { vm.fitVideo(); expanded = false })
            DropdownMenuItem(text = { Text(stringResource(R.string.pan_viewport)) }, leadingIcon = { ProductIcon(Mark.Pan) }, trailingIcon = { if (vm.panMode) ProductIcon(Mark.Check) }, onClick = { vm.panMode = !vm.panMode; expanded = false })
        }
    }
}
