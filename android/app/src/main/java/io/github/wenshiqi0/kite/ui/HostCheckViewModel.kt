package io.github.wenshiqi0.kite.ui

import android.app.Application
import android.graphics.Rect
import android.content.pm.ActivityInfo
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.wenshiqi0.kite.KiteApplication
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.core.AgentPauseStatus
import io.github.wenshiqi0.kite.data.HostForm
import io.github.wenshiqi0.kite.data.toForm
import io.github.wenshiqi0.kite.media.NativeMedia
import kotlinx.coroutines.launch

enum class DesktopLayout { Closed, Floating, Docked, Split, Fullscreen }

class HostCheckViewModel(application: Application) : AndroidViewModel(application) {
    val engine = (application as KiteApplication).engine
    var form by mutableStateOf(HostForm())
        private set
    var layout by mutableStateOf(DesktopLayout.Closed)
        private set
    val showDesktop: Boolean get() = layout != DesktopLayout.Closed
    var settingsVisible by mutableStateOf(false)
    var externalActivity by mutableStateOf(false)
    var inPip by mutableStateOf(false)
        private set
    var pipTransition by mutableStateOf(false)
        private set
    var draft by mutableStateOf("")
    val sending: Boolean get() = engine.state.value?.selectedAgent?.let { it in sendingAgents } ?: false
    var splitRatio by mutableStateOf(.36f)
    var floatingX by mutableStateOf(.90f)
    var floatingY by mutableStateOf(.28f)
    var zoom by mutableStateOf(1f)
    var trackpad by mutableStateOf(true)
    var panMode by mutableStateOf(false)
    var orientationRequest by mutableIntStateOf(ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED)
        private set
    var creatingAgent by mutableStateOf(false)
        private set
    private var wasConnecting = false
    private var lastConnection: ConnectionStatus? = null
    var ctrlHeld by mutableStateOf(false)
        private set
    var metaHeld by mutableStateOf(false)
        private set
    var videoBounds = Rect()
    private var restoreLayout = DesktopLayout.Floating
    private var beforeFullscreen = DesktopLayout.Floating
    private var videoSuspended = false
    private var autoDocked = false
    private var lastStatus: String? = null
    private var autoDesktop = true
    private val seenComputerCalls = mutableSetOf<String>()
    var target by mutableLongStateOf(0L)
        private set
    private var draftAgent: String? = null
    private val agentDrafts = mutableMapOf<String, String>()
    private val sendingAgents = androidx.compose.runtime.mutableStateListOf<String>()
    private var loaded = false
    private var foreground = true
    var localError by mutableStateOf<String?>(null)
        private set

    init {
        viewModelScope.launch {
            engine.state.collect { state ->
                if (state?.initialized == true && !loaded) {
                    loaded = true
                    val saved = state.hosts.find { it.id == state.selectedHost } ?: state.hosts.firstOrNull()
                    if (saved == null) settingsVisible = true else {
                        form = saved.toForm()
                        if (state.connection == ConnectionStatus.DISCONNECTED) connect()
                    }
                }
                if (state != null) {
                    if (lastConnection == ConnectionStatus.CONNECTED && state.connection != ConnectionStatus.CONNECTED) {
                        if (showDesktop) closeDesktop()
                        settingsVisible = true
                    }
                    lastConnection = state.connection
                    if (state.connection == ConnectionStatus.CONNECTING) wasConnecting = true
                    if (wasConnecting && state.connection == ConnectionStatus.CONNECTED) { settingsVisible = false; wasConnecting = false }
                    if (wasConnecting && state.connection == ConnectionStatus.FAILED) { settingsVisible = true; wasConnecting = false }
                    if (state.selectedAgent != draftAgent) {
                        draftAgent?.let { agentDrafts[it] = draft }
                        val old = draftAgent
                        draftAgent = state.selectedAgent
                        draft = draftAgent?.let { agentDrafts[it] } ?: if (old == null) draft else ""
                        if (agentDrafts.size > 32) agentDrafts.remove(agentDrafts.keys.first())
                    }
                    if (!state.desktop.takeover) { ctrlHeld = false; metaHeld = false }
                    val status = state.agents.find { it.id == state.selectedAgent }?.status
                    if (status == "blocked" && lastStatus != "blocked" && layout == DesktopLayout.Floating) {
                        layout = DesktopLayout.Docked; autoDocked = true
                    } else if (status != "blocked" && autoDocked) {
                        layout = DesktopLayout.Floating; autoDocked = false
                    }
                    lastStatus = status
                    val latest = state.conversation.lastOrNull()
                    if (autoDesktop && latest?.tool?.computer == true && !latest.tool!!.complete && status == "working" && seenComputerCalls.add(latest.id) && layout == DesktopLayout.Closed) {
                        layout = DesktopLayout.Floating
                    }
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
        wasConnecting = true
        settingsVisible = true
        fitVideo()
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
    fun prepareAgent() = engine.command { engine.core.ensureAgent() }
    fun startAgent(provider: String, cwd: String) {
        if (creatingAgent) return
        creatingAgent = true
        engine.command { try { engine.core.startAgent(provider, cwd) } finally { creatingAgent = false } }
    }
    fun selectAgent(id: String) = engine.command { engine.core.selectAgent(id) }
    fun sendPrompt(id: String) {
        if (engine.state.value?.selectedAgent != id) return
        val value = draft.trim()
        if (value.isEmpty() || sending) return
        sendingAgents.add(id)
        engine.command {
            try { engine.core.promptAgent(id, value); if (draftAgent == id && draft.trim() == value) draft = "" else if (agentDrafts[id]?.trim() == value) agentDrafts.remove(id) }
            finally { sendingAgents.remove(id) }
        }
    }
    fun terminalKey(id: String, key: String) {
        if (engine.state.value?.selectedAgent == id) engine.command { engine.core.sendAgentKey(id, key) }
    }
    fun respond(id: String, request: String, choice: Int) {
        if (engine.state.value?.selectedAgent != id) return
        engine.command { engine.core.respondApproval(id, request, choice.toUInt()) }
    }
    fun pauseAgent(id: String) {
        val state = engine.state.value ?: return
        if (state.selectedAgent != id) return
        engine.command { engine.core.setAgentPaused(id, state.agentPause == AgentPauseStatus.RUNNING) }
    }
    fun promptAgent(id: String, text: String) = engine.command { engine.core.promptAgent(id, text) }
    fun approve(pin: String) = engine.command { engine.core.approvePairing(pin) }
    fun takeOver(enabled: Boolean) = engine.command { engine.core.setTakeover(enabled) }
    fun openDesktop() {
        autoDesktop = true
        settingsVisible = false
        changeLayout(DesktopLayout.Floating)
    }
    fun changeLayout(value: DesktopLayout, rotate: Boolean = true) {
        if (value == DesktopLayout.Closed) { closeDesktop(); return }
        if (value == DesktopLayout.Floating || value == DesktopLayout.Docked) exitTakeover()
        if (layout != value) {
            if (value == DesktopLayout.Fullscreen) beforeFullscreen = if (layout == DesktopLayout.Closed) DesktopLayout.Floating else layout
            releaseModifiers()
            if (target != 0L) NativeMedia.releaseInput(target)
            panMode = false
        }
        layout = value; autoDocked = false
        if (value == DesktopLayout.Fullscreen && rotate) orientationRequest = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        else if (value != DesktopLayout.Fullscreen) orientationRequest = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    }
    fun returnToChat(landscape: Boolean) {
        changeLayout(if (landscape && beforeFullscreen == DesktopLayout.Split) DesktopLayout.Floating else beforeFullscreen)
    }
    fun rotateDesktop(landscape: Boolean) { orientationRequest = if (landscape) ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT else ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE }
    fun closeDesktop() { orientationRequest = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED; autoDesktop = false; exitTakeover(); layout = DesktopLayout.Closed; fitVideo(); engine.command { engine.core.stopDesktop() } }
    fun fitVideo() { zoom = 1f; panMode = false }
    fun exitTakeover() {
        releaseModifiers()
        if (target != 0L) NativeMedia.releaseInput(target)
        if (engine.state.value?.desktop?.takeover == true) takeOver(false)
    }
    fun setPip(active: Boolean) {
        if (active && !inPip) { restoreLayout = layout; exitTakeover() }
        inPip = active
        pipTransition = false
        if (!active && layout == DesktopLayout.Closed) layout = restoreLayout
    }
    fun preparePip() { pipTransition = true; exitTakeover() }
    fun cancelPip() { pipTransition = false }
    fun videoReady(id: Long) { target = id; if ((foreground || inPip) && showDesktop) retryDesktop() }
    fun videoGone(id: Long) { if (target == id) { target = 0; engine.command { engine.core.stopDesktop() } } }
    fun retryDesktop() {
        val id = target
        if (id != 0L) engine.command { engine.core.startDesktop(id.toULong()) }
    }
    fun onBackground(keepVideo: Boolean = false) { foreground = false; if (showDesktop && !keepVideo) { videoSuspended = true; exitTakeover(); engine.command { engine.core.stopDesktop() } } }
    fun onForeground() { foreground = true; if (showDesktop && target != 0L && (videoSuspended || engine.state.value?.desktop?.status == DesktopStatus.IDLE)) { videoSuspended = false; retryDesktop() } }
    fun modifier(ctrl: Boolean) {
        if (target == 0L) return
        if (ctrl) { ctrlHeld = !ctrlHeld; NativeMedia.key(target, 162, ctrlHeld, 0) }
        else { metaHeld = !metaHeld; NativeMedia.key(target, 91, metaHeld, 0) }
    }
    fun releaseModifiers() {
        if (target != 0L) {
            if (ctrlHeld) NativeMedia.key(target, 162, false, 0)
            if (metaHeld) NativeMedia.key(target, 91, false, 0)
        }
        ctrlHeld = false; metaHeld = false
    }
    fun sendText(text: String): Boolean = target != 0L && NativeMedia.text(target, text)
    fun leftClick() { if (target != 0L) { NativeMedia.mouseButton(target, 1, true); NativeMedia.mouseButton(target, 1, false) } }
    fun rightClick() { if (target != 0L) { NativeMedia.mouseButton(target, 3, true); NativeMedia.mouseButton(target, 3, false) } }
    fun scroll(vertical: Int) { if (target != 0L) NativeMedia.scroll(target, 0, vertical) }
    fun shortcut(key: Int, ctrl: Boolean = false) {
        if (target == 0L) return
        if (ctrl || ctrlHeld) NativeMedia.key(target, 162, true, 0)
        if (metaHeld) NativeMedia.key(target, 91, true, 0)
        val modifiers = (if (ctrl || ctrlHeld) 2 else 0) or (if (metaHeld) 8 else 0)
        NativeMedia.key(target, key, true, modifiers)
        NativeMedia.key(target, key, false, modifiers)
        if (ctrl) NativeMedia.key(target, 162, false, 0)
        releaseModifiers()
    }
}
