package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.QuestionReplyInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Each submitted answer stays visually attached to the question that prompted it. */
@Composable
fun AnsweredQuestions(replies: List<QuestionReplyInfo>, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        replies.forEach { reply ->
            Surface(shape = RoundedCornerShape(16.dp), border = BorderStroke(1.dp, KiteColors.Line)) {
                Column {
                    Surface(color = KiteColors.Surface) {
                        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(stringResource(R.string.answered_question), color = KiteColors.Muted, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                            MarkdownMessage(reply.question, size = 15f, modifier = Modifier.fillMaxWidth())
                        }
                    }
                    Surface(color = KiteColors.UserBubble) {
                        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(stringResource(R.string.answered_reply), color = KiteColors.Accent, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                            SelectionContainer { Text(reply.answer, color = KiteColors.Text, fontSize = 16.sp, lineHeight = 24.sp) }
                        }
                    }
                }
            }
        }
    }
}
