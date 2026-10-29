package io.github.wenshiqi0.kite.data

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.util.UUID

data class CachedAttachment(val path: String, val name: String, val mime: String)

/** Platform URI access only; Rust owns upload limits, transfer and the draft queue. */
suspend fun cacheAttachment(context: Context, uri: Uri): CachedAttachment = withContext(Dispatchers.IO) {
    val resolver = context.contentResolver
    val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }?.takeIf { it.isNotBlank() } ?: "attachment"
    val root = File(context.cacheDir, "attachments").apply { mkdirs() }
    val file = File(root, UUID.randomUUID().toString())
    try {
        resolver.openInputStream(uri).use { source ->
            requireNotNull(source)
            file.outputStream().use { destination ->
                val buffer = ByteArray(65536)
                var size = 0L
                while (true) {
                    val count = source.read(buffer)
                    if (count < 0) break
                    size += count
                    require(size <= 25L * 1024 * 1024) { context.getString(io.github.wenshiqi0.kite.R.string.attachment_too_large) }
                    destination.write(buffer, 0, count)
                }
                require(size > 0) { context.getString(io.github.wenshiqi0.kite.R.string.attachment_empty) }
            }
        }
        CachedAttachment(file.absolutePath, name, resolver.getType(uri) ?: "application/octet-stream")
    } catch (error: Exception) { file.delete(); throw error }
}
