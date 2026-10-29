package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*

@Composable
fun ModelQuestionCard(vm: HostCheckViewModel, question: QuestionInfo, enabled: Boolean) {
    val values = vm.questionValues(question.id)
    var page by rememberSaveable(question.id) { mutableIntStateOf(0) }
    val field = question.fields.getOrNull(page) ?: question.fields.firstOrNull() ?: return
    val focus = LocalFocusManager.current
    Surface(color = Color(0xFFF0F5FC), shape = RoundedCornerShape(16.dp), border = BorderStroke(1.dp, Color(0xFFD3E2F6)), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.model_question_title), fontWeight = FontWeight.SemiBold, fontSize = 15.sp, modifier = Modifier.weight(1f))
                if (question.fields.size > 1) Text(stringResource(R.string.question_progress, page + 1, question.fields.size), color = Color(0xFF777C84), fontSize = 12.sp)
            }
            key(field.id) {
            Column(Modifier.fillMaxWidth().heightIn(max = 300.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf(field).forEach { field ->
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (question.fields.size > 1) Text(field.header, color = Color(0xFF777C84), fontSize = 12.sp)
                        Text(field.text, fontSize = 14.sp)
                        field.options.forEach { option ->
                            val selected = values[field.id] == option.label
                            Surface(color = if (selected) Color(0xFFE0ECFC) else Color.White, shape = RoundedCornerShape(10.dp), border = if (selected) BorderStroke(1.dp, Color(0xFF2D85D8)) else null, modifier = Modifier.fillMaxWidth().clickable(enabled = enabled && !vm.answeringQuestion) { vm.setQuestionValue(question.id, field.id, option.label) }) {
                                Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                                    RadioButton(selected = selected, onClick = null, modifier = Modifier.size(26.dp))
                                    Column(Modifier.weight(1f).padding(start = 8.dp)) { Text(option.label, fontSize = 13.sp); if (option.description.isNotBlank()) Text(option.description, fontSize = 11.sp, color = Color(0xFF777C84)) }
                                }
                            }
                        }
                        if (field.options.isEmpty() || field.allowOther) {
                            val answerDescription = stringResource(R.string.question_field_description, field.header)
                            val typed = values[field.id].orEmpty().takeUnless { value -> field.options.any { it.label == value } }.orEmpty()
                            OutlinedTextField(value = typed, onValueChange = { vm.setQuestionValue(question.id, field.id, it) }, label = { Text(stringResource(if (field.options.isEmpty()) R.string.question_answer else R.string.question_custom_answer)) }, enabled = enabled && !vm.answeringQuestion, visualTransformation = if (field.secret) PasswordVisualTransformation() else VisualTransformation.None, keyboardOptions = KeyboardOptions(keyboardType = if (field.secret) KeyboardType.Password else KeyboardType.Text), singleLine = field.secret, maxLines = if (field.secret) 1 else 4, modifier = Modifier.fillMaxWidth().semantics { contentDescription = answerDescription }, shape = RoundedCornerShape(11.dp))
                        }
                    }
                }
            }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                if (page > 0) TextButton(onClick = { focus.clearFocus(); page-- }, enabled = !vm.answeringQuestion) { Text(stringResource(R.string.question_previous)) }
                val last = page == question.fields.lastIndex
                Button(onClick = { focus.clearFocus(); if (last) vm.answerQuestion(question) else page++ }, enabled = enabled && !vm.answeringQuestion && if (last) question.fields.all { !values[it.id].isNullOrBlank() } else !values[field.id].isNullOrBlank(), modifier = Modifier.weight(1f), shape = RoundedCornerShape(11.dp)) {
                    if (vm.answeringQuestion) { CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 1.5.dp, color = Color.White); Spacer(Modifier.width(8.dp)) }
                    Text(stringResource(if (vm.answeringQuestion) R.string.question_submitting else if (last) R.string.question_submit else R.string.question_next))
                }
            }
        }
    }
}
