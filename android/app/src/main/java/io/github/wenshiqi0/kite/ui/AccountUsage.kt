package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable private fun windowLabel(minutes: ULong?): String = when {
    minutes == null -> stringResource(R.string.usage_window)
    minutes > 0uL && minutes % 1440uL == 0uL -> stringResource(R.string.usage_days, (minutes / 1440uL).toString())
    minutes > 0uL && minutes % 60uL == 0uL -> stringResource(R.string.usage_hours, (minutes / 60uL).toString())
    else -> stringResource(R.string.usage_minutes, minutes.toString())
}

private fun primaryUsageBucket(buckets: List<UsageBucketInfo>, model: String?): UsageBucketInfo? {
    if (buckets.isEmpty()) return null
    buckets.find { it.id == "codex" }?.let { return it }
    val lower = model?.lowercase().orEmpty()
    if (lower.isNotEmpty()) {
        when {
            "argon" in lower -> buckets.find { "argon" in it.id.lowercase() || "argon" in (it.name?.lowercase().orEmpty()) }?.let { return it }
            "claude" in lower || "gpt" in lower -> buckets.find { it.id == "3p" || "claude" in (it.name?.lowercase().orEmpty()) || "gpt" in (it.name?.lowercase().orEmpty()) }?.let { return it }
            "gemini" in lower || "orchestrator" in lower -> buckets.find { it.id == "gemini" || ("gemini" in (it.name?.lowercase().orEmpty()) && "argon" !in (it.name?.lowercase().orEmpty())) }?.let { return it }
        }
    }
    return buckets.singleOrNull() ?: buckets.firstOrNull()
}

@Composable fun accountUsageSummary(usage: AccountUsageInfo?, model: String? = null): String {
    if (usage == null || usage.status == UsageStatus.NOT_LOADED) return stringResource(if (usage?.loading == true) R.string.usage_loading else R.string.usage_open)
    if (usage.status == UsageStatus.FAILED || usage.stale) return stringResource(R.string.usage_stale_summary)
    if (usage.status == UsageStatus.UNAVAILABLE) return stringResource(R.string.usage_not_provided)
    val bucket = primaryUsageBucket(usage.buckets, model)
    val windows = listOfNotNull(bucket?.primary, bucket?.secondary)
    if (windows.isNotEmpty()) {
        val labels = mutableListOf<String>()
        for (window in windows) labels.add(stringResource(R.string.usage_short_remaining, windowLabel(window.durationMins), window.remainingPercent.toInt()))
        return labels.joinToString(" · ")
    }
    usage.resetCredits?.let { return stringResource(R.string.usage_cards_count, it.availableCount.toString()) }
    return usage.plan ?: stringResource(R.string.usage_groups, usage.buckets.size)
}

@Composable fun AccountUsageDialog(usage: AccountUsageInfo?, provider: String?, connected: Boolean, refresh: () -> Unit, close: () -> Unit) {
    val isAntigravity = provider == "antigravity"
    val supportsUsage = provider == "codex" || isAntigravity
    AlertDialog(onDismissRequest = close, title = { Text(stringResource(R.string.usage_title)) },
        text = {
            Column(Modifier.fillMaxWidth().heightIn(max = 440.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                when {
                    provider == "claude" -> Text(stringResource(R.string.usage_claude_unavailable), color = KiteColors.Muted)
                    !connected -> Text(stringResource(if (isAntigravity) R.string.usage_connect_antigravity else R.string.usage_connect_codex), color = KiteColors.Muted)
                    else -> {
                        AccountUsageDetails(usage, provider)
                        Text(stringResource(if (isAntigravity) R.string.usage_scope_antigravity else R.string.usage_scope), color = KiteColors.Muted, fontSize = 11.sp)
                    }
                }
            }
        }, confirmButton = { TextButton(onClick = close) { Text(stringResource(R.string.dismiss)) } },
        dismissButton = { TextButton(onClick = refresh, enabled = connected && supportsUsage && usage?.loading != true) { Text(stringResource(R.string.usage_refresh)) } })
}

@Composable
private fun AccountUsageDetails(usage: AccountUsageInfo?, provider: String?, showProvider: Boolean = false) {
    val isAntigravity = provider == "antigravity"
    val providerName = providerLabel(provider ?: "codex")
    val plan = usage?.plan
    if (plan != null) Text(stringResource(R.string.usage_plan_provider, providerName, plan.replaceFirstChar(Char::uppercaseChar)), fontSize = 16.sp)
    else if (showProvider || isAntigravity) Text(providerName, fontSize = 16.sp)
    if (usage?.loading == true) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
        Text(stringResource(R.string.usage_loading), fontSize = 12.sp)
    }
    if (usage?.status == UsageStatus.FAILED || usage?.stale == true) Text(stringResource(if(usage.updatedMs==null)R.string.usage_failed else R.string.usage_stale), color = KiteColors.Warning, fontSize = 12.sp)
    if (usage?.status == UsageStatus.UNAVAILABLE) Text(stringResource(when (usage.unavailable) {
        UsageUnavailable.NO_ACCOUNT -> if (isAntigravity) R.string.usage_antigravity_no_account else R.string.usage_no_account
        UsageUnavailable.UNSUPPORTED_AUTH -> if (isAntigravity) R.string.usage_antigravity_auth_unavailable else R.string.usage_auth_unavailable
        else -> R.string.usage_not_provided
    }), color = KiteColors.Muted)
    if (usage?.ordinaryUsageAllowed == false) Text(stringResource(R.string.usage_included_unavailable), color = KiteColors.Warning, fontSize = 12.sp)
    usage?.buckets?.forEach { bucket ->
        Surface(shape = RoundedCornerShape(12.dp), color = KiteColors.Surface, border = BorderStroke(1.dp, KiteColors.Line)) {
            Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(bucket.name?.takeIf { it.isNotBlank() } ?: bucket.id.takeIf { it.isNotBlank() } ?: providerName, maxLines = 2, overflow = TextOverflow.Ellipsis)
                for (window in listOfNotNull(bucket.primary, bucket.secondary)) {
                    val remaining = window.remainingPercent.toInt().coerceIn(0, 100)
                    val used = 100 - remaining
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(if (usage.stale) R.string.usage_previous_remaining else R.string.usage_remaining, windowLabel(window.durationMins), remaining, used), fontSize = 13.sp)
                        LinearProgressIndicator(progress = { remaining.toFloat() / 100f }, modifier = Modifier.fillMaxWidth().height(4.dp), color = if (usage.stale) KiteColors.Muted else KiteColors.Accent, trackColor = KiteColors.Line)
                        window.resetsAt?.takeIf { it > 0 }?.let { at ->
                            Text(stringResource(R.string.usage_resets, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(at * 1000))), fontSize = 11.sp, color = KiteColors.Muted)
                        }
                    }
                }
                bucket.credits?.let { credit -> Text(stringResource(R.string.usage_credits, when {
                    credit.unlimited -> stringResource(R.string.usage_unlimited)
                    credit.balance != null -> credit.balance!!
                    credit.hasCredits -> stringResource(R.string.usage_amount_unknown)
                    else -> stringResource(R.string.usage_no_extra)
                }), fontSize = 12.sp, color = KiteColors.Muted) }
                if (bucket.limitReached) Text(stringResource(R.string.usage_limit_reached), color = KiteColors.Warning, fontSize = 12.sp)
                if (bucket.spendControlReached) Text(stringResource(R.string.usage_spend_limit), color = KiteColors.Warning, fontSize = 12.sp)
            }
        }
    }
    if (usage != null && (provider == "codex" || usage.resetCredits != null) && usage.status != UsageStatus.NOT_LOADED && usage.unavailable !in listOf(UsageUnavailable.NO_ACCOUNT, UsageUnavailable.UNSUPPORTED_AUTH)) ResetCreditsCard(usage.resetCredits, usage.stale)
    usage?.updatedMs?.let { Text(stringResource(R.string.usage_updated, DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(it))), color = KiteColors.Muted, fontSize = 11.sp) }
}

@Composable
internal fun WorkspaceUsageDialog(load: suspend () -> List<HostAccountUsageInfo>, close: () -> Unit) {
    var hosts by remember { mutableStateOf<List<HostAccountUsageInfo>?>(null) }
    var loading by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    suspend fun refresh() {
        if (loading) return
        loading = true
        failed = false
        try {
            hosts = load()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            hosts = null
            failed = true
        } finally {
            loading = false
        }
    }

    LaunchedEffect(Unit) { refresh() }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(stringResource(R.string.usage_overview)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (loading) {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(stringResource(R.string.usage_loading), color = KiteColors.Muted)
                }
                if (failed) Text(stringResource(R.string.usage_failed), color = KiteColors.Warning)
                if (hosts?.isEmpty() == true) Text(stringResource(R.string.hosts_empty), color = KiteColors.Muted)
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 480.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    items(hosts.orEmpty(), key = { it.hostId }) { host ->
                        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(host.host, style = MaterialTheme.typography.titleMedium)
                            val message = when (host.status) {
                                HostAccountUsageStatus.OFFLINE -> R.string.usage_host_offline
                                HostAccountUsageStatus.UNAVAILABLE -> R.string.usage_host_unavailable
                                HostAccountUsageStatus.READY -> R.string.usage_host_no_providers.takeIf { host.providers.isEmpty() }
                            }
                            if (message != null) Text(stringResource(message), color = KiteColors.Muted, fontSize = 13.sp)
                            for (profile in host.providers) {
                                AccountUsageDetails(profile.usage, profile.provider, showProvider = true)
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text(stringResource(R.string.dismiss)) } },
        dismissButton = {
            TextButton(onClick = { scope.launch { refresh() } }, enabled = !loading) {
                Text(stringResource(R.string.usage_refresh))
            }
        },
    )
}

@Composable fun AccountUsageAccess(usage: AccountUsageInfo?, model: String? = null, open: () -> Unit) {
    Surface(color=KiteColors.Surface,shape=RoundedCornerShape(12.dp),border=BorderStroke(1.dp,KiteColors.Line),
        modifier=Modifier.fillMaxWidth().clickable(onClick=open)) {
        Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(5.dp)) {
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                Text(stringResource(R.string.usage_title),fontSize=13.sp)
                usage?.plan?.let{Text(it.replaceFirstChar(Char::uppercaseChar),fontSize=12.sp,color=KiteColors.Muted)}
            }
            Text(accountUsageSummary(usage, model),fontSize=12.sp,color=if(usage?.stale==true)KiteColors.Warning else KiteColors.Text)
            usage?.resetCredits?.let { ResetCreditsSummary(it, usage.stale) }
            Text(stringResource(R.string.usage_details),fontSize=11.sp,color=KiteColors.Muted)
        }
    }
}
