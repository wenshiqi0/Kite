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
import io.github.wenshiqi0.kite.core.FailureKind
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.core.AgentPauseStatus
import io.github.wenshiqi0.kite.data.HostForm
import io.github.wenshiqi0.kite.data.toForm
import io.github.wenshiqi0.kite.data.cacheAttachment
import io.github.wenshiqi0.kite.core.UploadInfo
import io.github.wenshiqi0.kite.core.QuestionInfo
import io.github.wenshiqi0.kite.core.SessionInfo
import io.github.wenshiqi0.kite.core.QuestionAnswerInfo
import io.github.wenshiqi0.kite.media.DesktopInput
import io.github.wenshiqi0.kite.media.PointerMotion
import io.github.wenshiqi0.kite.core.ClipboardPlatformInfo
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout

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
    var updatingMemoryLimit by mutableStateOf(false)
        private set
    var externalActivity by mutableStateOf(false)
    var inPip by mutableStateOf(false)
        private set
    var pipTransition by mutableStateOf(false)
        private set
    var draft by mutableStateOf("")
    var copyingAttachments by mutableStateOf(false)
        private set
    val sending: Boolean get() = engine.state.value?.selectedAgent?.let { "${engine.state.value?.selectedHost}/$it" in sendingAgents } ?: false
    val interrupting: Boolean get() = engine.state.value?.selectedAgent?.let { "${engine.state.value?.selectedHost}/$it" in interruptingAgents } ?: false
    val sessionStartup: SessionStartupPhase get() {
        val state = engine.state.value
        val agent = state?.agents?.find { it.id == state.selectedAgent }
        val failed = state?.failure?.kind == FailureKind.AGENT_START || state?.composer?.let { !it.ready && !it.loading && it.details.isNotBlank() } == true
        return sessionStartupPhase(creatingAgent, state?.connection == ConnectionStatus.CONNECTED, agent, state?.composer?.ready == true, failed, state?.providers?.any { it.id == agent?.agent && it.managedChat } == true)
    }
    val submissionLocked: Boolean get() {
        val state = engine.state.value
        val agent = state?.agents?.find { it.id == state.selectedAgent }
        return sessionBusy || !sessionStartup.canSubmit || state?.providers?.none { it.id == agent?.agent } != false
    }
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
    var sessionToDelete by mutableStateOf<Pair<String, SessionInfo>?>(null)
        private set
    var releasingArchived by mutableStateOf(false)
        private set
    var archiveReleaseStatus by mutableStateOf<String?>(null)
        private set
    var archiveReleaseHost by mutableStateOf<String?>(null)
        private set
    var answeringQuestion by mutableStateOf(false)
        private set
    private val questionDrafts = androidx.compose.runtime.mutableStateMapOf<String, androidx.compose.runtime.snapshots.SnapshotStateMap<String, String>>()
    var creatingAgent by mutableStateOf(false)
        private set
    private var pendingConnectHost: String? = null
    private var lastConnection: ConnectionStatus? = null
    private var lastHost: String? = null
    var ctrlHeld by mutableStateOf(false)
        private set
    var metaHeld by mutableStateOf(false)
        private set
    var videoBounds = Rect()
    private var restoreLayout = DesktopLayout.Floating
    private var beforeFullscreen = DesktopLayout.Floating
    private var videoSuspended = false
    private var videoHost: String? = null
    private var autoDocked = false
    private var lastStatus: String? = null
    private var autoDesktop = true
    private val seenComputerCalls = mutableSetOf<String>()
    var target by mutableLongStateOf(0L)
        private set
    private var draftAgent: String? = null
    private val agentDrafts = mutableMapOf<String, String>()
    private val sendingAgents = androidx.compose.runtime.mutableStateListOf<String>()
    private val interruptingAgents = androidx.compose.runtime.mutableStateListOf<String>()
    private var loaded = false
    private var notificationRoute: Job? = null
    private var noticeRouteSerial=0L
    private var foreground = true
    var localError by mutableStateOf<String?>(null)
        private set

    init {
        viewModelScope.launch {
            engine.state.collect { state ->
                if (state?.initialized == true && !loaded) {
                    loaded = true
                    val saved = state.hosts.find { it.id == state.selectedHost } ?: state.hosts.firstOrNull()
                    if (saved != null) {
                        form = saved.toForm()
                        if (state.connection == ConnectionStatus.DISCONNECTED && !engine.notificationNavigationActive) engine.command { engine.core.restoreConnection() }
                    }
                }
                if (state != null) {
                    if (lastHost != null && state.selectedHost != lastHost) {
                        layout = DesktopLayout.Closed;orientationRequest = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
                        videoSuspended = false;fitVideo();hideTouchpad()
                    }
                    lastHost = state.selectedHost
                    if (lastConnection == ConnectionStatus.CONNECTED && state.connection == ConnectionStatus.RECONNECTING) {
                        releaseMouseButtons()
                        videoSuspended = showDesktop
                    }
                    if (lastConnection == ConnectionStatus.CONNECTED && state.connection != ConnectionStatus.CONNECTED && state.connection != ConnectionStatus.RECONNECTING) {
                        if (showDesktop) closeDesktop()
                    }
                    if (lastConnection == ConnectionStatus.RECONNECTING && state.connection == ConnectionStatus.CONNECTED && videoSuspended && foreground && target != 0L && showDesktop) {
                        videoSuspended = false
                        retryDesktop()
                    }

                    lastConnection = state.connection
                    if (pendingConnectHost == state.selectedHost && state.connection == ConnectionStatus.CONNECTED) {
                        settingsVisible=false;hostEditorVisible=false;pendingConnectHost=null
                    }
                    if (pendingConnectHost == state.selectedHost && state.connection == ConnectionStatus.FAILED) pendingConnectHost=null
                    val draftContext = state.selectedSession?.let { "${state.selectedHost}/$it" } ?: state.selectedAgent?.let { "${state.selectedHost}/$it" }
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
                    if (autoDesktop && latest?.tool?.computer == true && !latest.tool!!.complete && status == "working" && seenComputerCalls.add("$draftContext/${latest.id}") && layout == DesktopLayout.Closed) {
                        layout = DesktopLayout.Floating
                    }
                }
            }
        }
    }

    fun updateForm(value: HostForm) { form = value }
    fun openAgentNotification(id: String) {
        if(engine.stagedNoticeId==id&&notificationRoute?.isActive==true)return
        notificationRoute?.cancel()
        val serial=++noticeRouteSerial
        engine.stageNotification(id)
        notificationRoute = viewModelScope.launch {
            try {
                withTimeout(180_000) {
                    val initial=engine.state.filterNotNull().first{it.initialized}
                    val host=engine.core.notificationHost(id)
                    form=initial.hosts.first{it.id==host}.toForm()
                    if(showDesktop)closeDesktop()
                    autoDesktop=false
                    if(initial.selectedHost!=host||initial.connection!=ConnectionStatus.CONNECTED){
                        engine.ensureForeground()
                        engine.core.connect(host)
                    }
                    val connected=engine.state.filterNotNull().first{
                        it.selectedHost==host&&(it.connection==ConnectionStatus.CONNECTED||it.connection==ConnectionStatus.FAILED)
                    }
                    check(connected.connection==ConnectionStatus.CONNECTED){getApplication<Application>().getString(R.string.notice_connect_failed)}
                    val notice=engine.core.loadNotification(id)
                    engine.core.openSession(host, notice.sessionId)
                    engine.core.acknowledgeNotification(id,true,true)
                    engine.agentNotifications.cancel(id)
                    settingsVisible=false;hostEditorVisible=false
                }
            } catch(e:TimeoutCancellationException){localError=getApplication<Application>().getString(R.string.notice_open_failed)}
            catch(e:CancellationException){throw e}
            catch(e:Exception){localError=getApplication<Application>().getString(R.string.notice_open_failed)+"\n"+(e.message?:e.javaClass.simpleName)}
            finally {
                if(noticeRouteSerial==serial){engine.stagedNoticeId=null;engine.notificationNavigationActive=false;engine.refreshAgentNotifications()}
            }
        }
    }
    fun openMachineSettings(hostId: String? = engine.state.value?.selectedHost) {
        engine.state.value?.hosts?.find { it.id == hostId }?.let { form=it.toForm() }
        clearLocalError();hostEditorVisible=true;settingsVisible=true
    }
    fun chooseHost(host: HostRecord) { form = host.toForm();clearLocalError();hostEditorVisible=true }
    fun openResourceDetails(hostId: String) {
        viewModelScope.launch {
            val state = engine.state.filterNotNull().first { it.initialized }
            state.hosts.find { it.id == hostId }?.let { chooseHost(it); settingsVisible = true }
        }
    }
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
        flushRemoteText?.invoke();hideTouchpad();fitVideo()
        form=host.toForm();pendingConnectHost=host.id
        engine.command {
            engine.ensureForeground()
            engine.core.connect(host.id)
            if(engine.state.value?.selectedHost==host.id&&engine.state.value?.connection==ConnectionStatus.CONNECTED){settingsVisible=false;hostEditorVisible=false;pendingConnectHost=null}
        }
    }
    fun openHostDesktop(host: String) = engine.command {
        engine.core.selectHost(host)
        withTimeout(5_000) { engine.state.filterNotNull().first { it.selectedHost == host } }
        settingsVisible=false;hostEditorVisible=false
        openDesktop()
    }
    fun connect() {
        val host=validatedHost()?:return
        settingsVisible = true
        fitVideo()
        engine.command {
            val id = engine.core.saveHost(host)
            form = host.toForm().copy(id = id)
            pendingConnectHost=id
            engine.ensureForeground()
            engine.core.connect(id)
        }
    }
    fun disconnect() {val host=engine.state.value?.selectedHost?:return;disconnectHost(host)}
    fun disconnectHost(host: String) {flushRemoteText?.invoke();hideTouchpad();engine.command { engine.core.disconnectHost(host) }}
    fun setSessionSort(order: io.github.wenshiqi0.kite.core.SessionSortOrder) {
        engine.command { engine.core.setWorkspaceSort(order) }
    }
    fun trust(hostId: String, fingerprint: String) = engine.command { engine.core.trustHost(hostId, fingerprint) }
    fun forget(hostId: String) = engine.command { engine.core.forgetHost(hostId) }
    fun prepareAgent() { val host=engine.state.value?.selectedHost?:return; engine.command { engine.core.ensureAgent(host) } }
    fun createSession(provider: String, cwd: String, title: String, hostId: String? = engine.state.value?.selectedHost) {
        val host=hostId?:return
        if (creatingAgent || sessionBusy) return
        if (!canStartResourceAgent(engine.state.value?.hostConnections?.find { it.hostId == host }?.resources)) {
            localError = getApplication<Application>().getString(R.string.resource_new_sessions_paused)
            return
        }
        creatingAgent = true; sessionBusy = true
        engine.command { try { engine.core.createSession(host, provider, cwd, title) } finally { creatingAgent = false; sessionBusy = false } }
    }
    fun openSession(id: String, hostId: String? = engine.state.value?.selectedHost) {
        val host=hostId?:return
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.openWorkspaceSession(host, id) } finally { sessionBusy = false } }
    }
    fun resumeSession(id: String) {
        val host=engine.state.value?.selectedHost?:return
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.resumeSession(host, id) } finally { sessionBusy = false } }
    }
    fun archiveSession(id: String, archived: Boolean, hostId: String? = engine.state.value?.selectedHost) {
        val host=hostId?:return
        if (sessionBusy) return
        sessionBusy = true
        engine.command { try { engine.core.archiveSession(host, id, archived) } finally { sessionBusy = false } }
    }
    fun requestSessionDeletion(session: SessionInfo, hostId: String? = engine.state.value?.selectedHost) {
        val host = hostId ?: return
        if (!sessionBusy) sessionToDelete = host to session
    }
    fun dismissSessionDeletion() { sessionToDelete = null }
    fun deleteSession() {
        val (host, session) = sessionToDelete ?: return
        if (sessionBusy) return
        sessionToDelete = null
        sessionBusy = true
        engine.command { try { engine.core.deleteSession(host, session.id) } finally { sessionBusy = false } }
    }
    fun releaseArchivedSessions(hostId: String? = engine.state.value?.selectedHost) {
        if (sessionBusy) return
        if (releasingArchived) return
        val host = hostId ?: return
        releasingArchived = true
        archiveReleaseStatus = null
        archiveReleaseHost = host
        engine.command {
            try {
                val result = engine.core.releaseArchivedSessions(host)
                if (engine.state.value?.selectedHost == host) {
                    archiveReleaseStatus = getApplication<Application>().getString(
                        R.string.archived_release_result, result.reclaimed.toInt(), result.retained.toInt(), result.failed.toInt())
                }
            } finally { releasingArchived = false }
        }
    }
    fun updateAgentMemoryLimit(bytes: ULong) {
        if (updatingMemoryLimit) return
        val hostId = form.id
        updatingMemoryLimit = true
        engine.command {
            try { engine.core.setAgentMemoryLimit(hostId, bytes) }
            finally { updatingMemoryLimit = false }
        }
    }
    private fun questionDraftKey(request: String) = "${engine.state.value?.selectedHost}/${engine.state.value?.selectedSession}/$request"
    fun questionValues(request: String): Map<String,String> = questionDrafts[questionDraftKey(request)] ?: emptyMap()
    fun setQuestionValue(request: String, field: String, value: String) {
        val key = questionDraftKey(request)
        val values = questionDrafts.getOrPut(key) { androidx.compose.runtime.mutableStateMapOf() }
        values[field] = value
        if (questionDrafts.size > 16) questionDrafts.keys.firstOrNull { it != key }?.let(questionDrafts::remove)
    }
    fun answerQuestion(question: QuestionInfo) {
        val host=engine.state.value?.selectedHost?:return
        val id = engine.state.value?.selectedAgent ?: return
        if (answeringQuestion || engine.state.value?.question?.id != question.id) return
        val key = questionDraftKey(question.id)
        val answers = question.fields.map { QuestionAnswerInfo(it.id, questionDrafts[key]?.get(it.id).orEmpty()) }
        answeringQuestion = true
        engine.command { try { engine.core.answerQuestion(host,id, question.id, answers); questionDrafts.remove(key) } finally { answeringQuestion = false } }
    }
    fun selectAgent(id: String) { val host=engine.state.value?.selectedHost?:return; engine.command { engine.core.selectAgent(host,id) } }
    fun sendPrompt(id: String) {
        val host=engine.state.value?.selectedHost?:return
        if (submissionLocked || interrupting || engine.state.value?.selectedAgent != id) return
        val value = draft.trim()
        val context = draftAgent
        if ((value.isEmpty() && engine.state.value?.composer?.uploads.isNullOrEmpty() && engine.state.value?.composer?.skills?.selected.isNullOrEmpty()) || sending || copyingAttachments || skillBusy) return
        val sentFiles = engine.state.value?.composer?.uploads.orEmpty().map { it.localPath }.filter { it.isNotBlank() }.distinct()
        sendingAgents.add("$host/$id")
        engine.command {
            try {
                if (submissionLocked || engine.state.value?.selectedAgent != id) return@command
                engine.core.promptAgent(host,id, value)
                sentFiles.forEach { java.io.File(it).delete() }
                if (draftAgent == context && draft.trim() == value) draft = "" else if (context != null && agentDrafts[context]?.trim() == value) agentDrafts.remove(context)
            }
            finally { sendingAgents.remove("$host/$id") }
        }
    }
    fun interruptAgent(id: String) {
        val host=engine.state.value?.selectedHost?:return
        val state = engine.state.value ?: return
        if (submissionLocked || interrupting || state.selectedAgent != id || state.agentPause != AgentPauseStatus.RUNNING) return
        interruptingAgents.add("$host/$id")
        engine.command {
            try { engine.core.interruptAgent(host,id) }
            finally { interruptingAgents.remove("$host/$id") }
        }
    }
    fun selectModel(id: String, model: String, effort: String) { val host=engine.state.value?.selectedHost?:return; engine.command { engine.core.setChatModel(host,id, model, effort) } }
    fun selectPermission(id: String, setting: String, value: String) { val host=engine.state.value?.selectedHost?:return; engine.command { engine.core.setChatSetting(host,id, setting, value) } }
    fun reloadComposer() = engine.command { engine.core.reloadComposer() }
    fun refreshUsage() { if (engine.state.value?.composer?.ready == true) engine.command { engine.core.refreshAccountUsage() } }
    var skillBusy by mutableStateOf(false)
        private set
    fun refreshSkills(id: String) = engine.command { engine.core.refreshSkills(id) }
    fun selectSkill(id: String, path: String, selected: Boolean) {
        val host=engine.state.value?.selectedHost?:return
        if (sending || skillBusy || engine.state.value?.selectedAgent != id) return
        skillBusy = true
        engine.command { try { engine.core.selectSkill(host,id, path, selected) } finally { skillBusy = false } }
    }
    fun attachFiles(uris: List<Uri>) {
        val host=engine.state.value?.selectedHost?:return
        val id = engine.state.value?.selectedAgent ?: return
        if (copyingAttachments || sending || uris.isEmpty()) return
        copyingAttachments = true
        engine.command {
            try {
                for (uri in uris.take(5)) {
                    val file = cacheAttachment(getApplication(), uri)
                    engine.core.uploadAttachment(host,id, file.path, file.name, file.mime, null)
                }
            } finally { copyingAttachments = false }
        }
    }
    fun retryAttachment(file: UploadInfo) {
        val host=engine.state.value?.selectedHost?:return
        val id = engine.state.value?.selectedAgent ?: return
        engine.command { engine.core.uploadAttachment(host,id, file.localPath, file.file.name, file.file.mime, file.file.id) }
    }
    fun removeAttachment(file: UploadInfo) {
        val host=engine.state.value?.selectedHost?:return
        val id = engine.state.value?.selectedAgent ?: return
        engine.command { engine.core.removeAttachment(host,id, file.file.id); java.io.File(file.localPath).delete() }
    }
    fun terminalKey(id: String, key: String) {
        val host=engine.state.value?.selectedHost?:return
        if (engine.state.value?.selectedAgent == id) engine.command { engine.core.sendAgentKey(host,id, key) }
    }
    fun respond(id: String, request: String, choice: Int) {
        val host=engine.state.value?.selectedHost?:return
        if (engine.state.value?.selectedAgent != id) return
        engine.command { engine.core.respondApproval(host,id, request, choice.toUInt()) }
    }
    fun pauseAgent(id: String) {
        val host=engine.state.value?.selectedHost?:return
        val state = engine.state.value ?: return
        if (state.selectedAgent != id) return
        engine.command { engine.core.setAgentPaused(host,id, state.agentPause == AgentPauseStatus.RUNNING) }
    }
    fun promptAgent(id: String, text: String) {
        val host=engine.state.value?.selectedHost?:return
        if (submissionLocked || interrupting || engine.state.value?.selectedAgent != id) return
        engine.command { if (!submissionLocked && !interrupting && engine.state.value?.selectedAgent == id) engine.core.promptAgent(host,id, text) }
    }
    fun approve(pin: String) { val host=engine.state.value?.selectedHost?:return; engine.command { engine.core.approvePairing(host,pin) } }
    fun takeOver(enabled: Boolean) {
        val host=engine.state.value?.selectedHost?:return
        if(!enabled){flushRemoteText?.invoke();hideTouchpad();releaseMouseButtons();releaseModifiers()}
        engine.command { engine.core.setTakeover(host,enabled) }
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
            if (target != 0L) DesktopInput.releaseInput(target)
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
    fun closeDesktop() { val host=engine.state.value?.selectedHost?:return; orientationRequest = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED; autoDesktop = false; exitTakeover(); layout = DesktopLayout.Closed; fitVideo(); engine.command { engine.core.stopDesktop(host) } }
    fun fitVideo() { zoom = 1f; panMode = false }
    fun exitTakeover() {
        flushRemoteText?.invoke()
        hideTouchpad()
        releaseMouseButtons()
        releaseModifiers()
        if (target != 0L) DesktopInput.releaseInput(target)
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
    fun videoReady(id: Long) { videoHost=engine.state.value?.selectedHost; target = id; if ((foreground || inPip) && showDesktop) retryDesktop() }
    fun videoGone(id: Long) { val host=videoHost?:return; if (target == id) { releaseMouseButtons(); target = 0; engine.command { engine.core.stopDesktop(host) } } }
    fun retryDesktop() {
        val host=engine.state.value?.selectedHost?:return
        val id = target
        if (id != 0L && engine.state.value?.connection == ConnectionStatus.CONNECTED) engine.command { engine.core.startDesktop(host,id.toULong()) }
    }
    fun onBackground(keepVideo: Boolean = false) { val host=engine.state.value?.selectedHost?:return; releaseMouseButtons(); foreground = false; if (showDesktop && !keepVideo) { videoSuspended = true; exitTakeover(); engine.command { engine.core.stopDesktop(host) } } }
    fun onForeground() { foreground = true; if (engine.state.value?.connection == ConnectionStatus.CONNECTED && showDesktop && target != 0L && (videoSuspended || engine.state.value?.desktop?.status == DesktopStatus.IDLE)) { videoSuspended = false; retryDesktop() } }
    fun modifier(ctrl: Boolean) {
        flushRemoteText?.invoke()
        if (target == 0L) return
        if (ctrl) { ctrlHeld = !ctrlHeld; DesktopInput.key(target, 162, ctrlHeld, 0) }
        else { metaHeld = !metaHeld; DesktopInput.key(target, 91, metaHeld, 0) }
    }
    fun releaseModifiers() {
        if (target != 0L) {
            if (ctrlHeld) DesktopInput.key(target, 162, false, 0)
            if (metaHeld) DesktopInput.key(target, 91, false, 0)
        }
        ctrlHeld = false; metaHeld = false
    }
    fun sendText(text: String): Boolean = target != 0L && DesktopInput.text(target, text)
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
                DesktopInput.key(destination, modifier, true, 0)
                DesktopInput.key(destination, 86, true, if (modifier == 91) 8 else 2)
                DesktopInput.key(destination, 86, false, if (modifier == 91) 8 else 2)
                DesktopInput.key(destination, modifier, false, 0)
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
        if (target != 0L) DesktopInput.mouseButton(target, button, down)
        heldMouseButtons = if (down) heldMouseButtons or mask else heldMouseButtons and mask.inv()
    }
    fun releaseMouseButtons() { for (button in listOf(1, 2, 3)) if (heldMouseButtons and (1 shl button) != 0) mouseButton(button, false); pointerMotion.reset() }
    fun moveMouse(dx: Float, dy: Float, density: Float) {
        if (target == 0L || engine.state.value?.desktop?.takeover != true) return
        val d = pointerMotion.move(dx, dy, density)
        if (d.x != 0 || d.y != 0) DesktopInput.mouseMove(target, d.x, d.y)
    }
    fun wheelMouse(dy: Float, density: Float) {
        if (target == 0L || engine.state.value?.desktop?.takeover != true) return
        val amount = pointerMotion.scroll(dy, density)
        if (amount != 0) DesktopInput.scroll(target, 0, amount)
    }
    fun leftClick() { flushRemoteText?.invoke();if (target != 0L) { DesktopInput.mouseButton(target, 1, true); DesktopInput.mouseButton(target, 1, false) } }
    fun rightClick() { flushRemoteText?.invoke();if (target != 0L) { DesktopInput.mouseButton(target, 3, true); DesktopInput.mouseButton(target, 3, false) } }
    fun scroll(vertical: Int): Boolean {
        if (target == 0L || !foreground || engine.state.value?.desktop?.takeover != true) return false
        flushRemoteText?.invoke()
        return DesktopInput.scroll(target, 0, vertical)
    }
    fun desktopChord(modifierKey:Int,key:Int,mask:Int) {
        flushRemoteText?.invoke()
        if(target==0L||engine.state.value?.desktop?.takeover!=true)return
        DesktopInput.key(target,modifierKey,true,0)
        DesktopInput.key(target,key,true,mask);DesktopInput.key(target,key,false,mask)
        DesktopInput.key(target,modifierKey,false,0)
    }
    fun shortcut(key: Int, ctrl: Boolean = false) {
        flushRemoteText?.invoke()
        if (target == 0L) return
        if (ctrl || ctrlHeld) DesktopInput.key(target, 162, true, 0)
        if (metaHeld) DesktopInput.key(target, 91, true, 0)
        val modifiers = (if (ctrl || ctrlHeld) 2 else 0) or (if (metaHeld) 8 else 0)
        DesktopInput.key(target, key, true, modifiers)
        DesktopInput.key(target, key, false, modifiers)
        if (ctrl) DesktopInput.key(target, 162, false, 0)
        releaseModifiers()
    }
}
