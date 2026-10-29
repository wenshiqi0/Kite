package io.github.wenshiqi0.kite.ui

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.wenshiqi0.kite.KiteApplication
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.data.HostForm
import io.github.wenshiqi0.kite.data.toForm
import io.github.wenshiqi0.kite.media.NativeMedia
import kotlinx.coroutines.launch

class HostCheckViewModel(application: Application) : AndroidViewModel(application) {
    val engine = (application as KiteApplication).engine
    var form by mutableStateOf(HostForm())
        private set
    var showDesktop by mutableStateOf(false)
        private set
    var target by mutableLongStateOf(0L)
        private set
    private var loaded = false
    private var foreground = true
    var localError by mutableStateOf<String?>(null)
        private set

    init {
        viewModelScope.launch {
            engine.state.collect { state ->
                if (state?.initialized == true && !loaded) {
                    loaded = true
                    (state.hosts.find { it.id == state.selectedHost } ?: state.hosts.firstOrNull())?.let { form = it.toForm() }
                }
            }
        }
    }

    fun updateForm(value: HostForm) { form = value }
    fun chooseHost(host: HostRecord) { form = host.toForm() }
    fun clearLocalError() { localError = null; engine.platformError.value = null }
    fun connect() {
        val f = form
        val app = getApplication<Application>()
        val port = f.port.trim().toUShortOrNull()
        val sunshinePort = f.sunshinePort.trim().toUShortOrNull()
        when {
            f.host.isBlank() -> { localError = app.getString(R.string.error_host_required); return }
            f.username.isBlank() -> { localError = app.getString(R.string.error_username_required); return }
            port == null || port == 0.toUShort() -> { localError = app.getString(R.string.error_port_invalid); return }
            sunshinePort == null || sunshinePort.toInt() !in 6..65514 -> { localError = app.getString(R.string.error_sunshine_port); return }
        }
        localError = null
        engine.command {
            val id = engine.core.saveHost(HostRecord(f.id, f.host, port, f.username, f.herdrPath.ifBlank { null }, f.streamHost.ifBlank { null }, sunshinePort))
            form = f.copy(id = id)
            engine.ensureForeground()
            engine.core.connect(id)
        }
    }
    fun disconnect() = engine.command { engine.core.disconnect() }
    fun trust(hostId: String, fingerprint: String) = engine.command { engine.core.trustHost(hostId, fingerprint) }
    fun forget(hostId: String) = engine.command { engine.core.forgetHost(hostId) }
    fun selectAgent(id: String) = engine.command { engine.core.selectAgent(id) }
    fun promptAgent(id: String, text: String) = engine.command { engine.core.promptAgent(id, text) }
    fun approve(pin: String) = engine.command { engine.core.approvePairing(pin) }
    fun takeOver(enabled: Boolean) = engine.command { engine.core.setTakeover(enabled) }
    fun openDesktop() {
        showDesktop = true
    }
    fun closeDesktop() { showDesktop = false; engine.command { engine.core.stopDesktop() } }
    fun videoReady(id: Long) { target = id; if (foreground && showDesktop) retryDesktop() }
    fun videoGone(id: Long) { if (target == id) { target = 0; engine.command { engine.core.stopDesktop() } } }
    fun retryDesktop() {
        val id = target
        if (id != 0L) engine.command { engine.core.startDesktop(id.toULong()) }
    }
    fun onBackground() { foreground = false; if (showDesktop) engine.command { engine.core.stopDesktop() } }
    fun onForeground() { foreground = true; if (showDesktop && target != 0L) retryDesktop() }
    fun sendText(text: String): Boolean = target != 0L && NativeMedia.text(target, text)
    fun rightClick() { if (target != 0L) { NativeMedia.mouseButton(target, 3, true); NativeMedia.mouseButton(target, 3, false) } }
    fun scroll(vertical: Int) { if (target != 0L) NativeMedia.scroll(target, 0, vertical) }
    fun shortcut(key: Int, ctrl: Boolean = false) {
        if (target == 0L) return
        if (ctrl) NativeMedia.key(target, 162, true, 0)
        NativeMedia.key(target, key, true, if (ctrl) 2 else 0)
        NativeMedia.key(target, key, false, if (ctrl) 2 else 0)
        if (ctrl) NativeMedia.key(target, 162, false, 0)
    }
}
