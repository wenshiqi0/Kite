package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ChatSkillInfo
import io.github.wenshiqi0.kite.core.SkillDraftInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable
fun SkillPicker(
    draft: SkillDraftInfo,
    enabled: Boolean,
    select: (ChatSkillInfo, Boolean) -> Unit,
    refresh: () -> Unit,
    dismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val skills = remember(draft.available, query) {
        val search = query.trim()
        draft.available.filter { it.matchesSearch(search) }
    }
    val selected = draft.selected.map { it.path }.toSet()
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text(stringResource(R.string.skills_title)) },
        text = {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = query, onValueChange = { query = it }, singleLine = true,
                    placeholder = { Text(stringResource(R.string.skills_search)) },
                    modifier = Modifier.fillMaxWidth(),
                )
                if (draft.loading) Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    Text(stringResource(R.string.skills_loading), modifier = Modifier.padding(start = 10.dp), color = KiteColors.Muted)
                }
                if (draft.details.isNotBlank()) Text(
                    stringResource(if (draft.loaded) R.string.skills_partial_error else R.string.skills_load_error),
                    color = MaterialTheme.colorScheme.error,
                )
                if (!draft.loading && skills.isEmpty() && draft.loaded) Text(
                    stringResource(if (query.isBlank()) R.string.skills_empty else R.string.skills_no_match), color = KiteColors.Muted,
                )
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp)) {
                    items(skills, key = { it.path }) { skill ->
                        Row(
                            Modifier.fillMaxWidth().toggleable(
                                value = skill.path in selected, enabled = enabled && !draft.loading,
                                role = Role.Checkbox, onValueChange = { select(skill, it) },
                            ).padding(vertical = 10.dp),
                            verticalAlignment = Alignment.Top,
                        ) {
                            Checkbox(checked = skill.path in selected, onCheckedChange = null, modifier = Modifier.padding(end = 10.dp))
                            Column(Modifier.weight(1f)) {
                                Text(skill.presentationName, style = MaterialTheme.typography.titleSmall)
                                Text(
                                    skill.presentationDescription.ifBlank { stringResource(R.string.skills_no_description) },
                                    color = KiteColors.Muted, fontSize = 12.sp,
                                    maxLines = 3, overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }
                }
                if (selected.isNotEmpty()) Text(stringResource(R.string.skills_selected_count, selected.size), color = KiteColors.Accent)
            }
        },
        confirmButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.skills_done)) } },
        dismissButton = { TextButton(onClick = refresh, enabled = enabled && !draft.loading) { Text(stringResource(R.string.skills_refresh)) } },
    )
}

@Composable
fun SkillDrafts(skills: List<ChatSkillInfo>, enabled: Boolean, remove: (ChatSkillInfo) -> Unit) {
    if (skills.isEmpty()) return
    Row(
        Modifier.widthIn(max = 680.dp).fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        skills.forEach { skill ->
            val removeDescription = stringResource(R.string.skills_remove, skill.presentationName)
            InputChip(
                selected = true, onClick = { remove(skill) }, enabled = enabled,
                label = { Text(skill.presentationName, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 220.dp)) },
                leadingIcon = { ProductIcon(Mark.Skill, modifier = Modifier.size(16.dp)) },
                trailingIcon = { ProductIcon(Mark.Close, modifier = Modifier.size(14.dp)) },
                modifier = Modifier.semantics { contentDescription = removeDescription },
            )
        }
    }
}
