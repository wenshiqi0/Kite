package io.github.wenshiqi0.kite.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File

/** Shared image decoding for uploads, model output and full-screen previews. */
internal fun decodeAttachmentImage(file: File): Bitmap {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    require(bounds.outWidth > 0 && bounds.outHeight > 0) { "Invalid image" }
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2048) sample *= 2
    return requireNotNull(BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample }))
}
