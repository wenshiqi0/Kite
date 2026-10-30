package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.horizontalScroll
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

private val ControlInk = Color(0xFF52554F)
private val ControlBlue = Color(0xFF2D85D8)

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
    Surface(onClick = action, enabled = enabled, color = if (active) Color(0xFFE4EFFB) else Color.White.copy(alpha = .94f), contentColor = if (active) ControlBlue else ControlInk, shape = RoundedCornerShape(12.dp), modifier = Modifier.height(42.dp).widthIn(min = 44.dp).semantics { contentDescription = description }) {
        Row(Modifier.padding(horizontal = if (text == null) 12.dp else 13.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
            if (mark != null) ProductIcon(mark, if (active) ControlBlue else ControlInk, Modifier.size(19.dp))
            if (text != null) { if (mark != null) Spacer(Modifier.width(7.dp)); Text(text, fontSize = 13.sp) }
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

@Composable fun DesktopToolbar(vm: HostCheckViewModel, width: Dp, modifier: Modifier, keyboard: () -> Unit, help: () -> Unit) {
    var inputs by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    val narrow = width < 620.dp
    Row(modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 9.dp).then(if (narrow) Modifier.horizontalScroll(rememberScrollState()) else Modifier), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Surface(color = Color.White.copy(alpha = .95f), shape = RoundedCornerShape(13.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ControlButton(mark = Mark.Keyboard, label = R.string.keyboard, action = keyboard)
                ControlButton(text = stringResource(R.string.shortcut_esc)) { vm.shortcut(27) }
                if (width >= 700.dp) ControlButton(text = stringResource(R.string.shortcut_tab)) { vm.shortcut(9) }
                ControlButton(text = stringResource(R.string.key_ctrl), active = vm.ctrlHeld) { vm.modifier(true) }
                if (width >= 800.dp) ControlButton(text = stringResource(R.string.key_cmd), active = vm.metaHeld) { vm.modifier(false) }
            }
        }
        if (!narrow) Spacer(Modifier.weight(1f)) else Spacer(Modifier.width(8.dp))
        ZoomControl(vm)
        Box {
            ControlButton(text = stringResource(inputModeLabel(vm.inputMode)), mark = if (vm.inputMode == DesktopInputMode.Trackpad) Mark.Touchpad else Mark.Pointer) { inputs = true }
            DropdownMenu(inputs, { inputs = false }) {
                DesktopInputMode.entries.forEach { mode ->
                    DropdownMenuItem(text = { Text(stringResource(inputModeLabel(mode))) }, leadingIcon = { ProductIcon(if (mode == DesktopInputMode.Trackpad) Mark.Touchpad else Mark.Pointer) }, trailingIcon = { if (vm.inputMode == mode) ProductIcon(Mark.Check, ControlBlue) }, onClick = { inputs = false; vm.chooseInputMode(mode) })
                }
                HorizontalDivider()
                DropdownMenuItem(text = { Text(stringResource(R.string.mouse_left)) }, onClick = { vm.leftClick() })
                DropdownMenuItem(text = { Text(stringResource(R.string.mouse_right)) }, onClick = { vm.rightClick() })
            }
        }
        Box {
            ControlButton(mark = Mark.More, label = R.string.desktop_tools) { more = true }
            DropdownMenu(more, { more = false }) {
                Row(Modifier.padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    listOf(R.string.shortcut_tab to 9, R.string.shortcut_enter to 13).forEach { (label, key) -> ControlButton(text = stringResource(label)) { vm.shortcut(key) } }
                    ControlButton(text = stringResource(R.string.key_cmd), active = vm.metaHeld) { vm.modifier(false) }
                }
                Column(Modifier.padding(8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    ControlButton(text = stringResource(R.string.key_up)) { vm.shortcut(38) }
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        listOf(R.string.key_left to 37, R.string.key_down to 40, R.string.key_right to 39).forEach { (label, key) -> ControlButton(text = stringResource(label)) { vm.shortcut(key) } }
                    }
                }
                DropdownMenuItem(text = { Text(stringResource(R.string.gesture_help)) }, onClick = { more = false; help() })
            }
        }
    }
}

private fun inputModeLabel(mode: DesktopInputMode): Int = when(mode) {
    DesktopInputMode.Hybrid -> R.string.hybrid_touch
    DesktopInputMode.Trackpad -> R.string.trackpad
    DesktopInputMode.Direct -> R.string.direct_touch
}
