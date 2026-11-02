package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.SessionSettingInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable
fun SessionPermissionControls(settings: List<SessionSettingInfo>, enabled: Boolean, select: (String, String) -> Unit) {
    if (settings.isEmpty()) return
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        HorizontalDivider(color = KiteColors.Line)
        Text(stringResource(R.string.permission_settings), fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
        settings.forEach { setting ->
            var expanded by remember(setting.id) { mutableStateOf(false) }
            val current = setting.options.find { it.value == setting.currentValue }
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(settingTitle(setting), fontSize = 12.sp, color = KiteColors.Muted)
                Box {
                    TextButton(onClick = { expanded = true }, enabled = enabled, contentPadding = PaddingValues(0.dp)) {
                        Text(settingValue(setting.id, setting.currentValue, current?.name ?: setting.currentValue), fontSize = 14.sp)
                        ProductIcon(Mark.Chevron, modifier = Modifier.padding(start = 6.dp).size(14.dp))
                    }
                    DropdownMenu(expanded, { expanded = false }) {
                        setting.options.forEach { option ->
                            DropdownMenuItem(text = { Text(settingValue(setting.id, option.value, option.name)) },
                                trailingIcon = { if (setting.currentValue == option.value) ProductIcon(Mark.Check, modifier = Modifier.size(17.dp)) },
                                onClick = { expanded = false; select(setting.id, option.value) })
                        }
                    }
                }
                settingDescription(setting.id, setting.currentValue)?.let { Text(it, fontSize = 12.sp, color = KiteColors.Muted) }
            }
        }
        Text(stringResource(R.string.permission_next_message), fontSize = 12.sp, color = KiteColors.Muted)
    }
}

@Composable private fun settingTitle(setting: SessionSettingInfo): String = when (setting.id) {
    "approval_mode", "approval_policy" -> stringResource(R.string.permission_approval)
    "sandbox_mode" -> stringResource(R.string.permission_file_access)
    else -> setting.name
}

@Composable private fun settingValue(id: String, value: String, fallback: String): String {
    val resource = when (id to value) {
        "approval_mode" to "ask" -> R.string.permission_ask
        "approval_mode" to "risk" -> R.string.permission_risk
        "approval_mode" to "bypass" -> R.string.permission_bypass
        "approval_policy" to "untrusted" -> R.string.permission_untrusted
        "approval_policy" to "on-request" -> R.string.permission_on_request
        "approval_policy" to "never" -> R.string.permission_never
        "sandbox_mode" to "read-only" -> R.string.permission_read_only
        "sandbox_mode" to "workspace-write" -> R.string.permission_workspace_write
        "sandbox_mode" to "danger-full-access" -> R.string.permission_full_access
        else -> if (value == "default") R.string.permission_default else null
    }
    return resource?.let { stringResource(it) } ?: fallback
}

@Composable private fun settingDescription(id: String, value: String): String? {
    val resource = when (id to value) {
        "approval_mode" to "ask" -> R.string.permission_ask_hint
        "approval_mode" to "risk" -> R.string.permission_risk_hint
        "approval_mode" to "bypass" -> R.string.permission_bypass_hint
        "approval_policy" to "untrusted" -> R.string.permission_untrusted_hint
        "approval_policy" to "on-request" -> R.string.permission_on_request_hint
        "approval_policy" to "never" -> R.string.permission_never_hint
        "sandbox_mode" to "read-only" -> R.string.permission_read_only_hint
        "sandbox_mode" to "workspace-write" -> R.string.permission_workspace_write_hint
        "sandbox_mode" to "danger-full-access" -> R.string.permission_full_access_hint
        else -> null
    }
    return resource?.let { stringResource(it) }
}
