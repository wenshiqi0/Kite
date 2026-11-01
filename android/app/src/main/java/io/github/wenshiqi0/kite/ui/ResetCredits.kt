package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.UsageResetCreditsInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.delay
import java.text.DateFormat
import java.util.Date

@Composable
private fun creditClock(): Long {
    val now by produceState(System.currentTimeMillis() / 1000) {
        while (true) { delay(30_000); value = System.currentTimeMillis() / 1000 }
    }
    return now
}

@Composable
internal fun creditExpiryText(credits: UsageResetCreditsInfo, now: Long): String {
    val at = credits.earliestExpiresAt
    if (at == null) return stringResource(if (credits.expiryComplete) R.string.usage_cards_no_expiry else R.string.usage_cards_expiry_unknown)
    val left = creditTimeLeft(at, now)
    if (left.expired) return stringResource(R.string.usage_cards_expired)
    val remaining = when {
        left.days > 0 -> stringResource(R.string.usage_cards_days_hours, left.days.toString(), left.hours.toString())
        left.hours > 0 -> stringResource(R.string.usage_cards_hours_minutes, left.hours.toString(), left.minutes.toString())
        left.minutes > 0 -> stringResource(R.string.usage_cards_minutes, left.minutes.toString())
        else -> stringResource(R.string.usage_cards_under_minute)
    }
    return stringResource(if (credits.expiryComplete) R.string.usage_cards_earliest else R.string.usage_cards_earliest_known, remaining)
}

@Composable
internal fun ResetCreditsSummary(credits: UsageResetCreditsInfo, stale: Boolean) {
    val now = creditClock()
    val count = stringResource(if (stale) R.string.usage_cards_previous_count else R.string.usage_cards_count, credits.availableCount.toString())
    Text(count + if (credits.availableCount > 0uL) " · " + creditExpiryText(credits, now) else "", fontSize = 12.sp, color = if (stale) KiteColors.Warning else KiteColors.Text)
}

@Composable
internal fun ResetCreditsCard(credits: UsageResetCreditsInfo?, stale: Boolean) {
    val now = creditClock()
    Surface(shape = RoundedCornerShape(12.dp), color = KiteColors.Surface, border = BorderStroke(1.dp, KiteColors.Line)) {
        Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(stringResource(R.string.usage_cards_title))
            if (credits == null) Text(stringResource(R.string.usage_cards_not_provided), color = KiteColors.Muted, fontSize = 12.sp)
            else {
                Text(stringResource(if (stale) R.string.usage_cards_previous_count else R.string.usage_cards_count, credits.availableCount.toString()), fontSize = 13.sp)
                if (credits.availableCount > 0uL) {
                    Text(creditExpiryText(credits, now), color = if (credits.earliestExpiresAt?.let { it <= now } == true) KiteColors.Warning else KiteColors.Text, fontSize = 13.sp)
                    credits.earliestExpiresAt?.let { at ->
                        Text(stringResource(R.string.usage_cards_expires, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(at * 1000))), color = KiteColors.Muted, fontSize = 11.sp)
                    }
                    if (!credits.expiryComplete && credits.earliestExpiresAt != null) Text(stringResource(R.string.usage_cards_partial), color = KiteColors.Muted, fontSize = 11.sp)
                }
            }
        }
    }
}
