package io.github.wenshiqi0.kite.ui

/** Repair CJK strong-closing boundaries for display, leaving stored/copy-source text intact. */
internal fun normalizeCjkStrong(source: String): String {
    val out = StringBuilder(source.length)
    var i = 0
    var opener = -1
    var inlineTicks = 0
    var fence: Char? = null
    var fenceLength = 0
    var lineStart = true
    while (i < source.length) {
        if (lineStart) {
            var start = i
            while (start < source.length && source[start] == ' ') start++
            val marker = source.getOrNull(start)
            var end = start
            if (marker == '`' || marker == '~') while (end < source.length && source[end] == marker) end++
            val run = end - start
            if (start - i <= 3 && run >= 3 && (fence == null || marker == fence && run >= fenceLength)) {
                fence = if (fence == null) marker else null
                fenceLength = run
                val stop = source.indexOf('\n', i).let { if (it < 0) source.length else it + 1 }
                out.append(source, i, stop); i = stop; opener = -1; inlineTicks = 0; lineStart = true; continue
            }
            if (fence != null || start - i >= 4) {
                val stop = source.indexOf('\n', i).let { if (it < 0) source.length else it + 1 }
                out.append(source, i, stop); i = stop; opener = -1; lineStart = true; continue
            }
            lineStart = false
        }
        val c = source[i]
        if (c == '\n') {
            out.append(c); i++; lineStart = true
            if (source.getOrNull(i) == '\n') opener = -1
            continue
        }
        if (c == '\\') {
            val stop = minOf(i + 2, source.length)
            out.append(source, i, stop); i = stop; continue
        }
        if (c == '`') {
            var end = i
            while (end < source.length && source[end] == '`') end++
            val count = end - i
            if (inlineTicks == 0) inlineTicks = count else if (inlineTicks == count) inlineTicks = 0
            out.append(source, i, end); i = end; continue
        }
        if (inlineTicks == 0 && (c == '<' || c == ']' && source.getOrNull(i + 1) == '(')) {
            val stop = source.indexOf(if (c == '<') '>' else ')', i + 1)
            if (stop >= 0) { out.append(source, i, stop + 1); i = stop + 1; continue }
        }
        if (inlineTicks == 0 && c == '*') {
            var end = i
            while (end < source.length && source[end] == '*') end++
            if (end - i == 2) {
                val before = source.getOrNull(i - 1)
                val after = source.getOrNull(end)
                val body = if (opener >= 0) source.substring(opener + 2, i) else ""
                val cjkClose = opener >= 0 && before != null && before in "。，！？：；、）》】」』”’" && after?.isLetterOrDigit() == true && body.any { it.code in 0x3400..0x9fff }
                val closes = opener >= 0 && before != null && !before.isWhitespace() && (after == null || after.isWhitespace() || !before.isCjkPunctuation() || after.isCjkPunctuation())
                out.append("**")
                if (cjkClose) { out.append(' '); opener = -1 }
                else if (closes) opener = -1
                else if (after != null && !after.isWhitespace()) opener = i
                i = end; continue
            }
            out.append(source, i, end); i = end; continue
        }
        out.append(c); i++
    }
    return out.toString()
}

private fun Char.isCjkPunctuation(): Boolean = this in "。，！？：；、）》】」』”’"
