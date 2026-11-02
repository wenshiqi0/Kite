package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ProviderInfo

/** The host has already verified availability; presentation never invents providers. */
@Composable
@OptIn(ExperimentalMaterial3Api::class)
fun HarnessChoices(providers: List<ProviderInfo>, selected: String, select: (String) -> Unit) {
    if (providers.isEmpty()) {
        Text(stringResource(R.string.no_available_harness))
        return
    }
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded, { expanded = !expanded }) {
        OutlinedTextField(value = providers.find { it.id == selected }?.let { providerLabel(it.id) }.orEmpty(),
            onValueChange = {}, readOnly = true, singleLine = true,
            label = { Text(stringResource(R.string.harness_selector)) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
            modifier = Modifier.menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable).fillMaxWidth())
        ExposedDropdownMenu(expanded, { expanded = false }) {
            providers.forEach { provider ->
                DropdownMenuItem(text = { Text(providerLabel(provider.id)) },
                    onClick = { expanded = false; select(provider.id) })
            }
        }
    }
}
