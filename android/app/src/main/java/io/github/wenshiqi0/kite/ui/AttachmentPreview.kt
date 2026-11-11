package io.github.wenshiqi0.kite.ui

import android.graphics.Bitmap
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.content.Context
import android.widget.Toast
import android.widget.VideoView
import android.widget.MediaController
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

internal enum class AttachmentPreviewKind { Image, Audio, Video, Text, Other }
internal fun attachmentPreviewKind(mime: String, name: String): AttachmentPreviewKind {
    val type=mime.substringBefore(';').lowercase()
    val extension=name.substringAfterLast('.', "").lowercase()
    val generic=type.isBlank()||type=="application/octet-stream"
    return when {
        type.startsWith("image/") || generic && extension in setOf("png","jpg","jpeg","webp","gif","bmp","heic","heif","avif") -> AttachmentPreviewKind.Image
        type.startsWith("audio/") || generic && extension in setOf("mp3","m4a","aac","wav","ogg","opus","flac","amr") -> AttachmentPreviewKind.Audio
        type.startsWith("video/") || generic && extension in setOf("mp4","webm","mov","mkv","m4v","3gp") -> AttachmentPreviewKind.Video
        type.startsWith("text/") || type in listOf("application/json", "application/xml", "application/yaml") || extension in setOf("txt","md","log","json","yaml","yml","csv","tsv","xml","html","css","js","ts","py","rs","kt","sh","toml","ini","conf") -> AttachmentPreviewKind.Text
        else -> AttachmentPreviewKind.Other
    }
}

internal fun previewTime(ms: Int): String = "%d:%02d".format(ms.coerceAtLeast(0)/60000, ms.coerceAtLeast(0)/1000%60)

@Composable
fun MessageAttachments(vm: HostCheckViewModel, files: List<AttachmentInfo>, onExternalActivityChanged: () -> Unit, inspect: () -> Unit, conversation: String = vm.engine.timeline.context) {
    var selected by remember(conversation) { mutableStateOf<AttachmentInfo?>(null) }
    files.forEach { file ->
        key(conversation, file.id) {
        if (attachmentPreviewKind(file.mime, file.name) == AttachmentPreviewKind.Image) {
            InlineAttachmentImage(vm, conversation, file) { inspect(); selected = file }
        } else {
        Row(Modifier.fillMaxWidth().clickable { inspect(); selected = file }.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            ProductIcon(attachmentMark(file.mime), KiteColors.Accent, Modifier.size(20.dp))
            Column(Modifier.weight(1f).padding(start=8.dp)) {
                Text(file.name,fontSize=13.sp,maxLines=2,overflow=TextOverflow.Ellipsis)
                Text(stringResource(R.string.attachment_open_preview),fontSize=11.sp,color=KiteColors.Muted)
            }
        }
        }
        }
    }
    selected?.let { AttachmentPreview(vm, conversation, it, onExternalActivityChanged) { selected = null } }
}

@Composable
private fun InlineAttachmentImage(vm: HostCheckViewModel, conversation: String, attachment: AttachmentInfo, preview: () -> Unit) {
    var bitmap by remember(conversation, attachment.id) { mutableStateOf<Bitmap?>(null) }
    var failed by remember(conversation, attachment.id) { mutableStateOf(false) }
    var attempt by remember(conversation, attachment.id) { mutableIntStateOf(0) }
    LaunchedEffect(conversation, attachment.id, attempt) {
        failed = false
        try {
            val file = vm.engine.messageAttachment(conversation, attachment)
            bitmap = withContext(Dispatchers.IO) { decodeAttachmentImage(file) }
        } catch (cancel: CancellationException) { throw cancel }
        catch (_: Exception) { failed = true }
    }
    Column(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        val image = bitmap
        when {
            image != null -> Image(image.asImageBitmap(), attachment.name, contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxWidth().heightIn(max = 320.dp).clickable(onClick = preview))
            failed -> Column {
                Text(stringResource(R.string.attachment_preview_failed), color = MaterialTheme.colorScheme.error)
                TextButton(onClick = { attempt++ }) { Text(stringResource(R.string.retry)) }
            }
            else -> Box(Modifier.fillMaxWidth().height(120.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        TextButton(onClick = preview) { Text(attachment.name, maxLines = 2, overflow = TextOverflow.Ellipsis) }
    }
}

@Composable
internal fun AttachmentPreview(vm: HostCheckViewModel, conversation: String, attachment: AttachmentInfo, updatePip: () -> Unit, localFile: File? = null, close: () -> Unit) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    val kind=remember(attachment.mime,attachment.name){attachmentPreviewKind(attachment.mime,attachment.name)}
    var local by remember(conversation,attachment.id){mutableStateOf<File?>(null)}
    var bitmap by remember(conversation,attachment.id){mutableStateOf<Bitmap?>(null)}
    var text by remember(conversation,attachment.id){mutableStateOf<String?>(null)}
    var truncated by remember(conversation,attachment.id){mutableStateOf(false)}
    var failed by remember(conversation,attachment.id){mutableStateOf(false)}
    var attempt by remember(conversation,attachment.id){mutableIntStateOf(0)}
    LaunchedEffect(conversation,attachment.id,attempt,localFile?.path) {
        failed=false
        try {
            val file=localFile?.takeIf { it.isFile } ?: vm.engine.messageAttachment(conversation,attachment)
            withContext(Dispatchers.IO) {
                if(kind==AttachmentPreviewKind.Image) bitmap=decodeAttachmentImage(file)
                if(kind==AttachmentPreviewKind.Text) {
                    val preview=readAttachmentText(file)
                    truncated=preview.truncated;text=preview.text
                }
            }
            local=file
        } catch(cancel:CancellationException){throw cancel}
        catch(_:Exception){failed=true}
    }
    var saving by remember {mutableStateOf<File?>(null)}
    val save=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(attachment.mime)) { uri ->
        vm.externalActivity=false;updatePip()
        val source=saving;saving=null
        if(uri!=null&&source!=null)scope.launch {
            val success=runCatching{withContext(Dispatchers.IO){requireNotNull(context.contentResolver.openOutputStream(uri)).use{destination->source.inputStream().use{it.copyTo(destination)}}}}.isSuccess
            Toast.makeText(context,if(success)R.string.attachment_saved else R.string.attachment_save_failed,Toast.LENGTH_SHORT).show()
        }
    }
    Dialog(onDismissRequest=close,properties=DialogProperties(usePlatformDefaultWidth=false)) {
        Surface(color=KiteColors.Background,modifier=Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().safeDrawingPadding()) {
                Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                    Text(attachment.name,Modifier.weight(1f).padding(start=16.dp),maxLines=2,overflow=TextOverflow.Ellipsis)
                    TextButton(onClick={saving=local;vm.externalActivity=true;updatePip();runCatching{save.launch(attachment.name)}.onFailure{saving=null;vm.externalActivity=false;updatePip()}},enabled=local!=null){Text(stringResource(R.string.attachment_save))}
                    TextButton(onClick=close){Text(stringResource(R.string.dismiss))}
                }
                Box(Modifier.weight(1f).fillMaxWidth().clipToBounds(),contentAlignment=Alignment.Center) {
                    when {
                        failed -> Column(horizontalAlignment=Alignment.CenterHorizontally) {
                            Text(stringResource(R.string.attachment_preview_failed),color=MaterialTheme.colorScheme.error)
                            TextButton(onClick={attempt++}){Text(stringResource(R.string.retry))}
                        }
                        local==null -> CircularProgressIndicator()
                        kind==AttachmentPreviewKind.Image && bitmap!=null -> PreviewImage(requireNotNull(bitmap),attachment.name)
                        kind==AttachmentPreviewKind.Audio -> PreviewAudio(requireNotNull(local))
                        kind==AttachmentPreviewKind.Video -> PreviewVideo(requireNotNull(local))
                        kind==AttachmentPreviewKind.Text -> {
                            val chunks=remember(text){literalChunks(text.orEmpty())}
                            Column(Modifier.fillMaxSize().padding(horizontal=12.dp)) {
                                if(truncated)Text(stringResource(R.string.attachment_text_truncated),fontSize=12.sp,color=KiteColors.Warning)
                                SelectionContainer { LazyColumn(Modifier.fillMaxSize()) { itemsIndexed(chunks,key={index,_->index}) {_,chunk->Text(chunk,fontFamily=FontFamily.Monospace,fontSize=14.sp)} } }
                            }
                        }
                        else -> Text(stringResource(R.string.attachment_preview_unsupported),Modifier.padding(20.dp),color=KiteColors.Muted)
                    }
                }
            }
        }
    }
}

@Composable
private fun PreviewVideo(file: File) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var view by remember(file.path) { mutableStateOf<VideoView?>(null) }
    var failed by remember(file.path) { mutableStateOf(false) }
    DisposableEffect(lifecycle, view) {
        val video = view
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_PAUSE) video?.pause() }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); video?.stopPlayback() }
    }
    if (failed) Text(stringResource(R.string.attachment_preview_failed))
    else AndroidView(factory = { context ->
        VideoView(context).apply {
            view = this
            val controls = MediaController(context)
            controls.setAnchorView(this)
            setMediaController(controls)
            setOnPreparedListener { seekTo(1); controls.show(0) }
            setOnErrorListener { _, _, _ -> failed = true; true }
            setVideoPath(file.path)
        }
    }, modifier = Modifier.fillMaxWidth().height(300.dp))
}

@Composable
private fun PreviewImage(bitmap: Bitmap, name: String) {
    var scale by remember{mutableFloatStateOf(1f)}
    var offset by remember{mutableStateOf(Offset.Zero)}
    val transform=rememberTransformableState { zoom,pan,_->scale=(scale*zoom).coerceIn(1f,8f);offset=if(scale==1f)Offset.Zero else offset+pan}
    Image(bitmap.asImageBitmap(),name,contentScale=ContentScale.Fit,modifier=Modifier.fillMaxSize().graphicsLayer{scaleX=scale;scaleY=scale;translationX=offset.x;translationY=offset.y}.transformable(transform))
}

@Composable
private fun PreviewAudio(file: File) {
    val context=LocalContext.current
    val lifecycle=LocalLifecycleOwner.current.lifecycle
    val manager=remember(context){context.getSystemService(Context.AUDIO_SERVICE) as AudioManager}
    var player by remember{mutableStateOf<MediaPlayer?>(null)}
    var ready by remember{mutableStateOf(false)}
    var playing by remember{mutableStateOf(false)}
    var failed by remember{mutableStateOf(false)}
    var duration by remember{mutableIntStateOf(0)}
    var position by remember{mutableIntStateOf(0)}
    var seeking by remember{mutableStateOf(false)}
    var retry by remember{mutableIntStateOf(0)}
    val attributes=remember{AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build()}
    val focus=remember(manager){AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK).setAudioAttributes(attributes).setOnAudioFocusChangeListener{change->if(change==AudioManager.AUDIOFOCUS_LOSS||change==AudioManager.AUDIOFOCUS_LOSS_TRANSIENT){runCatching{player?.pause()};playing=false}}.build()}
    DisposableEffect(file.path,retry) {
        ready=false;failed=false;playing=false;position=0;duration=0;seeking=false
        val media=MediaPlayer();player=media
        media.setAudioAttributes(attributes)
        media.setOnPreparedListener{duration=it.duration.coerceAtLeast(0);ready=true}
        media.setOnCompletionListener{playing=false;position=duration;manager.abandonAudioFocusRequest(focus)}
        media.setOnErrorListener{_,_,_->failed=true;playing=false;ready=false;manager.abandonAudioFocusRequest(focus);true}
        try{media.setDataSource(file.path);media.prepareAsync()}catch(_:Exception){failed=true}
        val observer=LifecycleEventObserver{_,event->if(event==Lifecycle.Event.ON_PAUSE){if(playing)runCatching{media.pause()};playing=false;manager.abandonAudioFocusRequest(focus)}}
        lifecycle.addObserver(observer)
        onDispose{lifecycle.removeObserver(observer);playing=false;player=null;media.release();manager.abandonAudioFocusRequest(focus)}
    }
    LaunchedEffect(playing,player){while(playing){if(!seeking)position=runCatching{player?.currentPosition?:0}.getOrDefault(0);delay(250)}}
    Column(Modifier.fillMaxWidth().padding(24.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(16.dp)) {
        if(failed){Text(stringResource(R.string.attachment_audio_failed));TextButton(onClick={retry++}){Text(stringResource(R.string.retry))}}
        else {
            Text(previewTime(position)+" / "+previewTime(duration))
            Slider(value=position.toFloat().coerceIn(0f,duration.toFloat().coerceAtLeast(1f)),onValueChange={seeking=true;position=it.toInt()},onValueChangeFinished={runCatching{player?.seekTo(position)};seeking=false},valueRange=0f..duration.toFloat().coerceAtLeast(1f),enabled=ready)
            Button(onClick={
                val media=player?:return@Button
                if(playing){media.pause();playing=false;manager.abandonAudioFocusRequest(focus)}
                else if(manager.requestAudioFocus(focus)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED){if(duration>0&&position>=duration){media.seekTo(0);position=0};media.start();playing=true}
            },enabled=ready){Text(stringResource(if(playing)R.string.attachment_audio_pause else R.string.attachment_audio_play))}
        }
    }
}
