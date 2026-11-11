package io.github.wenshiqi0.kite.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.os.Build
import android.view.Surface
import android.view.TextureView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileInputStream
import java.lang.ref.WeakReference
import kotlin.math.roundToInt

internal data class AttachmentVideoInfo(val poster: Bitmap?, val durationMs: Int, val aspect: Float)

internal fun readAttachmentVideo(file: File): AttachmentVideoInfo {
    val reader = MediaMetadataRetriever()
    try {
        FileInputStream(file).use { reader.setDataSource(it.fd) }
        val width = reader.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 640
        val height = reader.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 360
        val duration = reader.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()?.coerceIn(0, Int.MAX_VALUE.toLong())?.toInt() ?: 0
        val scale = minOf(1f, 960f / maxOf(width, height).coerceAtLeast(1))
        val poster = if (Build.VERSION.SDK_INT >= 27) reader.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC,
            (width * scale).roundToInt().coerceAtLeast(1), (height * scale).roundToInt().coerceAtLeast(1))
        else reader.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
        return AttachmentVideoInfo(poster, duration, poster?.let { it.width.toFloat() / it.height } ?: (width.toFloat() / height.coerceAtLeast(1)))
    } finally { reader.release() }
}

/** Texture-backed playback keeps the poster and controls in the same dialog window. */
private class AttachmentVideoPlayer(private val file: File, context: Context) : TextureView.SurfaceTextureListener {
    var ready by mutableStateOf(false)
        private set
    var playing by mutableStateOf(false)
        private set
    var failed by mutableStateOf(false)
        private set
    var hasFrame by mutableStateOf(false)
        private set
    var duration by mutableIntStateOf(0)
        private set
    var position by mutableIntStateOf(0)
        private set
    var aspect by mutableFloatStateOf(16f / 9f)
        private set
    private var released = false
    private var sourceSet = false
    private var surface: Surface? = null
    private var textureView: WeakReference<TextureView>? = null
    private val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build()
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(attributes)
        .setOnAudioFocusChangeListener { change ->
            if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) pause()
        }.build()
    private val player = MediaPlayer().apply {
        setAudioAttributes(attributes)
        setOnPreparedListener {
            if (!released) {
                duration = it.duration.coerceAtLeast(0)
                ready = true
                if (it.videoWidth > 0 && it.videoHeight > 0) aspect = it.videoWidth.toFloat() / it.videoHeight
                it.seekTo(1)
            }
        }
        setOnVideoSizeChangedListener { _, width, height -> if (width > 0 && height > 0) { aspect = width.toFloat() / height; textureView?.get()?.let(::transform) } }
        setOnCompletionListener { playing = false; position = duration; manager.abandonAudioFocusRequest(focus) }
        setOnErrorListener { _, _, _ -> failed = true; ready = false; playing = false; manager.abandonAudioFocusRequest(focus); true }
    }
    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
        if (released) return
        surface?.release()
        surface = Surface(texture)
        try {
            player.setSurface(surface)
            if (!sourceSet) {
                FileInputStream(file).use { player.setDataSource(it.fd) }
                sourceSet = true
                player.prepareAsync()
            } else if (ready) player.seekTo(position)
        } catch (_: Exception) { failed = true }
    }
    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) { textureView?.get()?.let(::transform) }
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) { if (!released && ready) hasFrame = true }
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        if (!released) { pause(); player.setSurface(null) }
        surface?.release(); surface = null
        return true
    }
    fun transform(view: TextureView) {
        if (view.width <= 0 || view.height <= 0) return
        val viewport = view.width.toFloat() / view.height
        val sx = if (aspect < viewport) aspect / viewport else 1f
        val sy = if (aspect > viewport) viewport / aspect else 1f
        view.setTransform(Matrix().apply { setScale(sx, sy, view.width / 2f, view.height / 2f) })
    }
    fun attach(view: TextureView) { textureView = WeakReference(view); view.surfaceTextureListener = this }
    fun toggle() {
        if (!ready || released) return
        if (playing) pause()
        else if (manager.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            try {
                if (position >= duration && duration > 0) { player.seekTo(0); position = 0 }
                player.start(); playing = true
            } catch (_: Exception) { failed = true }
        }
    }
    fun pause() {
        if (!released && ready) runCatching { player.pause() }
        playing = false
        manager.abandonAudioFocusRequest(focus)
    }
    fun seek(value: Int) {
        if (!released && ready) { position = value.coerceIn(0, duration); runCatching { player.seekTo(position) } }
    }
    fun updatePosition() { if (!released && ready) position = runCatching { player.currentPosition }.getOrDefault(position) }
    fun release() {
        if (released) return
        pause(); released = true
        player.release(); surface?.release(); surface = null
    }
}

@Composable
internal fun PreviewVideo(file: File) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var attempt by remember(file.path) { mutableIntStateOf(0) }
    val controller = remember(context, file.path, attempt) { AttachmentVideoPlayer(file, context) }
    var info by remember(file.path) { mutableStateOf<AttachmentVideoInfo?>(null) }
    var seeking by remember(controller) { mutableStateOf(false) }
    var position by remember(controller) { mutableIntStateOf(0) }
    LaunchedEffect(file.path) {
        try { info = withContext(Dispatchers.IO) { readAttachmentVideo(file) } }
        catch (cancel: CancellationException) { throw cancel }
        catch (_: Exception) { /* Playback may still work when a container has no poster. */ }
    }
    DisposableEffect(controller, lifecycle) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_PAUSE) controller.pause() }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); controller.release() }
    }
    LaunchedEffect(controller, controller.playing) {
        while (true) {
            controller.updatePosition()
            if (!seeking) position = controller.position
            delay(200)
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
    val maximumVideoHeight = (maxHeight - 180.dp).coerceIn(90.dp, 360.dp)
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val aspect = if (controller.ready) controller.aspect else info?.aspect ?: 16f / 9f
            Box(Modifier.fillMaxWidth().height((maxWidth / aspect).coerceAtMost(maximumVideoHeight)).background(Color.Black), contentAlignment = Alignment.Center) {
                key(controller) {
                    AndroidView(factory = { TextureView(it).also(controller::attach) },
                        update = controller::transform, modifier = Modifier.fillMaxSize())
                }
                if (!controller.hasFrame) info?.poster?.let { Image(it.asImageBitmap(), stringResource(R.string.attachment_video_poster), contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize()) }
                val playLabel = stringResource(R.string.attachment_video_play)
                if (!controller.playing && !controller.failed) FilledIconButton(onClick = controller::toggle, enabled = controller.ready, modifier = Modifier.size(64.dp).semantics { contentDescription = playLabel }) {
                    ProductIcon(Mark.Play, KiteColors.OnAccent, Modifier.size(32.dp))
                }
            }
        }
        if (controller.failed) {
            Text(stringResource(R.string.attachment_video_failed), color = MaterialTheme.colorScheme.error)
            TextButton(onClick = { attempt++ }) { Text(stringResource(R.string.retry)) }
        } else {
            val duration = controller.duration.coerceAtLeast(info?.durationMs ?: 0)
            Text(previewTime(position) + " / " + previewTime(duration), color = KiteColors.Muted)
            Slider(value = position.toFloat().coerceIn(0f, duration.toFloat().coerceAtLeast(1f)),
                onValueChange = { seeking = true; position = it.toInt() },
                onValueChangeFinished = { controller.seek(position); seeking = false },
                valueRange = 0f..duration.toFloat().coerceAtLeast(1f), enabled = controller.ready)
            Button(onClick = controller::toggle, enabled = controller.ready) {
                ProductIcon(if (controller.playing) Mark.Pause else Mark.Play, KiteColors.OnAccent, Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(stringResource(if (controller.playing) R.string.attachment_video_pause else R.string.attachment_video_play))
            }
        }
    }
    }
}
