package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Typeface
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.text.Spanned
import android.text.TextPaint
import android.text.style.MetricAffectingSpan
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
import androidx.compose.ui.unit.em
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
import org.commonmark.node.Code
import org.commonmark.node.Link
import org.commonmark.node.Paragraph

/** Presentation only: original Markdown remains in the shared message history. */
@Composable
fun MarkdownMessage(text: String, modifier: Modifier = Modifier, color: Color = KiteColors.Text, size: Float = 16f, streaming: Boolean = false) {
    AndroidView(
        factory = { MarkdownTextView(it) },
        update = {
            if (it.currentTextColor != color.toArgb()) it.setTextColor(color.toArgb())
            it.setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
            it.bind(text, streaming)
        },
        modifier = modifier,
    )
}

/** Styled control labels keep their parent's selection action and original wire value. */
@Composable
fun MarkdownLabel(text: String, modifier: Modifier = Modifier, color: Color = KiteColors.Text, size: Float = 13f) {
    val context = LocalContext.current
    val label = remember(text, context) { markdownLabel(context, text) }
    Text(label, modifier = modifier, color = color, fontSize = size.sp)
}

fun markdownLabel(context: Context, source: String): AnnotatedString {
    val text = MarkdownRenderer.get(context).toMarkdown(normalizeCjkStrong(source))
    return buildAnnotatedString {
        append(text.toString())
        for (span in text.getSpans(0, text.length, Any::class.java)) {
            val style = when (span) {
                is StrongEmphasisSpan -> SpanStyle(fontWeight = FontWeight.Bold)
                is EmphasisSpan -> SpanStyle(fontStyle = FontStyle.Italic)
                is CodeSpan -> SpanStyle(fontFamily = FontFamily.Monospace, fontSize = .9.em, background = KiteColors.Raised)
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
    private var renderedSource: String? = null
    private var streaming = false
    init {
        setPadding(0, 0, 0, 0)
        includeFontPadding = false
        setLineSpacing(0f, 1.3f)
        setTextIsSelectable(true)
        setTextColor(0xFFE5E9EE.toInt())
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
    }
    fun bind(value: String, inProgress: Boolean = false) {
        if (source == value && streaming == inProgress) return
        source = value
        streaming = inProgress
        val display=normalizeCjkStrong(value)
        val tail=display.substringAfterLast("\n\n")
        // Stabilize a leading strong span during delivery. Code blocks and
        // escaped/literal markers keep their original CommonMark behavior.
        val partialStrong=inProgress && tail.startsWith("**") && tail.length>2 && !tail[2].isWhitespace() && tail.indexOf("**",2)<0 && markdown.parse(display).lastChild is Paragraph
        val rendered = if (partialStrong) display + "**" else display
        // The response phase can change while the visible Markdown stays identical.
        // Retain the selectable text and its layout across those updates.
        if (renderedSource == rendered) return
        markdown.setMarkdown(this, rendered)
        renderedSource = rendered
        (text as? Spanned)?.let { body ->
            body.getSpans(0, body.length, CompactCodeSpan::class.java).forEach { it.drawCompactBackground = true }
        }
    }

    // Paint.bgColor fills the entire TextView line, including its extra leading.
    // Draw only the code font's own band, leaving line breaking/selection native.
    fun inlineCodeBackgrounds(): List<RectF> {
        val body = text as? Spanned ?: return emptyList()
        val lines = layout ?: return emptyList()
        val padding = resources.displayMetrics.density
        val result = ArrayList<RectF>()
        for (span in body.getSpans(0, body.length, CompactCodeSpan::class.java)) {
            val start = body.getSpanStart(span)
            val end = body.getSpanEnd(span)
            if (start >= end) continue
            val codePaint = TextPaint(paint)
            body.getSpans(start, start + 1, MetricAffectingSpan::class.java).forEach { it.updateMeasureState(codePaint) }
            val metrics = codePaint.fontMetrics
            for (line in lines.getLineForOffset(start)..lines.getLineForOffset(end - 1)) {
                val from = maxOf(start, lines.getLineStart(line))
                var to = minOf(end, lines.getLineEnd(line))
                while (to > from && body[to - 1] == '\n') to--
                if (from >= to) continue
                // Selection geometry also handles leading margins, wrapping and bidi runs.
                val path = Path()
                lines.getSelectionPath(from, to, path)
                val baseline = lines.getLineBaseline(line)
                val top = maxOf(lines.getLineTop(line).toFloat(), baseline + metrics.ascent - padding)
                val bottom = minOf(lines.getLineBottom(line).toFloat(), baseline + metrics.descent + padding)
                // An end offset at a soft wrap also selects a zero-width contour on
                // the next line. Intersect before taking bounds so that contour
                // cannot extend this line's background across the list prefix.
                val band = Path().apply {
                    addRect(maxOf(0f, lines.getLineLeft(line)), top, minOf(lines.width.toFloat(), lines.getLineRight(line)), bottom, Path.Direction.CW)
                }
                path.op(band, Path.Op.INTERSECT)
                if (!path.isEmpty) {
                    val bounds = RectF()
                    path.computeBounds(bounds, true)
                    result.add(bounds)
                }
            }
        }
        return result
    }

    override fun onDraw(canvas: Canvas) {
        val background = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = KiteColors.Raised.toArgb() }
        val saved = canvas.save()
        canvas.translate(totalPaddingLeft.toFloat(), totalPaddingTop.toFloat())
        val radius = 3 * resources.displayMetrics.density
        inlineCodeBackgrounds().forEach { canvas.drawRoundRect(it, radius, radius, background) }
        canvas.restoreToCount(saved)
        super.onDraw(canvas)
    }
}

private class CompactCodeSpan(theme: MarkwonTheme) : CodeSpan(theme) {
    var drawCompactBackground = false
    override fun updateMeasureState(paint: TextPaint) {
        super.updateMeasureState(paint)
        paint.textSize *= .9f
    }
    override fun updateDrawState(paint: TextPaint) {
        super.updateDrawState(paint)
        paint.textSize *= .9f
        // Other renderers (notably table cells) retain their standard background.
        if (drawCompactBackground) paint.bgColor = android.graphics.Color.TRANSPARENT
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
        .usePlugin(TaskListPlugin.create(0xFF9BBEF5.toInt(), 0xFF9AA4B1.toInt(), 0xFF102039.toInt()))
        .usePlugin(TablePlugin.create { it
            .tableBorderColor(0xFF343B44.toInt())
            .tableCellPadding((8 * context.resources.displayMetrics.density).toInt())
            .tableHeaderRowBackgroundColor(0xFF2C3138.toInt())
            .tableEvenRowBackgroundColor(0xFF191C20.toInt())
            .tableOddRowBackgroundColor(0xFF23272D.toInt())
        })
        .usePlugin(MovementMethodPlugin.create(TableAwareMovementMethod.create()))
        .usePlugin(object : AbstractMarkwonPlugin() {
            override fun configureSpansFactory(builder: MarkwonSpansFactory.Builder) {
                builder.setFactory(Code::class.java) { configuration, _ -> CompactCodeSpan(configuration.theme()) }
                builder.setFactory(Link::class.java) { configuration, props ->
                    val link = CoreProps.LINK_DESTINATION.require(props)
                    if (isExternalLink(link)) LinkSpan(configuration.theme(), link, configuration.linkResolver()) else null
                }
            }
            override fun configureTheme(builder: MarkwonTheme.Builder) {
                builder.linkColor(0xFF9BBEF5.toInt())
                    .codeTypeface(Typeface.MONOSPACE)
                    .codeTextColor(0xFFE5E9EE.toInt())
                    .codeBackgroundColor(0xFF23272D.toInt())
                    .codeBlockBackgroundColor(0xFF23272D.toInt())
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
