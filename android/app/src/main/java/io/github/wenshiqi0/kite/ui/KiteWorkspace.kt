package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Rect
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.interaction.collectIsDraggedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.media.VideoSurface
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.collectLatest
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import kotlin.math.roundToInt
import java.lang.ref.WeakReference

private val Ink = KiteColors.Text
private val Muted = KiteColors.Muted
private val Paper = KiteColors.Surface
private val Blue = KiteColors.Accent
private val Line = KiteColors.Line
private val Amber = KiteColors.Warning

@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun KiteWorkspace(vm: HostCheckViewModel, enterPip: () -> Unit, updatePip: () -> Unit) {
    val current by vm.engine.state.collectAsState()
    val state = current
    val agent = state?.agents?.find { it.id == state.selectedAgent }
    val presentationPhase = responsePresentationPhase(state?.responsePhase ?: ResponsePhase.IDLE, vm.sending, state?.question != null, state?.approval != null)
    val responseInProgress = presentationPhase in listOf(ResponsePhase.SENDING, ResponsePhase.THINKING, ResponsePhase.TOOL, ResponsePhase.RESPONDING)
    val currentSession = state?.sessions?.find { it.id == state.selectedSession }
    val context = LocalContext.current
    val sendTextFailure = stringResource(R.string.error_send_text)
    val restoreDesktop = stringResource(R.string.restore_desktop)
    val returnChat = stringResource(R.string.return_chat)
    val menuDescription = stringResource(R.string.navigation_menu)
    val moreDescription = stringResource(R.string.more_actions)
    val splitDescription = stringResource(R.string.resize_split)
    val addDescription = stringResource(R.string.compose_add)
    val frameCounts = stringResource(R.string.video_frame_counts, state?.desktop?.receivedFrames?.toLong() ?: 0L, state?.desktop?.renderedFrames?.toLong() ?: 0L)
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val drawerActive = drawer.currentValue != DrawerValue.Closed || drawer.targetValue != DrawerValue.Closed
    val scope = rememberCoroutineScope()
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    var autoFullscreen by remember { mutableStateOf(false) }
    var tools by remember { mutableStateOf(false) }
    var newSession by remember { mutableStateOf(false) }
    var gestureHelp by remember { mutableStateOf(false) }
    var provider by remember { mutableStateOf("codex") }
    val availableProviders = if (state?.connection == ConnectionStatus.CONNECTED) state.providers else emptyList()
    val providerInfo = availableProviders.find { it.id == agent?.agent }
    var project by remember { mutableStateOf("") }
    var sessionName by remember { mutableStateOf("") }
    var remoteKeyboard by remember { mutableStateOf(false) }
    var modelSettings by remember { mutableStateOf(false) }
    var usageDialog by remember { mutableStateOf(false) }
    var noticeCenter by remember { mutableStateOf(false) }
    val videoReference=remember { mutableStateOf<VideoSurface?>(null) }
    var videoArea by remember { mutableStateOf(androidx.compose.ui.geometry.Rect.Zero) }
    var mouseBounds by remember { mutableStateOf(emptyList<androidx.compose.ui.geometry.Rect>()) }
    val chromeBounds=remember { mutableStateMapOf<String,androidx.compose.ui.geometry.Rect>() }
    var footerHeight by remember { mutableStateOf(60.dp) }
    val focusManager=LocalFocusManager.current
    val softwareKeyboard=LocalSoftwareKeyboardController.current
    val inputDensity=LocalDensity.current
    val imeVisible=WindowInsets.ime.getBottom(inputDensity)>0
    var terminal by remember { mutableStateOf(false) }
    var diagnostics by remember { mutableStateOf(false) }
    LaunchedEffect(state?.selectedHost, state?.selectedSession, state?.selectedAgent) {
        modelSettings = false; usageDialog = false; terminal = false
    }
    var skillPicker by remember(state?.selectedHost, state?.selectedSession, state?.selectedAgent) { mutableStateOf(false) }
    if (skillPicker && agent != null && state != null) SkillPicker(
        draft = state.composer.skills,
        enabled = !vm.sending && !vm.skillBusy && state.composer.ready,
        select = { skill, selected -> vm.selectSkill(agent.id, skill.path, selected) },
        refresh = { vm.refreshSkills(agent.id) },
        dismiss = { skillPicker = false },
    )
    var answerRequest by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state?.question?.id) { if (answerRequest != state?.question?.id) answerRequest = null }
    var arrows by remember { mutableStateOf(false) }
    var warning by remember { mutableStateOf<String?>(null) }
    val timeline=vm.engine.timeline
    val entries=timeline.rows
    val initializingSession = vm.sessionStartup == SessionStartupPhase.INITIALIZING
    val initializationFailed = vm.sessionStartup == SessionStartupPhase.FAILED
    val showInitialization = (initializingSession || initializationFailed) && (vm.creatingAgent || entries.isEmpty())
    val conversationContext=timeline.context
    val streamingId=timeline.messages.lastOrNull{it.kind==MessageKind.ASSISTANT}?.id
    val cacheWindow=remember{LazyLayoutCacheWindow(1f,.5f)}
    val chatScroll=rememberLazyListState(cacheWindow=cacheWindow)
    var followTail by remember(conversationContext){mutableStateOf(true)}
    val tailMargin=remember(conversationContext){ChatTailMargin()}
    var latestVisible by remember(conversationContext){mutableStateOf(false)}
    val historyScroll=remember(conversationContext,inputDensity.density){object:NestedScrollConnection{
        override fun onPostScroll(consumed:Offset,available:Offset,source:NestedScrollSource):Offset{
            if(consumed.y>0)followTail=false
            if(!followTail){tailMargin.scrolled(consumed.y/inputDensity.density);latestVisible=tailMargin.visible}
            return Offset.Zero
        }
    }}
    var chatHeight by remember{mutableIntStateOf(0)}
    val draggingHistory by chatScroll.interactionSource.collectIsDraggedAsState()
    fun pauseHistory(){followTail=false}
    fun jumpToLatest(){tailMargin.reset();latestVisible=false;followTail=true}
    LaunchedEffect(draggingHistory){if(draggingHistory)followTail=false}
    LaunchedEffect(conversationContext,chatScroll,inputDensity.density){
        snapshotFlow{
            val layout=chatScroll.layoutInfo
            val measurement=layout.visibleItemsInfo.lastOrNull()?.let{item->
                chatTailMeasurement(item.index,layout.totalItemsCount,
                    (item.offset.toFloat()+item.size)/inputDensity.density,
                    layout.viewportEndOffset/inputDensity.density,
                    layout.afterContentPadding/inputDensity.density)
            }
            Triple(chatScroll.isScrollInProgress,chatScroll.canScrollForward,measurement)
        }.collectLatest{(scrolling,canForward,measurement)->
            if(measurement!=null)tailMargin.observe(measurement)
            if(measurement?.canResumeFollow(scrolling,canForward)==true&&entries.isNotEmpty()){
                followTail=true;tailMargin.reset()
            }
            latestVisible=tailMargin.visible
        }
    }
    LaunchedEffect(vm.sending,vm.answeringQuestion){if(vm.sending||vm.answeringQuestion)followTail=true}
    LaunchedEffect(conversationContext,entries.lastOrNull(),state?.approval?.id,state?.question?.id,state?.responsePhase,state?.composer?.details,chatHeight,vm.inPip,vm.pipTransition,followTail){
        if(vm.inPip||vm.pipTransition||chatHeight<16||!followTail||chatScroll.isScrollInProgress||entries.isEmpty())return@LaunchedEffect
        withFrameNanos{};withFrameNanos{}
        if(followTail&&!chatScroll.isScrollInProgress)chatScroll.scrollToItem((chatScroll.layoutInfo.totalItemsCount-1).coerceAtLeast(0))
    }
    val attachments = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        vm.externalActivity = false
        updatePip()
        vm.attachFiles(uris)
    }
    fun pickAttachments(types: Array<String>) {
        vm.externalActivity = true
        updatePip()
        runCatching { attachments.launch(types) }.onFailure { vm.externalActivity = false; updatePip(); warning = it.message }
    }
    val hideChrome = vm.inPip || vm.pipTransition
    val bridgeActive=state?.desktop?.takeover==true&&!hideChrome&&!vm.settingsVisible&&!drawerActive
    fun toggleRemoteKeyboard(){
        vm.hideTouchpad()
        if(remoteKeyboard&&imeVisible){videoReference.value?.closeRemoteKeyboard();remoteKeyboard=false}
        else if(bridgeActive){
            focusManager.clearFocus()
            remoteKeyboard=videoReference.value?.openRemoteKeyboard()==true
        }
    }
    fun toggleTouchpad(){
        videoReference.value?.closeRemoteKeyboard()
        remoteKeyboard=false;softwareKeyboard?.hide();focusManager.clearFocus()
        vm.toggleTouchpad()
    }
    LaunchedEffect(imeVisible){if(imeVisible)vm.hideTouchpad()else if(remoteKeyboard){videoReference.value?.closeRemoteKeyboard(false);remoteKeyboard=false}}
    LaunchedEffect(bridgeActive){if(!bridgeActive){videoReference.value?.closeRemoteKeyboard();remoteKeyboard=false;vm.releaseMouseButtons()}}
    ClipboardBridge(vm,blocked=hideChrome || vm.settingsVisible || drawerActive)
    LaunchedEffect(hideChrome) { drawer.close(); if (hideChrome) { tools = false; terminal = false; diagnostics = false; remoteKeyboard = false; arrows = false; newSession = false; gestureHelp = false; usageDialog = false } }
    LaunchedEffect(landscape, vm.inPip) {
        if (vm.inPip) return@LaunchedEffect
        if (landscape && vm.layout == DesktopLayout.Split) { autoFullscreen = true; vm.changeLayout(DesktopLayout.Fullscreen, rotate = false) }
        else if (!landscape && autoFullscreen && vm.layout == DesktopLayout.Fullscreen) { autoFullscreen = false; vm.changeLayout(DesktopLayout.Split) }
    }
    LaunchedEffect(vm.layout, vm.inPip, state?.desktop?.status, state?.desktop?.sourceWidth, state?.desktop?.sourceHeight) { updatePip() }
    LaunchedEffect(vm.settingsVisible) { if (vm.settingsVisible) vm.exitTakeover() }
    BackHandler(enabled = vm.settingsVisible || terminal || diagnostics || vm.showDesktop) {
        when {
            remoteKeyboard -> {videoReference.value?.closeRemoteKeyboard();remoteKeyboard=false}
            terminal -> terminal = false
            diagnostics -> diagnostics = false
            vm.settingsVisible -> if(vm.hostEditorVisible)vm.hostEditorVisible=false else vm.settingsVisible=false
            vm.layout == DesktopLayout.Fullscreen -> vm.returnToChat(landscape)
            vm.showDesktop -> enterPip()
        }
    }
    BackHandler(enabled = drawerActive) { scope.launch { drawer.close() } }
    ModalNavigationDrawer(drawerState = drawer, scrimColor = if (hideChrome) Color.Transparent else DrawerDefaults.scrimColor, gesturesEnabled = drawerActive, drawerContent = {
        ModalDrawerSheet(drawerState = drawer, modifier = Modifier.width(310.dp), drawerContainerColor = KiteColors.Surface) {
            SessionDrawer(vm, state, close = { scope.launch { drawer.close() } }, open = { id -> vm.openSession(id); scope.launch { drawer.close() } }, create = { newSession = true; scope.launch { drawer.close() } }, settings = { vm.settingsVisible = true; scope.launch { drawer.close() } })
        }
    }) {
        BoxWithConstraints(Modifier.fillMaxSize().background(if (vm.layout == DesktopLayout.Fullscreen || vm.inPip) Paper else KiteColors.Surface).pointerInput(vm.target,bridgeActive){
            if(!bridgeActive)return@pointerInput
            var canvasFinger:PointerId?=null
            try{awaitPointerEventScope{
                while(true){
                    val event=awaitPointerEvent(PointerEventPass.Initial)
                    if(canvasFinger==null&&vm.heldMouseButtons!=0){
                        canvasFinger=event.changes.firstOrNull{it.pressed&&!it.previousPressed&&videoArea.contains(it.position)&&mouseBounds.none{b->b.contains(it.position)}&&chromeBounds.values.none{b->b.contains(it.position)}}?.id
                    }
                    event.changes.firstOrNull{it.id==canvasFinger}?.let{finger->
                        if(finger.pressed)videoReference.value?.bridgePointer(finger.position.x-videoArea.left,finger.position.y-videoArea.top)
                        else{videoReference.value?.finishBridgedPointer();canvasFinger=null}
                        finger.consume()
                    }
                }
            }}finally{if(canvasFinger!=null)videoReference.value?.finishBridgedPointer()}
        }) {
            val density = LocalDensity.current
            val availableWidth = maxWidth
            val availableHeight = maxHeight
            val fullscreen = vm.layout == DesktopLayout.Fullscreen || vm.inPip
            val railWidth=if(vm.layout==DesktopLayout.Fullscreen&&!hideChrome)64.dp else 0.dp
            val desktopWidth=(availableWidth-railWidth).coerceAtLeast(1.dp)
            val takeover = state?.desktop?.takeover == true && !hideChrome && !vm.settingsVisible
            val split = vm.layout == DesktopLayout.Split && !fullscreen
            val aspect=if(state!=null&&state.desktop.sourceWidth>0u&&state.desktop.sourceHeight>0u)(state.desktop.sourceWidth.toFloat()/state.desktop.sourceHeight.toFloat()).coerceIn(.1f,10f)else 16f/9f
            val safeTop=with(density){WindowInsets.safeDrawing.getTop(this).toDp()}
            val safeBottom=with(density){maxOf(WindowInsets.safeDrawing.getBottom(this),WindowInsets.ime.getBottom(this)).toDp()}
            val usableHeight=(availableHeight-safeTop-safeBottom).coerceAtLeast(0.dp)
            val padVisible=split&&vm.touchpadVisible&&takeover&&!imeVisible
            val padHeight=WorkspaceGeometry.padHeight(usableHeight.value).dp
            val splitHeight=WorkspaceGeometry.splitVideoHeight(availableWidth.value,usableHeight.value,aspect,if(padVisible)padHeight.value else 0f,footerHeight.value,vm.splitAdjustment).dp
            if (!fullscreen && !hideChrome) Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).imePadding()) {
                Row(Modifier.fillMaxWidth().height(48.dp).borderBottom(), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = { scope.launch { drawer.open() } }, modifier = Modifier.semantics { contentDescription = menuDescription }) { ProductIcon(Mark.Menu) }
                    Text(if(vm.creatingAgent)stringResource(R.string.new_session) else currentSession?.let { sessionTitle(it) } ?: agent?.let { agentTitle(it) } ?: stringResource(R.string.app_name), modifier = Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if(!initializingSession&&!showInitialization&&state?.history?.loading==true)CircularProgressIndicator(Modifier.padding(end=8.dp).size(14.dp),strokeWidth=1.5.dp,color=Muted)
                    if(!initializingSession&&!showInitialization&&state?.history?.failed==true)TextButton(onClick={vm.engine.command{vm.engine.core.retryHistory()}}){Text("重试同步",fontSize=11.sp)}
                    if (!initializingSession && !showInitialization && agent != null) { StatusDot(if (state.question != null || state.approval != null || state.agentPause == AgentPauseStatus.PAUSED) "blocked" else if (responseInProgress) "working" else agent.status); Spacer(Modifier.width(6.dp)) }
                    if(split&&state!=null)DesktopModeSwitch(takeover,state.desktop.status==DesktopStatus.STREAMING,vm::takeOver)
                    Box {
                        IconButton(onClick = { tools = true }, modifier = Modifier.semantics { contentDescription = moreDescription }) { ProductIcon(Mark.More) }
                        DropdownMenu(expanded = tools, onDismissRequest = { tools = false }) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.new_session)) }, onClick = { tools = false; newSession = true }, enabled = state?.connection == ConnectionStatus.CONNECTED && canStartResourceAgent(state?.hostResources) && !vm.creatingAgent && !vm.sessionBusy)
                            DropdownMenuItem(text = { Text(stringResource(R.string.open_desktop)) }, leadingIcon = { ProductIcon(Mark.Monitor) }, onClick = { tools = false; vm.openDesktop() }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            DropdownMenuItem(text = { Text(stringResource(R.string.mode_split)) }, leadingIcon = { ProductIcon(Mark.Split) }, onClick = { tools = false; vm.changeLayout(DesktopLayout.Split) }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            if(vm.showDesktop||state?.desktop?.status in listOf(DesktopStatus.STARTING,DesktopStatus.STREAMING,DesktopStatus.PAIRING))DropdownMenuItem(text={Text(stringResource(R.string.desktop_disconnect))},leadingIcon={ProductIcon(Mark.Close)},onClick={tools=false;videoReference.value?.closeRemoteKeyboard();softwareKeyboard?.hide();focusManager.clearFocus();vm.closeDesktop();updatePip()})
                            if(vm.showDesktop)DropdownMenuItem(text={Text(stringResource(if(vm.engine.desktopAudioEnabled)R.string.desktop_audio_mute else R.string.desktop_audio_enable))},onClick={vm.engine.setDesktopAudio(!vm.engine.desktopAudioEnabled);tools=false})
                            currentSession?.let { session -> DropdownMenuItem(text = { Text(stringResource(if (session.archived) R.string.restore_session else R.string.archive_session)) }, onClick = { tools = false; vm.archiveSession(session.id, !session.archived) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy) }
                            currentSession?.let { session -> DropdownMenuItem(text = { Text(stringResource(R.string.delete_session), color = MaterialTheme.colorScheme.error) }, onClick = { tools = false; vm.requestSessionDeletion(session) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy) }
                            if (agent?.canPause == true) DropdownMenuItem(text = { Text(stringResource(if (state.agentPause == AgentPauseStatus.RUNNING) R.string.pause_agent else R.string.resume_agent)) }, onClick = { tools = false; vm.pauseAgent(agent.id) })
                            if (providerInfo?.terminal == true) DropdownMenuItem(text = { Text(stringResource(R.string.terminal_output)) }, onClick = { tools = false; terminal = true }, enabled = providerInfo?.terminal == true)
                            DropdownMenuItem(text={Text(stringResource(R.string.notice_center))},onClick={tools=false;noticeCenter=true;vm.engine.refreshAgentNotifications()})
                            DropdownMenuItem(text = { Text(stringResource(R.string.connection_diagnostics)) }, onClick = { tools = false; diagnostics = true })
                            DropdownMenuItem(text = { Text(stringResource(R.string.host_settings)) }, onClick = { tools = false; vm.settingsVisible = true })
                        }
                    }
                }
                if (state?.connection == ConnectionStatus.RECONNECTING) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 1.5.dp)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.connection_reconnecting), color = Muted, fontSize = 12.sp, modifier = Modifier.weight(1f))
                    TextButton(onClick = vm::disconnect) { Text(stringResource(R.string.disconnect)) }
                }
                if (state?.connection == ConnectionStatus.CONNECTED) Box(Modifier.padding(horizontal = 12.dp, vertical = 4.dp)) {
                    ResourceWarning(state?.hostResources) {
                        state?.hosts?.find { it.id == state?.selectedHost }?.let { host -> vm.chooseHost(host); vm.settingsVisible = true }
                    }
                }
                if (split) {
                    Spacer(Modifier.height(splitHeight))
                    Row(Modifier.fillMaxWidth().height(48.dp).background(Paper),verticalAlignment=Alignment.CenterVertically){
                        Box(Modifier.size(48.dp).semantics{contentDescription=splitDescription}.pointerInput(availableHeight){
                            detectDragGestures{change,amount->change.consume();vm.splitAdjustment=(vm.splitAdjustment+amount.y/density.density).coerceIn(-240f,240f)}
                        },contentAlignment=Alignment.Center){Box(Modifier.size(24.dp,3.dp).background(Muted.copy(alpha=.5f),RoundedCornerShape(3.dp)))}
                        Text(stringResource(desktopStatusLabel(state?.desktop?.status?:DesktopStatus.IDLE)),color=Muted,fontSize=11.sp,modifier=Modifier.weight(1f))
                        TextButton(onClick=::toggleTouchpad,enabled=takeover&&state?.desktop?.status==DesktopStatus.STREAMING,modifier=Modifier.height(48.dp)){
                            ProductIcon(Mark.Touchpad,if(padVisible)Blue else Muted,Modifier.size(18.dp));Spacer(Modifier.width(5.dp));Text(stringResource(R.string.trackpad),fontSize=12.sp)
                        }
                        IconButton(onClick=::toggleRemoteKeyboard,enabled=takeover,modifier=Modifier.size(48.dp).semantics{contentDescription="键盘"}){ProductIcon(Mark.Keyboard,modifier=Modifier.size(20.dp))}
                        IconButton(onClick={vm.changeLayout(DesktopLayout.Fullscreen)},modifier=Modifier.size(48.dp).semantics{contentDescription="全屏桌面"}){ProductIcon(Mark.Expand,modifier=Modifier.size(20.dp))}
                    }
                    if(padVisible)TouchpadPanel(vm,padHeight,bridgeActive,vm::hideTouchpad)
                }
                if (state == null || !state.initialized) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Blue)
                if (state != null && state.connection != ConnectionStatus.CONNECTED) {
                    Column(Modifier.fillMaxWidth().background(Paper).padding(horizontal = 16.dp, vertical = 8.dp)) {
                        Text(connectionStepLabel(state), color = Muted)
                        state.failure?.let { Text(it.details, color = MaterialTheme.colorScheme.error, fontSize = 12.sp, maxLines = 4, overflow = TextOverflow.Ellipsis) }
                        TextButton(onClick = { vm.settingsVisible = true }) { Text(stringResource(R.string.open_host_settings)) }
                    }
                }
                Box(Modifier.weight(1f).fillMaxWidth().onSizeChanged{chatHeight=it.height}){
                if (showInitialization) Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {
                    SessionInitializationMessage(failed=initializationFailed,retryEnabled=!vm.sessionBusy,onRetry={
                        if(currentSession!=null&&(agent==null||state?.failure?.kind==FailureKind.AGENT_START))vm.resumeSession(currentSession.id)
                        else if(agent!=null)vm.engine.command{vm.engine.core.reloadComposer()}
                        else newSession=true
                    })
                }
                else {
                LazyColumn(Modifier.fillMaxSize().nestedScroll(historyScroll).semantics { contentDescription="聊天记录" }.pointerInput(conversationContext) {
                    awaitEachGesture {
                        awaitFirstDown(requireUnconsumed=false,pass=PointerEventPass.Initial)
                        followTail=false
                        do {val event=awaitPointerEvent(PointerEventPass.Initial)}while(event.changes.any{it.pressed})
                    }
                }, state = chatScroll, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(CHAT_ROW_SPACING_DP.dp)) {
                    if (state?.failure?.kind == FailureKind.AGENT_START) item {
                        Surface(color = KiteColors.WarningSurface, shape = RoundedCornerShape(16.dp)) {
                            Column(Modifier.padding(16.dp)) {
                                Text(stringResource(R.string.agent_start_failed))
                                Text(state.failure!!.details, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                                currentSession?.let { session -> TextButton(onClick = { vm.resumeSession(session.id) }, enabled = !vm.sessionBusy) { Text(stringResource(R.string.retry)) } }
                                TextButton(onClick = { vm.engine.command { vm.engine.core.clearFailure() } }) { Text(stringResource(R.string.dismiss)) }
                            }
                        }
                    }

                    if (agent == null && currentSession == null) item {
                        Column(Modifier.fillMaxWidth().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            ProductIcon(Mark.Monitor, Muted, Modifier.size(36.dp))
                            Text(stringResource(if (state?.connection == ConnectionStatus.CONNECTED) R.string.no_active_sessions else R.string.workspace_welcome), color = Muted, modifier = Modifier.padding(16.dp))
                            if (state?.connection == ConnectionStatus.CONNECTED) {
                                val preparing = state.connectionStep == ConnectionStep.AGENT && state.failure == null
                                Button(onClick = { newSession = true }, enabled = !preparing && !vm.creatingAgent && !vm.sessionBusy) { Text(stringResource(if (preparing) R.string.connection_agent else R.string.new_session)) }
                                TextButton(onClick = vm::openDesktop) { Text(stringResource(R.string.open_desktop)) }
                            } else Button(onClick = { vm.settingsVisible = true }) { Text(stringResource(R.string.open_host_settings)) }
                        }
                    }
                    items(entries, key = { it.key },contentType={if(it is ChatEntry.Work)"work" else (it as ChatEntry.Message).message.kind}) { entry -> when (entry) {
                        is ChatEntry.Message -> MessageCard(vm, updatePip, ::pauseHistory, entry.message, state?.responsePhase == ResponsePhase.RESPONDING && entry.message.id == streamingId)
                        is ChatEntry.Work -> ToolGroupCard(entry, agent?.status == "working", ::pauseHistory, vm::openDesktop)
                    } }
                    state?.approval?.let { approval -> item { ApprovalCard(approval, state.agentPause == AgentPauseStatus.RUNNING) { state.selectedAgent?.let { id -> vm.respond(id, approval.id, it) } } } }
                    state?.question?.let { question -> item(key = "question:${question.id}") { ModelQuestionCard(vm, question, state.connection == ConnectionStatus.CONNECTED && state.agentPause == AgentPauseStatus.RUNNING) } }
                    if (state?.connection == ConnectionStatus.CONNECTED && presentationPhase != ResponsePhase.IDLE) item(key = "response-feedback") { ResponseFeedback(presentationPhase, vm.sending, waitingForApproval = state.question == null && state.approval != null, details = state.composer.details) }
                    if (agent?.status == "blocked" && state?.approval == null && state?.question == null && !responseInProgress) item {
                        TextButton(onClick = { terminal = true }, enabled = providerInfo?.terminal == true) { Text(stringResource(R.string.agent_waiting_open_terminal), color = KiteColors.Warning) }
                    }
                    item(key = "conversation-tail") { Spacer(Modifier.height(CHAT_TAIL_SPACER_DP.dp)) }
                }
                if(entries.isEmpty()&&state?.selectedSession!=null&&state.question==null&&state.approval==null&&(state.history.loading||state.history.failed||state.history.waiting)){
                    Column(Modifier.align(Alignment.Center).padding(24.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(12.dp)){
                        if(state.history.loading)CircularProgressIndicator(Modifier.size(20.dp),strokeWidth=2.dp,color=Muted)
                        Text(stringResource(when{state.history.failed->R.string.history_sync_failed;state.history.waiting->if(state.composer.ready)R.string.new_session_ready else R.string.composer_starting;else->R.string.history_syncing}),color=Muted,fontSize=13.sp)
                        if(state.history.failed)TextButton(onClick={vm.engine.command{vm.engine.core.retryHistory()}}){Text("重试")}
                    }
                }
                if(!followTail&&latestVisible&&entries.isNotEmpty())LatestMessageButton(::jumpToLatest,Modifier.align(Alignment.BottomEnd).padding(8.dp))
                }
                }
                Column(Modifier.fillMaxWidth().onSizeChanged{footerHeight=with(density){it.height.toDp()}}){
                if (!initializingSession && !showInitialization && currentSession != null && (currentSession.archived || agent == null)) {
                    Row(Modifier.fillMaxWidth().background(Paper).padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(stringResource(if (availableProviders.none { it.id == currentSession.provider }) R.string.harness_unavailable else if (currentSession.archived) R.string.session_archived_hint else if (currentSession.provider == "claude" && !currentSession.recorded) R.string.claude_not_connected else R.string.session_recorded), fontSize = 12.sp, color = Muted, modifier = Modifier.weight(1f))
                        if (availableProviders.any { it.id == currentSession.provider }) TextButton(onClick = { vm.resumeSession(currentSession.id) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy) {
                            if (vm.sessionBusy) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 1.5.dp) else Text(stringResource(if (currentSession.archived) R.string.restore_and_continue else if (currentSession.provider == "claude" && !currentSession.recorded) R.string.connect_claude else R.string.continue_session))
                        }
                    }
                }
                val composeReason = when {
                    state?.connection != ConnectionStatus.CONNECTED -> R.string.composer_no_connection
                    currentSession?.archived == true -> R.string.session_archived_hint
                    agent == null -> if (currentSession == null) R.string.composer_no_agent else null
                    providerInfo == null -> R.string.harness_unavailable
                    agent.launchPending -> R.string.composer_starting
                    providerInfo?.managedChat == true && !state.composer.ready -> if(state.composer.loading)R.string.composer_starting else R.string.composer_backend_unavailable
                    state.agentPause == AgentPauseStatus.PAUSED -> R.string.composer_paused
                    state.question != null -> if (state.question?.blocking == true) R.string.awaiting_answer else R.string.question_pending_hint
                    agent.status == "blocked" && state.question == null && !responseInProgress -> R.string.composer_blocked_hint
                    state.agentPause != AgentPauseStatus.RUNNING -> R.string.composer_checking
                    else -> null
                }
                if (initializingSession && !showInitialization) SessionInitializationMessage(Modifier.align(Alignment.CenterHorizontally))
                if (!initializingSession && !showInitialization && composeReason != null && currentSession?.archived != true) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(composeReason), color = Muted, fontSize = 12.sp, modifier = Modifier.weight(1f))
                    if (state?.question != null) TextButton(onClick = { answerRequest = state.question?.id }) { Text(stringResource(R.string.go_answer)) }
                    else if(composeReason==R.string.composer_backend_unavailable)TextButton(onClick={vm.engine.command{vm.engine.core.reloadComposer()}}){Text(stringResource(R.string.retry))}
                }
                if (state?.question?.blocking != true) {
                AttachmentDrafts(vm, state?.composer?.uploads.orEmpty(), onExternalActivityChanged = updatePip, inspect = ::pauseHistory)
                SkillDrafts(state?.composer?.skills?.selected.orEmpty(), !vm.sending && !vm.skillBusy) { skill ->
                    agent?.id?.let { vm.selectSkill(it, skill.path, false) }
                }
                Row(Modifier.widthIn(max=680.dp).fillMaxWidth().align(Alignment.CenterHorizontally).padding(horizontal=10.dp,vertical=6.dp).padding(bottom=6.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (providerInfo?.skills == true || providerInfo?.attachments == true) Box {
                        var plus by remember { mutableStateOf(false) }
                        IconButton(onClick = { plus = true }, enabled = !initializingSession, modifier = Modifier.semantics { contentDescription = addDescription }) { ProductIcon(Mark.Plus) }
                        DropdownMenu(plus, { plus = false }) {
                            if (providerInfo?.skills == true) DropdownMenuItem(
                                text = { Text(stringResource(R.string.skills_title)) },
                                leadingIcon = { ProductIcon(Mark.Skill) },
                                enabled = providerInfo?.skills == true && state?.composer?.ready == true && !vm.sending,
                                onClick = { plus = false; skillPicker = true; agent?.id?.let(vm::refreshSkills) },
                            )
                            if (providerInfo?.attachments == true) buildList {
                                val modalities = state?.composer?.models?.find { it.model == state.composer.model }?.modalities.orEmpty()
                                if ("image" in modalities) add(R.string.attach_image to "image/*")
                                add(R.string.attach_file to "*/*")
                                if ("audio" in modalities) add(R.string.attach_audio to "audio/*")
                            }.forEach { (label, type) ->
                                DropdownMenuItem(text = { Text(stringResource(label)) }, leadingIcon = { ProductIcon(when (label) { R.string.attach_image -> Mark.Image; R.string.attach_audio -> Mark.Audio; else -> Mark.File }) }, onClick = { plus = false; pickAttachments(arrayOf(type)) }, enabled = providerInfo?.attachments == true && state?.composer?.ready == true && !vm.sending && !vm.copyingAttachments && state.composer.uploads.size < 5)
                            }
                        }
                    }
                    if (providerInfo?.models == true) IconButton(onClick={modelSettings=true;vm.refreshUsage()},enabled=!initializingSession&&providerInfo?.models==true,modifier=Modifier.size(48.dp).semantics{contentDescription="模型与思考强度"}){ProductIcon(Mark.Settings,modifier=Modifier.size(19.dp))}
                    ChatDraftField(if(vm.creatingAgent)"" else vm.draft,{vm.draft=it},Modifier.weight(1f),onFocus={vm.hideTouchpad();videoReference.value?.closeRemoteKeyboard(false);remoteKeyboard=false;vm.releaseModifiers()},enabled=!initializingSession)
                    ComposerActionButton(
                        working = vm.sessionStartup == SessionStartupPhase.READY && if (providerInfo?.managedChat == true) state?.responsePhase in listOf(ResponsePhase.SENDING, ResponsePhase.THINKING, ResponsePhase.TOOL, ResponsePhase.RESPONDING, ResponsePhase.WAITING) else agent?.status in listOf("working", "blocked"),
                        hasInput = !vm.creatingAgent && (vm.draft.isNotBlank() || !state?.composer?.uploads.isNullOrEmpty() || !state?.composer?.skills?.selected.isNullOrEmpty()),
                        canSend = !vm.submissionLocked && agent != null && !vm.sending && !vm.skillBusy && !vm.copyingAttachments && !vm.interrupting && state?.composer?.uploads?.all { it.ready } != false && composeReason == null,
                        canInterrupt = !vm.submissionLocked && agent != null && state?.agentPause == AgentPauseStatus.RUNNING && (providerInfo?.managedChat == true || providerInfo?.terminal == true),
                        sending = vm.sending,
                        interrupting = vm.interrupting,
                        onSend = { agent?.id?.let(vm::sendPrompt) },
                        onInterrupt = { agent?.id?.let(vm::interruptAgent) },
                    )
                }
                }
                }
            }
            if (state != null && vm.showDesktop && state.connection == ConnectionStatus.CONNECTED) {
                val floatingWidth = (availableWidth * .59f).coerceIn(160.dp, 250.dp)
                val floatingHeight = floatingWidth / aspect + 38.dp
                val fx = (availableWidth - floatingWidth - 12.dp).coerceAtLeast(0.dp) * vm.floatingX
                val fy = safeTop + 64.dp + (availableHeight - floatingHeight - safeTop - 154.dp).coerceAtLeast(0.dp) * vm.floatingY
                val docked = vm.layout == DesktopLayout.Docked || vm.settingsVisible
                val frameX: Dp
                val frameY: Dp
                val frameW: Dp
                val frameH: Dp
                when {
                    vm.inPip -> { frameX = 0.dp; frameY = 0.dp; frameW = availableWidth; frameH = availableHeight }
                    docked -> { frameX = (-2).dp; frameY = (-2).dp; frameW = 1.dp; frameH = 1.dp }
                    vm.layout == DesktopLayout.Fullscreen -> { frameX = 0.dp; frameY = 0.dp; frameW = desktopWidth; frameH = (availableHeight-with(density){WindowInsets.ime.getBottom(this).toDp()}).coerceAtLeast(80.dp) }
                    split -> { frameX = 0.dp; frameY = safeTop + 48.dp; frameW = availableWidth; frameH = splitHeight }
                    else -> { frameX = fx; frameY = fy + 12.dp; frameW = floatingWidth; frameH = floatingHeight - 38.dp }
                }
                if (vm.layout == DesktopLayout.Floating && !vm.inPip && !docked) {
                    Box(Modifier.absoluteOffset(fx, fy).width(floatingWidth).height(floatingHeight).background(Paper, RoundedCornerShape(14.dp)).border(1.dp, KiteColors.Line, RoundedCornerShape(14.dp)))
                }
                SideEffect{
                    videoArea=if(docked||hideChrome)androidx.compose.ui.geometry.Rect.Zero else with(density){androidx.compose.ui.geometry.Rect(frameX.toPx(),frameY.toPx(),(frameX+frameW).toPx(),(frameY+frameH).toPx())}
                    if(vm.layout==DesktopLayout.Fullscreen){
                        chromeBounds["rail"]=with(density){androidx.compose.ui.geometry.Rect(desktopWidth.toPx(),0f,availableWidth.toPx(),availableHeight.toPx())}
                        if(!imeVisible)chromeBounds.remove("toolbar")
                    }else{chromeBounds.remove("top");chromeBounds.remove("rail");chromeBounds.remove("toolbar")}
                }
                Box(Modifier.absoluteOffset(frameX, frameY).size(frameW, frameH).clipToBounds().background(Paper)) {
                    AndroidView(factory = { VideoSurface(it, vm::videoReady, {id->vm.videoGone(id);if(videoReference.value?.target==id){videoReference.value=null;vm.flushRemoteText=null}}).also { surface ->
                        videoReference.value=surface
                        val weak=WeakReference(surface)
                        vm.flushRemoteText={weak.get()?.flushRemoteText()}
                    } }, update = { surface ->
                        surface.takeover = takeover && !docked
                        surface.audioEnabled=vm.engine.desktopAudioEnabled
                        surface.fullscreenInput = vm.layout == DesktopLayout.Fullscreen
                        surface.trackpad = false
                        surface.externalMouseButtons = vm.heldMouseButtons
                        surface.onPaste = if(vm.engine.clipboardSyncEnabled) vm::pasteSynchronized else null
                        surface.onTextFailure={warning=sendTextFailure}
                        surface.editorModifiers=(if(vm.ctrlHeld)2 else 0) or (if(vm.metaHeld)8 else 0)
                        surface.panMode = vm.panMode
                        surface.gesturesEnabled = !hideChrome && !docked && !drawerActive
                        surface.pip = hideChrome
                        surface.setSourceSize(state.desktop.sourceWidth.toInt(), state.desktop.sourceHeight.toInt())
                        surface.onZoomChanged = { vm.zoom = it }
                        surface.zoom = vm.zoom
                    }, modifier = Modifier.fillMaxSize().onGloballyPositioned { p ->
                        val b = p.boundsInWindow(); vm.videoBounds = Rect(b.left.roundToInt(), b.top.roundToInt(), b.right.roundToInt(), b.bottom.roundToInt())
                    })
                    if (!docked && state.desktop.status != DesktopStatus.STREAMING && state.desktop.status != DesktopStatus.PAIRING) {
                        Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(stringResource(desktopStatusLabel(state.desktop.status)),color=Muted,fontSize=13.sp)
                            if (state.desktop.status in listOf(DesktopStatus.FAILED,DesktopStatus.IDLE)) {
                                TextButton(onClick = { if(state.connection==ConnectionStatus.CONNECTED)vm.retryDesktop()else vm.settingsVisible=true }) { Text(stringResource(if(state.connection==ConnectionStatus.CONNECTED)R.string.retry else R.string.open_host_settings)) }
                            } else CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp, color = Blue)
                        }
                    }
                }
                if (!hideChrome && !vm.settingsVisible) when (vm.layout) {
                    DesktopLayout.Floating -> {
                        Row(Modifier.absoluteOffset(fx, fy).width(floatingWidth).height(24.dp).background(KiteColors.Raised, RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp)).pointerInput(availableWidth, availableHeight) {
                            detectDragGestures(onDragEnd = updatePip) { change, amount ->
                                change.consume()
                                val rangeX = with(density) { (availableWidth - floatingWidth - 12.dp).toPx() }.coerceAtLeast(1f)
                                val rangeY = with(density) { (availableHeight - floatingHeight - 154.dp).toPx() }.coerceAtLeast(1f)
                                vm.floatingX = (vm.floatingX + amount.x / rangeX).coerceIn(0f, 1f)
                                vm.floatingY = (vm.floatingY + amount.y / rangeY).coerceIn(0f, 1f)
                            }
                        }, verticalAlignment = Alignment.CenterVertically) {
                            Text(stringResource(if (state.desktop.status == DesktopStatus.STREAMING) R.string.desktop_live else R.string.desktop_connecting), color = KiteColors.Accent, fontSize = 11.sp, modifier = Modifier.padding(start = 10.dp).weight(1f))
                            ProductIcon(Mark.Eye, Color.White, Modifier.size(16.dp).padding(end = 2.dp))
                        }
                        Row(Modifier.absoluteOffset(fx, fy + floatingHeight - 26.dp).width(floatingWidth).height(26.dp), horizontalArrangement = Arrangement.End) {
                            MiniAction(Mark.Dock, R.string.mode_docked) { vm.changeLayout(DesktopLayout.Docked) }
                            MiniAction(Mark.Split, R.string.mode_split) { vm.changeLayout(DesktopLayout.Split) }
                            MiniAction(Mark.Expand, R.string.mode_fullscreen) { vm.changeLayout(DesktopLayout.Fullscreen) }
                            MiniAction(Mark.Close, R.string.close_desktop) { vm.closeDesktop() }
                        }
                    }
                    DesktopLayout.Docked -> Box(Modifier.align(Alignment.CenterEnd).size(48.dp).clickable { vm.changeLayout(DesktopLayout.Floating) }.semantics { contentDescription = restoreDesktop }, contentAlignment = Alignment.Center) { Box(Modifier.size(32.dp).background(Paper,RoundedCornerShape(16.dp)),contentAlignment=Alignment.Center){ProductIcon(Mark.Monitor,modifier=Modifier.size(18.dp))} }
                    DesktopLayout.Split -> Unit
                    DesktopLayout.Fullscreen -> {
                        Row(Modifier.align(Alignment.TopStart).padding(10.dp).widthIn(max=(desktopWidth-24.dp).coerceAtLeast(90.dp)).onGloballyPositioned{chromeBounds["top"]=it.boundsInWindow()}.background(KiteColors.Background.copy(alpha=.88f),RoundedCornerShape(18.dp)).clickable{vm.returnToChat(landscape)}.padding(horizontal=12.dp,vertical=6.dp),verticalAlignment=Alignment.CenterVertically){
                            StatusDot(if(state.agentPause==AgentPauseStatus.PAUSED)"blocked" else agent?.status?:"idle")
                            Spacer(Modifier.width(8.dp))
                            val activity=timeline.messages.lastOrNull()?.tool?.takeIf{!it.complete&&agent?.status=="working"}
                            Text(if(activity!=null)agent?.agent.orEmpty()+" · "+activity.summary else agent?.let { agentTitle(it) }?:stringResource(R.string.desktop_title),fontSize=12.sp,maxLines=1,overflow=TextOverflow.Ellipsis)
                        }
                        FullscreenRail(vm,(availableHeight-safeTop-safeBottom).coerceAtLeast(48.dp),::toggleRemoteKeyboard,{vm.returnToChat(landscape)},{vm.rotateDesktop(landscape)},{gestureHelp=true},enterPip,Modifier.align(Alignment.TopEnd).padding(top=safeTop))
                    }
                    else -> Unit
                }
                if(takeover&&!hideChrome&&!vm.settingsVisible&&vm.layout==DesktopLayout.Fullscreen&&imeVisible){
                    RemoteKeyStrip(vm,Modifier.align(Alignment.BottomStart).imePadding().onGloballyPositioned{chromeBounds["toolbar"]=it.boundsInWindow()})
                }
                if(vm.layout==DesktopLayout.Fullscreen&&takeover&&!hideChrome&&!docked&&!drawerActive)VirtualMouse(vm,videoReference.value,desktopWidth,availableHeight,safeTop,safeBottom,bridgeActive){mouseBounds=it}

            }
            if (vm.settingsVisible && !hideChrome) Surface(Modifier.fillMaxSize(), color = KiteColors.OnAccent) { HostSetupScreen(vm) { vm.settingsVisible = false } }
        }
    }
    if (!hideChrome && !vm.settingsVisible) state?.pendingHostKey?.let { p -> AlertDialog(onDismissRequest = vm::disconnect,
        title = { Text(stringResource(R.string.host_key_title)) }, text = { Column { Text(p.address); Text(p.fingerprint, fontFamily = FontFamily.Monospace); Text(stringResource(R.string.host_key_hint)) } },
        confirmButton = { TextButton(onClick = { vm.trust(p.hostId, p.fingerprint) }) { Text(stringResource(R.string.host_key_trust)) } }, dismissButton = { TextButton(onClick = vm::disconnect) { Text(stringResource(R.string.cancel)) } }) }
    state?.desktop?.takeIf { it.status == DesktopStatus.PAIRING && !hideChrome }?.let { d -> AlertDialog(onDismissRequest = vm::closeDesktop,
        title = { Text(stringResource(R.string.pairing_title)) }, text = { Column { Text(d.pairingName.orEmpty()); Text(d.pin.orEmpty(), fontSize = 36.sp, fontFamily = FontFamily.Monospace); Text(stringResource(R.string.pairing_hint)) } },
        confirmButton = { TextButton(onClick = { d.pin?.let(vm::approve) }) { Text(stringResource(R.string.auto_confirm_pairing)) } }, dismissButton = { TextButton(onClick = vm::closeDesktop) { Text(stringResource(R.string.cancel)) } }) }
    if (newSession && !hideChrome) AlertDialog(onDismissRequest = { if (!vm.creatingAgent) newSession = false }, title = { Text(stringResource(R.string.new_session)) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.new_session_hint), fontSize = 14.sp)
            HarnessChoices(availableProviders, provider) { provider = it }
            if (availableProviders.isNotEmpty()) Text(stringResource(if (availableProviders.none { it.id == provider }) R.string.harness_unavailable else if (provider == "claude") R.string.claude_draft_hint else if (provider == "codex") R.string.codex_session_hint else R.string.harness_session_hint), fontSize = 12.sp, color = Muted)
            OutlinedTextField(sessionName, { sessionName = it }, label = { Text(stringResource(R.string.session_name)) }, singleLine = true)
            OutlinedTextField(project, { project = it }, label = { Text(stringResource(R.string.project_directory)) }, singleLine = true)
        }
    }, confirmButton = { TextButton(onClick = { vm.createSession(provider, project, sessionName); newSession = false; sessionName = "" }, enabled = !vm.creatingAgent && !vm.sessionBusy && availableProviders.any { it.id == provider }) { Text(stringResource(if (provider == "claude") R.string.save_session else R.string.start_session)) } }, dismissButton = { TextButton(onClick = { newSession = false }) { Text(stringResource(R.string.cancel)) } })
    if (gestureHelp && !hideChrome) AlertDialog(onDismissRequest = { gestureHelp = false }, title = { Text(stringResource(R.string.gesture_help)) }, text = { Text(stringResource(R.string.gesture_help_text)) }, confirmButton = { TextButton(onClick = { gestureHelp = false }) { Text(stringResource(R.string.dismiss)) } })
    state?.question?.takeIf { it.id == answerRequest && !hideChrome }?.let { question ->
        ModalBottomSheet(onDismissRequest = { answerRequest = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().heightIn(max = (LocalConfiguration.current.screenHeightDp * .8f).dp).verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).navigationBarsPadding().imePadding()) {
                ModelQuestionCard(vm, question, state.connection == ConnectionStatus.CONNECTED && state.agentPause == AgentPauseStatus.RUNNING)
                Spacer(Modifier.height(12.dp))
            }
        }
    }
    if(noticeCenter&&!hideChrome)AgentNotificationCenter(vm,onOpen={id->noticeCenter=false;vm.openAgentNotification(id)},onClose={noticeCenter=false})
    if(modelSettings&&providerInfo?.models==true&&!hideChrome)AlertDialog(onDismissRequest={modelSettings=false},title={Text(stringResource(R.string.chat_model_settings))},text={Column(Modifier.heightIn(max=440.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)){
        ComposerSettings(vm,state?.agents?.find{it.id==state.selectedAgent},state?.composer,providerInfo?.models == true)
        if(providerInfo?.usage == true) AccountUsageAccess(state?.composer?.usage){modelSettings=false;usageDialog=true;vm.refreshUsage()}
    }},confirmButton={TextButton(onClick={modelSettings=false}){Text(stringResource(R.string.dismiss))}})
    if(usageDialog&&providerInfo?.usage==true&&!hideChrome)AccountUsageDialog(state?.composer?.usage,currentSession?.provider?:state?.agents?.find{it.id==state.selectedAgent}?.agent,state?.connection==ConnectionStatus.CONNECTED&&state.composer.ready,vm::refreshUsage){usageDialog=false}
    if ((terminal || diagnostics) && !hideChrome) AlertDialog(onDismissRequest = { terminal = false; diagnostics = false }, title = { Text(stringResource(if (terminal) R.string.terminal_output else R.string.connection_diagnostics)) }, text = {
        Column {
        if (terminal && state?.selectedAgent != null) {
            Text(stringResource(R.string.terminal_keys_hint), fontSize = 12.sp, color = Muted)
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf("↑" to "Up", "↓" to "Down", "Enter" to "Enter", "Esc" to "Escape", "Tab" to "Tab").forEach { (label, key) ->
                    KeyButton(text = label) { state.selectedAgent?.let { vm.terminalKey(it, key) } }
                }
            }
        }
        SelectionContainer { Text(if (terminal) state?.agentOutput.orEmpty() else buildString {
            append(state?.let(::connectionDiagnostics).orEmpty()); append("\n${state?.desktop?.sourceWidth}×${state?.desktop?.sourceHeight} → ${state?.desktop?.width}×${state?.desktop?.height} @${state?.desktop?.fps}\n")
            append(frameCounts); state?.failure?.let { append("\n${it.details}") }; state?.composer?.details?.takeIf { it.isNotBlank() }?.let { append("\n$it") }
            state?.history?.details?.takeIf{it.isNotBlank()}?.let{append("\n$it")}
            videoReference.value?.performance?.let{append("\n\n");append(it.diagnostics())}
            videoReference.value?.audioPerformance?.let{append("\n\n");append(it.diagnostics())}
        }, fontFamily = FontFamily.Monospace, fontSize = 12.sp, modifier = Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) }
        }
    }, confirmButton = { TextButton(onClick = { terminal = false; diagnostics = false }) { Text(stringResource(R.string.dismiss)) } })
    val platformError by vm.engine.platformError.collectAsState()
    val error = warning ?: platformError
    vm.sessionToDelete?.takeIf { !hideChrome }?.let { (host, session) ->
        DeleteSessionDialog(sessionTitle(session), state?.selectedHost == host && state.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy, vm::deleteSession, vm::dismissSessionDeletion)
    }
    if (error != null && !hideChrome) AlertDialog(onDismissRequest = { warning = null; vm.clearLocalError() }, title = { Text(stringResource(R.string.error_action)) }, text = { Text(error) }, confirmButton = { TextButton(onClick = { warning = null; vm.clearLocalError() }) { Text(stringResource(R.string.dismiss)) } })
}

private fun Modifier.borderBottom() = border(0.5.dp, Line)
private fun Modifier.borderTop() = border(0.5.dp, Line)
@Composable private fun agentTitle(a: AgentInfo): String {
    val id = a.agent ?: a.name ?: "agent"
    val provider = providerLabel(id)
    val project = a.name?.takeIf { it != id && it != provider && !it.matches(Regex("kite-(codex|claude)-[0-9a-f]{8}")) } ?: a.cwd?.trimEnd('/')?.substringAfterLast('/') ?: a.title
    return if (project.isNullOrBlank() || project == provider) provider else "$provider · $project"
}
@Composable private fun statusLabel(status: String, pause: AgentPauseStatus, launching: Boolean = false): String = stringResource(when {
    launching -> R.string.agent_starting_short
    pause == AgentPauseStatus.PAUSED -> R.string.agent_paused
    pause == AgentPauseStatus.UNKNOWN -> R.string.agent_checking
    status == "blocked" -> R.string.agent_blocked
    status == "working" -> R.string.agent_working
    status == "done" -> R.string.agent_done
    status == "idle" -> R.string.agent_idle
    else -> R.string.agent_unknown
})
@Composable private fun StatusDot(status: String) { Box(Modifier.size(9.dp).background(if (status == "blocked") Amber else if (status == "working") Blue else Muted, RoundedCornerShape(9.dp))) }
@Composable private fun MiniAction(mark: Mark, label: Int, action: () -> Unit) { val description = stringResource(label); IconButton(onClick = action, modifier = Modifier.size(28.dp).semantics { contentDescription = description }) { ProductIcon(mark, modifier = Modifier.size(16.dp)) } }
@Composable private fun KeyButton(text: String? = null, icon: Mark? = null, label: Int = R.string.keyboard, active: Boolean = false, action: () -> Unit) {
    val translated = stringResource(label)
    val description = if (label == R.string.zoom_in || label == R.string.zoom_out) translated else text ?: translated
    Surface(onClick = action, color = if (active) KiteColors.AccentSurface else KiteColors.Raised, shape = RoundedCornerShape(10.dp), modifier = Modifier.height(40.dp).widthIn(min = 40.dp).semantics { contentDescription = description }) {
        Row(Modifier.padding(horizontal = 9.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) { if (icon != null) ProductIcon(icon, if (active) Blue else KiteColors.Muted, Modifier.size(20.dp)); if (text != null) { if (icon != null) Spacer(Modifier.width(5.dp)); Text(text, fontSize = 14.sp, color = if (active) Blue else Muted) } }
    }
}
@Composable private fun MessageCard(vm: HostCheckViewModel, updatePip: () -> Unit, inspect: () -> Unit, message: MessageInfo, streaming: Boolean = false) {
    if (message.questionReplies.isNotEmpty()) {
        AnsweredQuestions(message.questionReplies)
        return
    }
    val user = message.kind == MessageKind.USER
    val context = LocalContext.current
    val copyLabel = stringResource(R.string.copy_reply)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = if (user) Arrangement.End else Arrangement.Start) {
        Surface(color = if (user) KiteColors.UserBubble else Paper, shape = RoundedCornerShape(19.dp), modifier = Modifier.widthIn(max = 340.dp)) {
            Column(Modifier.padding(horizontal = 15.dp, vertical = 11.dp)) {
                MessageAttachments(vm, message.attachments.filterNot { it.generatedImage }, updatePip, inspect)
                GeneratedImages(vm, message.attachments.filter { it.generatedImage }, onExternalActivityChanged = updatePip, onOpenPreview = inspect)
                if (message.text.isNotBlank()) {
                    if (user) SelectionContainer { Text(message.text, color = KiteColors.Accent, fontSize = 16.sp, lineHeight = 25.sp) }
                    else {
                        MarkdownMessage(message.text,streaming=streaming)
                        IconButton(onClick = {
                            (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText(copyLabel, message.text))
                        }, modifier = Modifier.align(Alignment.End).size(40.dp).semantics { contentDescription = copyLabel }) {
                            ProductIcon(Mark.Clipboard, Muted, Modifier.size(17.dp))
                        }
                    }
                }
            }
        }
    }
}
