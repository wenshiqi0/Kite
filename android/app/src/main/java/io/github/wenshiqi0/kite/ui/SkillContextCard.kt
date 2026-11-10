package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Skill context has its own chrome and never adopts the user's message style. */
@Composable
internal fun SkillContextCard(skill: BlockInfo.SkillContext) {
    var expanded by rememberSaveable(skill.name, skill.path) { mutableStateOf(false) }
    val toggleLabel = stringResource(
        if (expanded) R.string.skill_context_collapse else R.string.skill_context_expand,
        skill.name,
    )
    Surface(
        color = KiteColors.Raised,
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, KiteColors.Muted.copy(alpha = .2f)),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
    ) {
        Column {
            Row(
                Modifier.fillMaxWidth()
                    .clickable(role = Role.Button, onClickLabel = toggleLabel) { expanded = !expanded }
                    .semantics { contentDescription = toggleLabel }
                    .padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                ProductIcon(Mark.Skill, KiteColors.Muted, Modifier.size(20.dp))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(stringResource(R.string.skill_context_title), color = KiteColors.Muted, fontSize = 12.sp)
                    Text(skill.name, color = KiteColors.Text, fontSize = 14.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
                ProductIcon(Mark.Chevron, KiteColors.Muted, Modifier.size(16.dp).rotate(if (expanded) 180f else 0f))
            }
            if (expanded) {
                HorizontalDivider(color = KiteColors.Muted.copy(alpha = .2f))
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    SelectionContainer {
                        Text(stringResource(R.string.skill_context_source, skill.path), color = KiteColors.Muted, fontSize = 12.sp)
                    }
                    if (skill.content.isNotBlank()) MarkdownMessage(skill.content, size = 14f)
                }
            }
        }
    }
}
