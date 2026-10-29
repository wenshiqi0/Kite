package io.github.wenshiqi0.kite.data

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat
import io.github.wenshiqi0.kite.ConnectionService
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.CoreObserver
import io.github.wenshiqi0.kite.core.CoreSnapshot
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.core.KiteCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File

/** One process-wide Core and observer; no Activity-owned domain state. */
class CoreController(private val context: Context) {
    private val main = Handler(Looper.getMainLooper())
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val current = MutableStateFlow<CoreSnapshot?>(null)
    val state = current.asStateFlow()
    val platformError = MutableStateFlow<String?>(null)
    private var lastRevision: ULong? = null
    private var foregroundWanted = false
    private var foregroundRequested = false
    private var service: ConnectionService? = null

    val core = KiteCore(DeviceKeyStore(context), object : CoreObserver {
        override fun stateChanged(snapshot: CoreSnapshot) {
            main.post {
                if (lastRevision == null || snapshot.revision > lastRevision!!) {
                    lastRevision = snapshot.revision
                    current.value = snapshot
                    refreshForeground(snapshot)
                }
            }
        }
    })

    fun initialize() = command {
        core.initialize(File(context.filesDir, "core").absolutePath, "kite@${Build.MODEL}", HostStore(context).legacyData())
    }

    fun command(action: suspend () -> Unit) {
        scope.launch {
            try { action() } catch (e: Exception) {
                platformError.value = context.getString(R.string.error_action) + "\n" + (e.message ?: e.javaClass.simpleName)
                current.value?.let(::refreshForeground)
            }
        }
    }

    fun ensureForeground() {
        foregroundWanted = true
        if (!foregroundRequested) {
            foregroundRequested = true
            try {
                ContextCompat.startForegroundService(context, Intent(context, ConnectionService::class.java))
            } catch (e: Exception) {
                foregroundRequested = false
                platformError.value = e.javaClass.simpleName
            }
        }
    }

    fun foregroundReady(instance: ConnectionService) {
        service = instance
        if (!foregroundWanted) instance.stopSelf()
        else instance.setVideoActive(current.value?.desktop?.status == DesktopStatus.STREAMING)
    }

    fun foregroundStopped(instance: ConnectionService) {
        if (service === instance) {
            service = null
            foregroundRequested = false
            if (foregroundWanted) ensureForeground()
        }
    }

    fun foregroundTimedOut() {
        foregroundWanted = false
        command { core.disconnect() }
    }

    private fun refreshForeground(snapshot: CoreSnapshot) {
        service?.setVideoActive(snapshot.desktop.status == DesktopStatus.STREAMING || snapshot.desktop.status == DesktopStatus.STARTING)
        if (snapshot.connection == ConnectionStatus.CONNECTING || snapshot.connection == ConnectionStatus.CONNECTED) {
            ensureForeground()
        } else {
            foregroundWanted = false
            // Do not cancel a pending startForegroundService before onCreate promotes it.
            service?.stopSelf()
        }
    }
}
