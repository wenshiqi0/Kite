package io.github.wenshiqi0.kite.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/** Shared semantic colours for Compose, native Markdown and desktop controls. */
object KiteColors {
    val Background=Color(0xFF111316)
    val Surface=Color(0xFF191C20)
    val Raised=Color(0xFF23272D)
    val Hover=Color(0xFF2C3138)
    val Line=Color(0xFF343B44)
    val Text=Color(0xFFE5E9EE)
    val Muted=Color(0xFF9AA4B1)
    val Accent=Color(0xFF9BBEF5)
    val AccentSurface=Color(0xFF23334A)
    val OnAccent=Color(0xFF102039)
    val UserBubble=Color(0xFF24364F)
    val Warning=Color(0xFFF2C66D)
    val WarningSurface=Color(0xFF3A301C)
    val Success=Color(0xFFA4D8B3)
    val Mouse=Color(0xFF2E3949)
    val MouseLine=Color(0xFF718096)
}
@Composable
fun KiteTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme=darkColorScheme(
        primary=KiteColors.Accent,onPrimary=KiteColors.OnAccent,
        primaryContainer=KiteColors.AccentSurface,onPrimaryContainer=KiteColors.Text,
        secondary=KiteColors.Muted,onSecondary=KiteColors.Background,
        secondaryContainer=KiteColors.Hover,onSecondaryContainer=KiteColors.Text,
        background=KiteColors.Background,onBackground=KiteColors.Text,
        surface=KiteColors.Surface,onSurface=KiteColors.Text,
        surfaceVariant=KiteColors.Raised,onSurfaceVariant=KiteColors.Muted,
        surfaceContainer=KiteColors.Raised,surfaceContainerHigh=KiteColors.Hover,
        surfaceContainerLow=KiteColors.Surface,outline=KiteColors.MouseLine,outlineVariant=KiteColors.Line,
        error=Color(0xFFFFAAA3),onError=Color(0xFF471917),
        errorContainer=Color(0xFF4C2426),onErrorContainer=Color(0xFFFFDAD6),
        inverseSurface=KiteColors.Text,inverseOnSurface=KiteColors.Background,
    )){
        CompositionLocalProvider(LocalContentColor provides KiteColors.Text){content()}
    }
}
