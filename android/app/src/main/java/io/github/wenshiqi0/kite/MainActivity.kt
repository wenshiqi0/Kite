package io.github.wenshiqi0.kite

import android.app.PictureInPictureParams
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Rational
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.SideEffect
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.ViewModelProvider
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.ui.KiteWorkspace
import io.github.wenshiqi0.kite.ui.theme.KiteTheme

class MainActivity : ComponentActivity() {
    private lateinit var model: HostCheckViewModel
    private var enteringPip = false
    private var started = false
    private val handler = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        model = ViewModelProvider(this)[HostCheckViewModel::class.java]
        setContent {
            KiteTheme {
                val immersive = model.layout == DesktopLayout.Fullscreen
                SideEffect {
                    val bars = WindowInsetsControllerCompat(window, window.decorView)
                    bars.isAppearanceLightStatusBars = true
                    bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                    if (!model.inPip) {
                        if (immersive) bars.hide(WindowInsetsCompat.Type.systemBars()) else bars.show(WindowInsetsCompat.Type.systemBars())
                    }
                }
                KiteWorkspace(model, ::enterVideoPip, ::refreshPipParams)
            }
        }
    }

    private fun pipEligible(): Boolean = ::model.isInitialized && model.showDesktop && !model.externalActivity &&
        model.engine.state.value?.desktop?.status == DesktopStatus.STREAMING &&
        packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        refreshPipParams()
    }

    private fun params(): PictureInPictureParams {
        val desktop = model.engine.state.value?.desktop
        val ratio = if (desktop != null && desktop.sourceWidth > 0u && desktop.sourceHeight > 0u)
            desktop.sourceWidth.toFloat() / desktop.sourceHeight.toFloat() else 16f / 9f
        val builder = PictureInPictureParams.Builder().setAspectRatio(Rational((ratio.coerceIn(1f / 2.39f, 2.39f) * 1000).toInt(), 1000))
        val source = model.videoBounds
        if (!source.isEmpty && source.left >= 0 && source.top >= 0) builder.setSourceRectHint(source)
        if (Build.VERSION.SDK_INT >= 31) builder.setAutoEnterEnabled(pipEligible()).setSeamlessResizeEnabled(true)
        return builder.build()
    }

    private fun refreshPipParams() {
        if (::model.isInitialized && packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) {
            runCatching { setPictureInPictureParams(params()) }
        }
    }

    private fun enterVideoPip() {
        if (!pipEligible()) { model.closeDesktop(); return }
        model.preparePip()
        enteringPip = true
        if (!runCatching { enterPictureInPictureMode(params()) }.getOrDefault(false)) {
            enteringPip = false
            model.cancelPip()
            model.onBackground(false)
        }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        refreshPipParams()
        if (pipEligible()) {
            model.preparePip()
            enteringPip = true
            if (Build.VERSION.SDK_INT < 31) enterVideoPip()
        }
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        enteringPip = false
        model.setPip(isInPictureInPictureMode)
        if (!isInPictureInPictureMode && started) model.onForeground()
    }

    override fun onStart() {
        super.onStart()
        started = true
        if (::model.isInitialized) model.onForeground()
    }

    override fun onStop() {
        started = false
        if (!isChangingConfigurations && ::model.isInitialized) {
            model.onBackground(isInPictureInPictureMode || enteringPip)
            if (enteringPip) handler.postDelayed({
                if (!started && !isInPictureInPictureMode) { enteringPip = false; model.cancelPip(); model.onBackground(false) }
            }, 1200)
        }
        super.onStop()
    }

    override fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }
}
