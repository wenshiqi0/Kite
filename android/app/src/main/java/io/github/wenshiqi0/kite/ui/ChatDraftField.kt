package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable fun ChatDraftField(value: String, onValue: (String) -> Unit, modifier: Modifier = Modifier, onFocus: () -> Unit = {}) {
    var focused by remember { mutableStateOf(false) }
    val description = stringResource(R.string.message_input)
    Surface(modifier.heightIn(min = 44.dp, max = 132.dp), shape = RoundedCornerShape(20.dp), color = KiteColors.Surface,
        border = BorderStroke(1.dp, if (focused) KiteColors.Accent.copy(alpha = .6f) else KiteColors.Line)) {
        BasicTextField(value, onValue, maxLines = 4, textStyle = TextStyle(color = KiteColors.Text, fontSize = 15.sp, lineHeight = 22.sp),
            cursorBrush = SolidColor(KiteColors.Accent),
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = description }.padding(horizontal = 14.dp, vertical = 11.dp)
                .onFocusChanged { focused = it.isFocused; if (it.isFocused) onFocus() },
            decorationBox = { input ->
                Box {
                    if (value.isEmpty()) Text(stringResource(R.string.composer_hint), color = KiteColors.Muted, fontSize = 15.sp, lineHeight = 22.sp)
                    input()
                }
            })
    }
}
