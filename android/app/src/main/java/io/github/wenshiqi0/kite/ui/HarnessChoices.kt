package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ProviderInfo

/** The host has already verified availability; presentation never invents providers. */
@Composable
fun HarnessChoices(providers: List<ProviderInfo>, selected: String, select: (String) -> Unit) {
    if (providers.isEmpty()) {
        Text(stringResource(R.string.no_available_harness))
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        providers.forEach { provider ->
            FilterChip(selected = selected == provider.id, onClick = { select(provider.id) },
                label = { Text(providerLabel(provider.id)) })
        }
    }
}
