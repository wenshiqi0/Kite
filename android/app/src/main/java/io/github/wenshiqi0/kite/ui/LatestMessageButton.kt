package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable
fun LatestMessageButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Surface(onClick = onClick, color = KiteColors.AccentSurface, shape = CircleShape,
        modifier = modifier.size(48.dp).semantics { contentDescription = "回到最新消息"; role = Role.Button }) {
        Box(contentAlignment = Alignment.Center) {
            ProductIcon(Mark.Chevron, KiteColors.Accent, Modifier.size(24.dp))
        }
    }
}
