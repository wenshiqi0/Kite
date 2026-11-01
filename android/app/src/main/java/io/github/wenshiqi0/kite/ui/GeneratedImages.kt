package io.github.wenshiqi0.kite.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

internal fun decodeGeneratedImage(file: File): Bitmap {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    require(bounds.outWidth > 0 && bounds.outHeight > 0) { "Invalid image" }
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2048) sample *= 2
    return requireNotNull(BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample }))
}

@Composable
fun GeneratedImages(vm: HostCheckViewModel, images: List<AttachmentInfo>, conversation: String = vm.engine.timeline.context, onExternalActivityChanged: () -> Unit = {}) {
    images.forEach { image ->
        key(conversation, image.id) { GeneratedImage(vm, conversation, image, onExternalActivityChanged) }
    }
}

@Composable
private fun GeneratedImage(vm: HostCheckViewModel, conversation: String, image: AttachmentInfo, onExternalActivityChanged: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var bitmap by remember(conversation, image.id) { mutableStateOf<Bitmap?>(null) }
    var file by remember(conversation, image.id) { mutableStateOf<File?>(null) }
    var failed by remember(conversation, image.id) { mutableStateOf(false) }
    var attempt by remember(conversation, image.id) { mutableIntStateOf(0) }
    var preview by remember(conversation, image.id) { mutableStateOf(false) }
    LaunchedEffect(conversation, image.id, attempt) {
        failed = false
        try {
            val local = vm.engine.generatedImage(conversation, image.id)
            bitmap = withContext(Dispatchers.IO) { decodeGeneratedImage(local) }
            file = local
        } catch (cancel: CancellationException) { throw cancel }
        catch (_: Exception) { failed = true }
    }
    // Capture the chosen immutable file when opening Documents; a conversation
    // change cannot cause the callback to save another image.
    var savingFile by remember { mutableStateOf<File?>(null) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(image.mime)) { uri ->
        vm.externalActivity = false
        onExternalActivityChanged()
        val source = savingFile
        savingFile = null
        if (uri != null && source != null) scope.launch {
            val success = runCatching {
                withContext(Dispatchers.IO) {
                    requireNotNull(context.contentResolver.openOutputStream(uri)).use { destination ->
                        source.inputStream().use { it.copyTo(destination) }
                    }
                }
            }.isSuccess
            Toast.makeText(context, if (success) R.string.generated_image_saved else R.string.generated_image_save_failed, Toast.LENGTH_SHORT).show()
        }
    }
    Column(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        val current = bitmap
        if (current != null) {
            val label = stringResource(R.string.generated_image_preview)
            Image(current.asImageBitmap(), label, contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxWidth().heightIn(max = 320.dp).clickable { preview = true })
            TextButton(onClick = {
                savingFile = file
                vm.externalActivity = true
                onExternalActivityChanged()
                runCatching { save.launch(image.name) }.onFailure { vm.externalActivity = false; savingFile = null; onExternalActivityChanged() }
            }, enabled = file != null) { Text(stringResource(R.string.generated_image_save)) }
        } else if (failed) {
            Text(stringResource(R.string.generated_image_failed), color = MaterialTheme.colorScheme.error)
            TextButton(onClick = { attempt++ }) { Text(stringResource(R.string.retry)) }
        } else {
            Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                Text(stringResource(R.string.generated_image_loading))
            }
        }
    }
    if (preview && bitmap != null) Dialog(onDismissRequest = { preview = false }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        var scale by remember { mutableFloatStateOf(1f) }
        var offset by remember { mutableStateOf(Offset.Zero) }
        val transform = rememberTransformableState { zoom, pan, _ ->
            scale = (scale * zoom).coerceIn(1f, 8f)
            offset = if (scale == 1f) Offset.Zero else offset + pan
        }
        Box(Modifier.fillMaxSize().background(KiteColors.Background)) {
            Image(bitmap!!.asImageBitmap(), image.name, contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize().graphicsLayer {
                    scaleX = scale; scaleY = scale; translationX = offset.x; translationY = offset.y
                }.transformable(transform))
            TextButton(onClick = { preview = false }, modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding()) { Text(stringResource(R.string.dismiss)) }
        }
    }
}
