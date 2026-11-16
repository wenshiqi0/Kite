package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import java.util.Locale

private const val RESOURCE_GIB = 1073741824.0

internal fun resourceBytes(bytes: ULong): String = if (bytes >= 1073741824uL) {
    String.format(Locale.ROOT, "%.1f GiB", bytes.toDouble() / RESOURCE_GIB)
} else String.format(Locale.ROOT, "%.0f MiB", bytes.toDouble() / 1048576.0)

internal fun resourceBudgetBytes(value: String): ULong? = value.trim().toDoubleOrNull()
    ?.takeIf { it.isFinite() && it >= 0.25 && it <= 1024.0 }
    ?.let { (it * RESOURCE_GIB).toULong() }

internal fun canStartResourceAgent(profile: HostResourcesInfo?): Boolean =
    profile?.let { it.available && it.limitEnforced && it.canStartAgents } == true

@Composable
internal fun ResourceWarning(profile: HostResourcesInfo?, modifier: Modifier = Modifier, onDetails: (() -> Unit)? = null) {
    if (profile == null || profile.available && profile.memoryPressure == HostMemoryPressure.NORMAL && profile.canStartAgents) return
    val critical = profile.memoryPressure == HostMemoryPressure.CRITICAL || !profile.limitEnforced
    val title = stringResource(when {
        !profile.available -> R.string.resource_unavailable
        !profile.limitEnforced -> R.string.resource_protection_unavailable
        else -> when (profile.reason) {
            MemoryWarningReason.HOST_MEMORY -> R.string.resource_host_memory_warning
            MemoryWarningReason.AGENT_BUDGET -> R.string.resource_agent_memory_warning
            MemoryWarningReason.MEMORY_STALL -> R.string.resource_memory_stall
            MemoryWarningReason.RECENT_OOM -> R.string.resource_oom_warning
            MemoryWarningReason.STARTUP_RESERVATION -> R.string.resource_start_paused
            MemoryWarningReason.ENFORCEMENT_UNAVAILABLE -> R.string.resource_protection_unavailable
            MemoryWarningReason.NONE -> R.string.resource_memory_recovering
        }
    })
    Surface(color = if (critical) MaterialTheme.colorScheme.errorContainer else KiteColors.WarningSurface,
        shape = MaterialTheme.shapes.medium, modifier = modifier.fillMaxWidth().semantics { contentDescription = title }) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.labelLarge)
                if (profile.available) Text(stringResource(if (profile.reason == MemoryWarningReason.RECENT_OOM) R.string.resource_oom_hint else if (profile.canStartAgents) R.string.resource_existing_tasks_continue else R.string.resource_new_sessions_paused), style = MaterialTheme.typography.bodySmall)
            }
            onDetails?.let { TextButton(onClick = it) { Text(stringResource(R.string.details)) } }
        }
    }
}

@Composable
private fun ResourceMeter(title: String, value: String, fraction: Float?, caption: String? = null) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(title, style = MaterialTheme.typography.labelLarge)
            Text(value, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Medium)
        }
        if (fraction != null) LinearProgressIndicator(progress = { fraction.coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
        else LinearProgressIndicator(Modifier.fillMaxWidth())
        caption?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted) }
    }
}

@Composable
internal fun HostResourcesCard(
    profile: HostResourcesInfo?, connected: Boolean, saving: Boolean,
    onRefresh: () -> Unit, onSetLimit: (ULong) -> Unit,
) {
    var editing by remember { mutableStateOf(false) }
    var allProcesses by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.host_resources), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                if (connected) TextButton(onClick = onRefresh, enabled = !saving) { Text(stringResource(R.string.resource_refresh)) }
            }
            when {
                !connected -> Text(stringResource(R.string.resource_connect_hint), style = MaterialTheme.typography.bodyMedium)
                profile == null -> { LinearProgressIndicator(Modifier.fillMaxWidth()); Text(stringResource(R.string.resource_sampling)) }
                !profile.available -> ResourceWarning(profile)
                else -> {
                    ResourceWarning(profile)
                    ResourceMeter("CPU", profile.cpuPercent?.let { String.format(Locale.ROOT, "%.1f%% · %d 核", it, profile.cpuCores.toInt()) } ?: stringResource(R.string.resource_sampling),
                        profile.cpuPercent?.div(100.0)?.toFloat(),
                        stringResource(R.string.resource_load, String.format(Locale.ROOT, "%.2f / %.2f / %.2f", profile.loadOne, profile.loadFive, profile.loadFifteen)))
                    val total = profile.memoryTotalBytes
                    val used = total - profile.memoryAvailableBytes.coerceAtMost(total)
                    ResourceMeter(stringResource(R.string.resource_host_memory), "${resourceBytes(used)} / ${resourceBytes(total)}",
                        if (total > 0uL) (used.toDouble() / total.toDouble()).toFloat() else 0f,
                        stringResource(R.string.resource_available_swap, resourceBytes(profile.memoryAvailableBytes), resourceBytes(profile.swapUsedBytes), resourceBytes(profile.swapTotalBytes)))
                    HorizontalDivider()
                    val committed = profile.agentMemoryUsedBytes.toDouble() + profile.agentStartupReservedBytes.toDouble()
                    ResourceMeter(stringResource(R.string.resource_agent_budget), "${resourceBytes(profile.agentMemoryUsedBytes)} / ${resourceBytes(profile.agentMemoryLimitBytes)}",
                        if (profile.agentMemoryLimitBytes > 0uL) (committed / profile.agentMemoryLimitBytes.toDouble()).toFloat() else 0f,
                        stringResource(R.string.resource_agent_count, profile.agentProcessCount.toInt(), profile.agentCpuPercent?.let { String.format(Locale.ROOT, "%.1f%%", it) } ?: "—"))
                    Text(stringResource(if (profile.limitEnforced) R.string.resource_limit_enforced else R.string.resource_limit_not_enforced),
                        color = if (profile.limitEnforced) KiteColors.Success else MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall)
                    if (profile.agentReclaimableBytes > 0uL) Text(stringResource(R.string.resource_reclaimable, resourceBytes(profile.agentReclaimableBytes)), style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted)
                    if (profile.agentMigrationReservedBytes > 0uL) Text(stringResource(R.string.resource_migration_reserve, resourceBytes(profile.agentMigrationReservedBytes)), style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted)
                    if (profile.agentStartupReservedBytes > 0uL) Text(stringResource(R.string.resource_startup_reserve, resourceBytes(profile.agentStartupReservedBytes)), style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted)
                    OutlinedButton(onClick = { editing = true }, enabled = !saving, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.resource_edit_limit))
                    }
                    Text(stringResource(R.string.resource_scope_hint), style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted)
                    if (profile.processes.isNotEmpty()) {
                        HorizontalDivider()
                        Text(stringResource(R.string.resource_top_processes), style = MaterialTheme.typography.labelLarge)
                        profile.processes.take(if (allProcesses) 8 else 3).forEach { process ->
                            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Text(process.name, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
                                    Text("PID ${process.pid} · " + stringResource(if (process.managed) R.string.resource_agent_process else R.string.resource_other_process), style = MaterialTheme.typography.bodySmall, color = KiteColors.Muted)
                                }
                                Text(resourceBytes(process.memoryBytes), style = MaterialTheme.typography.bodyMedium)
                            }
                        }
                        if (profile.processes.size > 3) TextButton(onClick = { allProcesses = !allProcesses }) {
                            Text(stringResource(if (allProcesses) R.string.resource_collapse_processes else R.string.resource_all_processes))
                        }
                    }
                }
            }
            if (saving) LinearProgressIndicator(Modifier.fillMaxWidth())
        }
    }
    if (editing && profile != null) {
        var value by remember(profile.agentMemoryLimitBytes) { mutableStateOf(String.format(Locale.ROOT, "%.2f", profile.agentMemoryLimitBytes.toDouble() / RESOURCE_GIB).trimEnd('0').trimEnd('.')) }
        val parsed = resourceBudgetBytes(value)
        AlertDialog(onDismissRequest = { if (!saving) editing = false }, title = { Text(stringResource(R.string.resource_edit_limit)) },
            text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(value, { value = it }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                    label = { Text(stringResource(R.string.resource_limit_gib)) }, isError = parsed == null, modifier = Modifier.fillMaxWidth())
                Text(stringResource(R.string.resource_limit_hint, resourceBytes(profile.agentMemoryUsedBytes + profile.agentStartupReservedBytes)), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.resource_limit_effect), style = MaterialTheme.typography.bodySmall)
            } },
            confirmButton = { TextButton(enabled = parsed != null && !saving, onClick = { resourceBudgetBytes(value)?.let { onSetLimit(it); editing = false } }) { Text(stringResource(R.string.resource_save_limit)) } },
            dismissButton = { TextButton(onClick = { editing = false }, enabled = !saving) { Text(stringResource(R.string.cancel)) } })
    }
}
