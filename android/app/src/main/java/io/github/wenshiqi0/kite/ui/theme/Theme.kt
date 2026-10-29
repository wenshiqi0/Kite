package io.github.wenshiqi0.kite.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

@Composable
fun KiteTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = lightColorScheme(
        primary = Color(0xFF2D85D8), onPrimary = Color.White,
        primaryContainer = Color(0xFFE2EEFC), onPrimaryContainer = Color(0xFF2269AA),
        background = Color.White, onBackground = Color(0xFF161715),
        surface = Color.White, onSurface = Color(0xFF161715),
        surfaceContainer = Color(0xFFF5F4EF), outline = Color(0xFFBFBFB8)
    ), content = content)
}
