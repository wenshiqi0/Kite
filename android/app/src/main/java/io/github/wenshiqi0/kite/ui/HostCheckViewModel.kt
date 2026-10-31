package io.github.wenshiqi0.kite.ui

import android.app.Application
import android.graphics.Rect
import android.net.Uri
import android.content.pm.ActivityInfo
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableFloatStateOf
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
import io.github.wenshiqi0.kite.data.cacheAttachment
import io.github.wenshiqi0.kite.core.UploadInfo
import io.github.wenshiqi0.kite.core.QuestionInfo
import io.github.wenshiqi0.kite.core.QuestionAnswerInfo
import io.github.wenshiqi0.kite.media.NativeMedia
import io.github.wenshiqi0.kite.media.PointerMotion
import io.github.wenshiqi0.kite.core.ClipboardPlatformInfo
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
    var hostEditorVisible by mutableStateOf(false)
    var savingHost by mutableStateOf(false)
        private set
    var externalActivity by mutableStateOf(false)
    var inPip by mutableStateOf(false)
        private set
    var pipTransition by mutableStateOf(false)
        private set
    var draft by mutableStateOf("")
    var copyingAttachments by mutableStateOf(false)
        private set
    val sending: Boolean get() = engine.state.value?.selectedAgent?.let { it in sendingAgents } ?: false
    var splitAdjustment by mutableFloatStateOf(0f)
    var touchpadVisible by mutableStateOf(false)
        private set
    var floatingX by mutableStateOf(.90f)
    var floatingY by mutableStateOf(.28f)
    var zoom by mutableStateOf(1f)
    var heldMouseButtons by mutableIntStateOf(0)
        private set
    var clipboardBusy by mutableStateOf(false)
        private set
    private val pointerMotion = PointerMotion()
    var flushRemoteText:(()->Unit)?=null
    var panMode by mutableStateOf(false)
    var orientationRequest by mutableIntStateOf(ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED)
        private set
    var sessionBusy by mutableStateOf(false)
        private set
    var answeringQuestion by mutableStateOf(false)
        private set
    private val questionDrafts = androidx.compose.runtime.mutableStateMapOf<String, androidx.compose.runtime.snapshots.SnapshotStateMap<String, String>>()
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
                    if (wasConnecting && state.connection == ConnectionStatus.CONNECTED) { if(form.id==state.selectedHost){settingsVisible = false;hostEditorVisible=false}; wasConnecting = false }
                    if (wasConnecting && state.connection == ConnectionStatus.FAILED) { settingsVisible = true; wasConnecting = false }
                    val draftContext = state.selectedSession?.let { "${state.selectedHost}/$it" } ?: state.selectedAgent
                    if (draftContext != draftAgent) {
                        draftAgent?.let { agentDrafts[it] = draft }
                        val old = draftAgent
                        draftAgent = draftContext
                        draft = draftAgent?.let { agentDrafts[it] } ?: if (old == null) draft else ""
                        if (agentDrafts.size > 32) agentDrafts.remove(agentDrafts.keys.first())
                    }
                    if (!state.desktop.takeover) { ctrlHeld = false; metaHeld = false; hideTouchpad();releaseMouseButtons() }
                    val status = state.agents.find { it.id == state.selectedAgent }?.status
                    if ((status == "blocked" && lastStatus != "blocked" || state.question != null) && layout == DesktopLayout.Floating) {
                        layout = DesktopLayout.Docked; autoDocked = true
                    } else if (status != "blocked" && autoDocked) {
                        layout = DesktopLayout.Floating; autoDocked = false
                    }
                    lastStatus = status
                    val latest = engine.timeline.messages.lastOrNull()
                    if (autoDesktop && latest?.tool?.computer == true && !latest.tool!!.complete && status == "working" && seenComputerCalls.add(latest.id) && layout == DesktopLayout.Closed) {
                        layout = DesktopLayout.Floating
                    }
                }
            }
        }
    }

    fun updateForm(value: HostForm) { form = value }
    fun chooseHost(host: HostRecord) { form = host.toForm();clearLocalError();hostEditorVisible=true }
    fun newHost() {
        clearLocalError();form=HostForm();hostEditorVisible=true;settingsVisible=true
    }
    fun clearLocalError() { localError = null; engine.platformError.value = null }
    private fun validatedHost():HostRecord? {
        val f = form
        val app = getApplication<Application>()
        val port = f.port.trim().toUShortOrNull()
        val sunshinePort = f.sunshinePort.trim().toUShortOrNull()
        when {
            f.host.isBlank() -> { localError = app.getString(R.string.error_host_required); return null }
            f.username.isBlank() -> { localError = app.getString(R.string.error_username_required); return null }
            port == null || port == 0.toUShort() -> { localError = app.getString(R.string.error_port_invalid); return null }
            sunshinePort == null || sunshinePort.toInt() !in 6..65514 -> { localError = app.getString(R.string.error_sunshine_port); return null }
        }
        localError = null
        return HostRecord(f.id,f.host,port,f.username,f.herdrPath.ifBlank{null},f.streamHost.ifBlank{null},sunshinePort)
    }
    fun saveHostProfile(){
        val host=validatedHost()?:return
        if(savingHost)return
        savingHost=true
        engine.command {try{val id=engine.core.saveHost(host);if(form.id==host.id){form=host.toForm().copy(id=id);hostEditorVisible=false}}finally{savingHost=false}}
    }
    fun connectSaved(host:HostRecord){
        form=host.toForm();connect()
    }
    fun connect() {
        val host=validatedHost()?:return
        wasConnecting = true
        settingsVisible = true
        fitVideo()
        engine.command {
            val id = engine.core.saveHost(host)
            form = host.toForm().copy(id = id)
            engine.ensureForeground()
            engine.core.connect(id)
        }
    }
    fun disconnect() {flushRemoteText?.invoke();hideTouchpad();engine.command { engine.core.disconnect() }}
    fun trust(hostId: String, fingerprint: String) = engine.command { engine.core.trustHost(hostId, fingerprint) }
    fun forget(hostId: String) = engine.command { engine.core.forgetHost(hostId) }
    fun prepareAgent() = engine.command { engine.core.ensureAgent() }
    fun createSession(provider: String, cwd: String, title: String) {
        if (creatingAgent || sessionBusy) return
        creatingAgent = true; sessionBusy = true
        engine.command { try { engine.core.createSession(provider, cwd, title) } finally { creatingAgent = false; sessionBusy = false } }
    }
    fun openSession(id: String) {
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.openSession(id) } finally { sessionBusy = false } }
    }
    fun resumeSession(id: String) {
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.resumeSession(id) } finally { sessionBusy = false } }
    }
    fun archiveSession(id: String, archived: Boolean) {
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.archiveSession(id, archived) } finally { sessionBusy = false } }
    }
    fun questionValues(request: String): Map<String,String> = questionDrafts[request] ?: emptyMap()
    fun setQuestionValue(request: String, field: String, value: String) {
        val values = questionDrafts.getOrPut(request) { androidx.compose.runtime.mutableStateMapOf() }
        values[field] = value
        if (questionDrafts.size > 16) questionDrafts.keys.firstOrNull { it != request }?.let(questionDrafts::remove)
    }
    fun answerQuestion(question: QuestionInfo) {
        val id = engine.state.value?.selectedAgent ?: return
        if (answeringQuestion || engine.state.value?.question?.id != question.id) return
        val answers = question.fields.map { QuestionAnswerInfo(it.id, questionDrafts[question.id]?.get(it.id).orEmpty()) }
        answeringQuestion = true
        engine.command { try { engine.core.answerQuestion(id, question.id, answers); questionDrafts.remove(question.id) } finally { answeringQuestion = false } }
    }
    fun selectAgent(id: String) = engine.command { engine.core.selectAgent(id) }
    fun sendPrompt(id: String) {
        if (engine.state.value?.selectedAgent != id) return
        val value = draft.trim()
        val context = draftAgent
        if ((value.isEmpty() && engine.state.value?.composer?.uploads.isNullOrEmpty()) || sending || copyingAttachments) return
        sendingAgents.add(id)
        engine.command {
            try { engine.core.promptAgent(id, value); if (draftAgent == context && draft.trim() == value) draft = "" else if (context != null && agentDrafts[context]?.trim() == value) agentDrafts.remove(context) }
            finally { sendingAgents.remove(id) }
        }
    }
    fun selectModel(id: String, model: String, effort: String) = engine.command { engine.core.setChatModel(id, model, effort) }
    fun reloadComposer() = engine.command { engine.core.reloadComposer() }
    fun attachFiles(uris: List<Uri>) {
        val id = engine.state.value?.selectedAgent ?: return
        if (copyingAttachments || sending || uris.isEmpty()) return
        copyingAttachments = true
        engine.command {
            try {
                for (uri in uris.take(5)) {
                    val file = cacheAttachment(getApplication(), uri)
                    engine.core.uploadAttachment(id, file.path, file.name, file.mime, null)
                    java.io.File(file.path).delete()
                }
            } finally { copyingAttachments = false }
        }
    }
    fun retryAttachment(file: UploadInfo) {
        val id = engine.state.value?.selectedAgent ?: return
        engine.command { engine.core.uploadAttachment(id, file.localPath, file.file.name, file.file.mime, file.file.id) }
    }
    fun removeAttachment(file: UploadInfo) {
        val id = engine.state.value?.selectedAgent ?: return
        engine.command { engine.core.removeAttachment(id, file.file.id); java.io.File(file.localPath).delete() }
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
    fun takeOver(enabled: Boolean) {
        if(!enabled){flushRemoteText?.invoke();hideTouchpad();releaseMouseButtons();releaseModifiers()}
        engine.command { engine.core.setTakeover(enabled) }
    }
    fun openDesktop() {
        autoDesktop = true
        settingsVisible = false
        hostEditorVisible = false
        changeLayout(DesktopLayout.Floating)
    }
    fun changeLayout(value: DesktopLayout, rotate: Boolean = true) {
        if (value == DesktopLayout.Closed) { closeDesktop(); return }
        if (value == DesktopLayout.Floating || value == DesktopLayout.Docked) exitTakeover()
        if (layout != value) {
            if (value == DesktopLayout.Fullscreen) beforeFullscreen = if (layout == DesktopLayout.Closed) DesktopLayout.Floating else layout
            releaseMouseButtons()
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
        flushRemoteText?.invoke()
        hideTouchpad()
        releaseMouseButtons()
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
    fun videoGone(id: Long) { if (target == id) { releaseMouseButtons(); target = 0; engine.command { engine.core.stopDesktop() } } }
    fun retryDesktop() {
        val id = target
        if (id != 0L) engine.command { engine.core.startDesktop(id.toULong()) }
    }
    fun onBackground(keepVideo: Boolean = false) { releaseMouseButtons(); foreground = false; if (showDesktop && !keepVideo) { videoSuspended = true; exitTakeover(); engine.command { engine.core.stopDesktop() } } }
    fun onForeground() { foreground = true; if (showDesktop && target != 0L && (videoSuspended || engine.state.value?.desktop?.status == DesktopStatus.IDLE)) { videoSuspended = false; retryDesktop() } }
    fun modifier(ctrl: Boolean) {
        flushRemoteText?.invoke()
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
    fun pasteSynchronized(): Boolean {
        if (!engine.clipboardSyncEnabled || clipboardBusy || !foreground || target == 0L || engine.state.value?.desktop?.takeover != true) return false
        val clipboard=getApplication<Application>().getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
        val text=clipboard.primaryClip?.takeIf {it.itemCount>0}?.getItemAt(0)?.text?.toString() ?: return false
        if(text.toByteArray().size>256*1024)return false
        val host = engine.state.value?.selectedHost ?: return false
        val destination = target
        clipboardBusy = true
        engine.command { try {
            val platform = engine.core.writeClipboard(host, text)
            if (foreground && engine.clipboardSyncEnabled && target == destination && engine.state.value?.selectedHost == host && engine.state.value?.desktop?.takeover == true) {
                releaseModifiers()
                val modifier = if (platform == ClipboardPlatformInfo.MACOS) 91 else 162
                NativeMedia.key(destination, modifier, true, 0)
                NativeMedia.key(destination, 86, true, if (modifier == 91) 8 else 2)
                NativeMedia.key(destination, 86, false, if (modifier == 91) 8 else 2)
                NativeMedia.key(destination, modifier, false, 0)
            }
        } finally { clipboardBusy = false } }
        return true
    }
    fun hideTouchpad() { if(touchpadVisible){releaseMouseButtons();touchpadVisible=false} }
    fun toggleTouchpad() {
        if(touchpadVisible)hideTouchpad()
        else if(engine.state.value?.desktop?.takeover==true){releaseMouseButtons();touchpadVisible=true}
    }
    fun mouseButton(button: Int, down: Boolean) {
        if(down)flushRemoteText?.invoke()
        if (down && (!foreground || engine.state.value?.desktop?.takeover != true)) return
        val mask = 1 shl button
        if (target != 0L) NativeMedia.mouseButton(target, button, down)
        heldMouseButtons = if (down) heldMouseButtons or mask else heldMouseButtons and mask.inv()
    }
    fun releaseMouseButtons() { for (button in listOf(1, 2, 3)) if (heldMouseButtons and (1 shl button) != 0) mouseButton(button, false); pointerMotion.reset() }
    fun moveMouse(dx: Float, dy: Float, density: Float) {
        if (target == 0L || engine.state.value?.desktop?.takeover != true) return
        val d = pointerMotion.move(dx, dy, density)
        if (d.x != 0 || d.y != 0) NativeMedia.mouseMove(target, d.x, d.y)
    }
    fun wheelMouse(dy: Float, density: Float) {
        if (target == 0L || engine.state.value?.desktop?.takeover != true) return
        val amount = pointerMotion.scroll(dy, density)
        if (amount != 0) NativeMedia.scroll(target, 0, amount)
    }
    fun leftClick() { flushRemoteText?.invoke();if (target != 0L) { NativeMedia.mouseButton(target, 1, true); NativeMedia.mouseButton(target, 1, false) } }
    fun rightClick() { flushRemoteText?.invoke();if (target != 0L) { NativeMedia.mouseButton(target, 3, true); NativeMedia.mouseButton(target, 3, false) } }
    fun scroll(vertical: Int): Boolean {
        if (target == 0L || !foreground || engine.state.value?.desktop?.takeover != true) return false
        flushRemoteText?.invoke()
        return NativeMedia.scroll(target, 0, vertical)
    }
    fun desktopChord(modifierKey:Int,key:Int,mask:Int) {
        flushRemoteText?.invoke()
        if(target==0L||engine.state.value?.desktop?.takeover!=true)return
        NativeMedia.key(target,modifierKey,true,0)
        NativeMedia.key(target,key,true,mask);NativeMedia.key(target,key,false,mask)
        NativeMedia.key(target,modifierKey,false,0)
    }
    fun shortcut(key: Int, ctrl: Boolean = false) {
        flushRemoteText?.invoke()
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
