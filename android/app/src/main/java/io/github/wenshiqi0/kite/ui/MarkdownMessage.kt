package io.github.wenshiqi0.kite.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Typeface
import android.net.Uri
import android.util.TypedValue
import android.widget.TextView
import android.text.style.StrikethroughSpan
import android.text.style.URLSpan
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import io.noties.markwon.AbstractMarkwonPlugin
import io.noties.markwon.Markwon
import io.noties.markwon.MarkwonConfiguration
import io.noties.markwon.MarkwonSpansFactory
import io.noties.markwon.core.CoreProps
import io.noties.markwon.core.MarkwonTheme
import io.noties.markwon.core.spans.LinkSpan
import io.noties.markwon.core.spans.StrongEmphasisSpan
import io.noties.markwon.core.spans.EmphasisSpan
import io.noties.markwon.core.spans.CodeSpan
import io.noties.markwon.ext.strikethrough.StrikethroughPlugin
import io.noties.markwon.ext.tables.TableAwareMovementMethod
import io.noties.markwon.ext.tables.TablePlugin
import io.noties.markwon.ext.tasklist.TaskListPlugin
import io.noties.markwon.movement.MovementMethodPlugin
import java.util.Locale
import org.commonmark.node.Link
import org.commonmark.node.Paragraph

/** Presentation only: original Markdown remains in the shared message history. */
@Composable
fun MarkdownMessage(text: String, modifier: Modifier = Modifier, color: Color = Color(0xFF161715), size: Float = 16f, streaming: Boolean = false) {
    AndroidView(
        factory = { MarkdownTextView(it) },
        update = { it.setTextColor(color.toArgb()); it.setTextSize(TypedValue.COMPLEX_UNIT_SP, size); it.bind(text,streaming) },
        modifier = modifier,
    )
}

/** Styled control labels keep their parent's selection action and original wire value. */
@Composable
fun MarkdownLabel(text: String, modifier: Modifier = Modifier, color: Color = Color(0xFF161715), size: Float = 13f) {
    val context = LocalContext.current
    val label = remember(text, context) { markdownLabel(context, text) }
    Text(label, modifier = modifier, color = color, fontSize = size.sp)
}

fun markdownLabel(context: Context, source: String): AnnotatedString {
    val text = MarkdownRenderer.get(context).toMarkdown(source)
    return buildAnnotatedString {
        append(text.toString())
        for (span in text.getSpans(0, text.length, Any::class.java)) {
            val style = when (span) {
                is StrongEmphasisSpan -> SpanStyle(fontWeight = FontWeight.Bold)
                is EmphasisSpan -> SpanStyle(fontStyle = FontStyle.Italic)
                is CodeSpan -> SpanStyle(fontFamily = FontFamily.Monospace, background = Color(0xFFE7E8E2))
                is StrikethroughSpan -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                is URLSpan -> SpanStyle(textDecoration = TextDecoration.Underline)
                else -> continue
            }
            addStyle(style, text.getSpanStart(span), text.getSpanEnd(span))
        }
    }
}

class MarkdownTextView(context: Context) : TextView(context) {
    private val markdown = MarkdownRenderer.get(context)
    private var source: String? = null
    private var streaming = false
    init {
        setPadding(0, 0, 0, 0)
        includeFontPadding = false
        setLineSpacing(0f, 1.3f)
        setTextIsSelectable(true)
        setTextColor(0xFF161715.toInt())
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
    }
    fun bind(value: String, inProgress: Boolean = false) {
        if (source == value && streaming == inProgress) return
        source = value
        streaming = inProgress
        val tail=value.substringAfterLast("\n\n")
        // Stabilize a leading strong span during delivery. Code blocks and
        // escaped/literal markers keep their original CommonMark behavior.
        val partialStrong=inProgress && tail.startsWith("**") && tail.length>2 && !tail[2].isWhitespace() && tail.indexOf("**",2)<0 && markdown.parse(value).lastChild is Paragraph
        markdown.setMarkdown(this,if(partialStrong)value+"**" else value)
    }
}

object MarkdownRenderer {
    @Volatile private var instance: Markwon? = null
    fun get(context: Context): Markwon = instance ?: synchronized(this) {
        instance ?: create(context.applicationContext).also { instance = it }
    }

    fun isExternalLink(link: String): Boolean = when (Uri.parse(link).scheme?.lowercase(Locale.ROOT)) {
        "http", "https" -> !Uri.parse(link).host.isNullOrBlank()
        "mailto" -> !Uri.parse(link).schemeSpecificPart.isNullOrBlank()
        else -> false
    }

    private fun create(context: Context): Markwon = Markwon.builder(context)
        .usePlugin(StrikethroughPlugin.create())
        .usePlugin(TaskListPlugin.create(0xFF2D85D8.toInt(), 0xFF888982.toInt(), 0xFFFFFFFF.toInt()))
        .usePlugin(TablePlugin.create { it
            .tableBorderColor(0xFFDDDDD6.toInt())
            .tableCellPadding((8 * context.resources.displayMetrics.density).toInt())
            .tableHeaderRowBackgroundColor(0xFFE8EAE3.toInt())
            .tableEvenRowBackgroundColor(0xFFF5F4EF.toInt())
            .tableOddRowBackgroundColor(0xFFFAFAF7.toInt())
        })
        .usePlugin(MovementMethodPlugin.create(TableAwareMovementMethod.create()))
        .usePlugin(object : AbstractMarkwonPlugin() {
            override fun configureSpansFactory(builder: MarkwonSpansFactory.Builder) {
                builder.setFactory(Link::class.java) { configuration, props ->
                    val link = CoreProps.LINK_DESTINATION.require(props)
                    if (isExternalLink(link)) LinkSpan(configuration.theme(), link, configuration.linkResolver()) else null
                }
            }
            override fun configureTheme(builder: MarkwonTheme.Builder) {
                builder.linkColor(0xFF2269AA.toInt())
                    .codeTypeface(Typeface.MONOSPACE)
                    .codeTextColor(0xFF344451.toInt())
                    .codeBackgroundColor(0xFFE7E8E2.toInt())
                    .codeBlockBackgroundColor(0xFFECEDE7.toInt())
                    .headingBreakHeight(0)
                    .headingTextSizeMultipliers(floatArrayOf(1.5f, 1.3f, 1.15f, 1.05f, 1f, 1f))
            }
            override fun configureConfiguration(builder: MarkwonConfiguration.Builder) {
                builder.linkResolver { view, link ->
                    if (isExternalLink(link)) try {
                        view.context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    } catch (_: ActivityNotFoundException) { /* No compatible app installed. */ }
                }
            }
        })
        .build()
}
