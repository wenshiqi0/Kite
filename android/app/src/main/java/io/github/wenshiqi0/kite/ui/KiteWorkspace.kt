package io.github.wenshiqi0.kite.ui

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Rect
import android.speech.RecognizerIntent
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
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
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private val Ink = Color(0xFF161715)
private val Muted = Color(0xFF888982)
private val Paper = Color(0xFFF5F4EF)
private val Blue = Color(0xFF2D85D8)
private val Line = Color(0xFFDDDDD6)
private val Amber = Color(0xFFF5C847)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun KiteWorkspace(vm: HostCheckViewModel, enterPip: () -> Unit, updatePip: () -> Unit) {
    val current by vm.engine.state.collectAsState()
    val state = current
    val context = LocalContext.current
    val voiceUnavailable = stringResource(R.string.voice_unavailable)
    val sendTextFailure = stringResource(R.string.error_send_text)
    val restoreDesktop = stringResource(R.string.restore_desktop)
    val returnChat = stringResource(R.string.return_chat)
    val sendPromptLabel = stringResource(R.string.send_prompt)
    val menuDescription = stringResource(R.string.navigation_menu)
    val moreDescription = stringResource(R.string.more_actions)
    val splitDescription = stringResource(R.string.resize_split)
    val addDescription = stringResource(R.string.compose_add)
    val voiceDescription = stringResource(R.string.voice_input)
    val frameCounts = stringResource(R.string.video_frame_counts, state?.desktop?.receivedFrames?.toLong() ?: 0L, state?.desktop?.renderedFrames?.toLong() ?: 0L)
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    var autoFullscreen by remember { mutableStateOf(false) }
    var tools by remember { mutableStateOf(false) }
    var newSession by remember { mutableStateOf(false) }
    var gestureHelp by remember { mutableStateOf(false) }
    var provider by remember { mutableStateOf("codex") }
    var project by remember { mutableStateOf("") }
    var remoteKeyboard by remember { mutableStateOf(false) }
    var remoteText by remember { mutableStateOf("") }
    var terminal by remember { mutableStateOf(false) }
    var diagnostics by remember { mutableStateOf(false) }
    var arrows by remember { mutableStateOf(false) }
    var warning by remember { mutableStateOf<String?>(null) }
    val chatScroll = rememberLazyListState()
    var followTail by remember { mutableStateOf(true) }
    var chatHeight by remember { mutableIntStateOf(0) }
    var previousAgent by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(chatScroll) {
        snapshotFlow { chatScroll.isScrollInProgress }.collect { scrolling -> if (!scrolling) followTail = !chatScroll.canScrollForward }
    }
    LaunchedEffect(state?.selectedAgent, state?.conversation?.size, state?.approval?.id, chatHeight, vm.inPip, vm.pipTransition) {
        if (vm.inPip || vm.pipTransition || chatHeight < 16) return@LaunchedEffect
        val count = (state?.conversation?.size ?: 0) + (if (state?.approval != null) 1 else 0) + (if (state?.conversationAvailable == false && !state.agentOutput.isBlank()) 1 else 0) + 1
        if (previousAgent != state?.selectedAgent) followTail = true
        if (followTail) chatScroll.scrollToItem((count - 1).coerceAtLeast(0))
        previousAgent = state?.selectedAgent
    }
    val voice = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        vm.externalActivity = false
        updatePip()
        if (result.resultCode == Activity.RESULT_OK) result.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()?.let { value ->
            vm.draft = if (vm.draft.isBlank()) value else vm.draft + " " + value
        }
    }
    val hideChrome = vm.inPip || vm.pipTransition
    LaunchedEffect(hideChrome) { drawer.close(); if (hideChrome) { tools = false; terminal = false; diagnostics = false; remoteKeyboard = false; arrows = false; newSession = false; gestureHelp = false } }
    LaunchedEffect(landscape, vm.inPip) {
        if (vm.inPip) return@LaunchedEffect
        if (landscape && vm.layout == DesktopLayout.Split) { autoFullscreen = true; vm.changeLayout(DesktopLayout.Fullscreen, rotate = false) }
        else if (!landscape && autoFullscreen && vm.layout == DesktopLayout.Fullscreen) { autoFullscreen = false; vm.changeLayout(DesktopLayout.Split) }
    }
    LaunchedEffect(vm.layout, vm.inPip, state?.desktop?.status, state?.desktop?.sourceWidth, state?.desktop?.sourceHeight) { updatePip() }
    LaunchedEffect(vm.settingsVisible) { if (vm.settingsVisible) vm.exitTakeover() }
    BackHandler(enabled = vm.settingsVisible || terminal || diagnostics || vm.showDesktop) {
        when {
            terminal -> terminal = false
            diagnostics -> diagnostics = false
            vm.settingsVisible -> vm.settingsVisible = false
            vm.layout == DesktopLayout.Fullscreen -> vm.returnToChat(landscape)
            remoteKeyboard -> remoteKeyboard = false
            vm.showDesktop -> enterPip()
        }
    }
    ModalNavigationDrawer(drawerState = drawer, scrimColor = if (hideChrome) Color.Transparent else DrawerDefaults.scrimColor, gesturesEnabled = !hideChrome && vm.layout != DesktopLayout.Fullscreen, drawerContent = {
        ModalDrawerSheet(Modifier.width(310.dp), drawerContainerColor = Color.White) {
            Text(stringResource(R.string.app_name), fontSize = 26.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(24.dp))
            Text(stringResource(R.string.agents_title), color = Muted, modifier = Modifier.padding(horizontal = 24.dp, vertical = 10.dp))
            state?.agents?.forEach { agent ->
                NavigationDrawerItem(selected = agent.id == state.selectedAgent, onClick = { vm.selectAgent(agent.id); scope.launch { drawer.close() } },
                    label = { Column { Text(agentTitle(agent), maxLines = 1, overflow = TextOverflow.Ellipsis); Text(agent.status, fontSize = 12.sp, color = Muted) } },
                    icon = { StatusDot(agent.status) }, modifier = Modifier.padding(horizontal = 12.dp))
            }
            if (state?.agents.isNullOrEmpty()) Text(stringResource(R.string.workspace_no_agents_short), color = Muted, modifier = Modifier.padding(24.dp))
            TextButton(onClick = { newSession = true; scope.launch { drawer.close() } }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.creatingAgent, modifier = Modifier.padding(horizontal = 12.dp)) { ProductIcon(Mark.Plus); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.new_session)) }
            Spacer(Modifier.weight(1f))
            TextButton(onClick = { vm.settingsVisible = true; scope.launch { drawer.close() } }, modifier = Modifier.padding(12.dp)) {
                ProductIcon(Mark.Settings); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.host_settings))
            }
        }
    }) {
        BoxWithConstraints(Modifier.fillMaxSize().background(if (vm.layout == DesktopLayout.Fullscreen || vm.inPip) Paper else Color.White)) {
            val density = LocalDensity.current
            val availableWidth = maxWidth
            val availableHeight = maxHeight
            val fullscreen = vm.layout == DesktopLayout.Fullscreen || vm.inPip
            val agent = state?.agents?.find { it.id == state.selectedAgent }
            val takeover = state?.desktop?.takeover == true && !hideChrome && !vm.settingsVisible
            val split = vm.layout == DesktopLayout.Split && !fullscreen
            val splitHeight = (availableHeight * vm.splitRatio).coerceIn(140.dp, (availableHeight * .65f).coerceAtLeast(140.dp))
            if (!fullscreen && !hideChrome) Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).imePadding()) {
                Row(Modifier.fillMaxWidth().height(56.dp).borderBottom(), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = { scope.launch { drawer.open() } }, modifier = Modifier.semantics { contentDescription = menuDescription }) { ProductIcon(Mark.Menu) }
                    Text(agent?.let(::agentTitle) ?: stringResource(R.string.app_name), modifier = Modifier.weight(1f), fontSize = 18.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (agent != null) { StatusDot(if (state.agentPause == AgentPauseStatus.PAUSED) "blocked" else agent.status); Spacer(Modifier.width(6.dp)); Text(statusLabel(agent.status, state.agentPause, agent.launchPending), color = Muted, fontSize = 13.sp) }
                    Box {
                        IconButton(onClick = { tools = true }, modifier = Modifier.semantics { contentDescription = moreDescription }) { ProductIcon(Mark.More) }
                        DropdownMenu(expanded = tools, onDismissRequest = { tools = false }) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.new_session)) }, onClick = { tools = false; newSession = true }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.creatingAgent)
                            DropdownMenuItem(text = { Text(stringResource(R.string.open_desktop)) }, leadingIcon = { ProductIcon(Mark.Monitor) }, onClick = { tools = false; vm.openDesktop() }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            DropdownMenuItem(text = { Text(stringResource(R.string.mode_split)) }, leadingIcon = { ProductIcon(Mark.Split) }, onClick = { tools = false; vm.changeLayout(DesktopLayout.Split) }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            if (agent?.canPause == true) DropdownMenuItem(text = { Text(stringResource(if (state.agentPause == AgentPauseStatus.RUNNING) R.string.pause_agent else R.string.resume_agent)) }, onClick = { tools = false; vm.pauseAgent(agent.id) })
                            DropdownMenuItem(text = { Text(stringResource(R.string.terminal_output)) }, onClick = { tools = false; terminal = true }, enabled = agent != null)
                            DropdownMenuItem(text = { Text(stringResource(R.string.connection_diagnostics)) }, onClick = { tools = false; diagnostics = true })
                            DropdownMenuItem(text = { Text(stringResource(R.string.host_settings)) }, onClick = { tools = false; vm.settingsVisible = true })
                        }
                    }
                }
                if (split) {
                    Spacer(Modifier.height(splitHeight))
                    Box(Modifier.fillMaxWidth().height(18.dp).semantics { contentDescription = splitDescription }.pointerInput(availableHeight) {
                        detectDragGestures { change, amount -> change.consume(); vm.splitRatio = (vm.splitRatio + amount.y / with(density) { availableHeight.toPx() }).coerceIn(.20f, .65f) }
                    }, contentAlignment = Alignment.Center) { Box(Modifier.width(62.dp).height(4.dp).background(Color(0xFFA3A39F), RoundedCornerShape(2.dp))) }
                }
                if (state == null || !state.initialized) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Blue)
                if (state != null && state.connection != ConnectionStatus.CONNECTED) {
                    Column(Modifier.fillMaxWidth().background(Paper).padding(horizontal = 16.dp, vertical = 8.dp)) {
                        Text(connectionStepLabel(state), color = Muted)
                        state.failure?.let { Text(it.details, color = MaterialTheme.colorScheme.error, fontSize = 12.sp, maxLines = 4, overflow = TextOverflow.Ellipsis) }
                        TextButton(onClick = { vm.settingsVisible = true }) { Text(stringResource(R.string.open_host_settings)) }
                    }
                }
                LazyColumn(Modifier.weight(1f).fillMaxWidth().onSizeChanged { chatHeight = it.height }, state = chatScroll, contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (state?.failure?.kind == FailureKind.AGENT_START) item {
                        Surface(color = Color(0xFFFFF2D0), shape = RoundedCornerShape(16.dp)) {
                            Column(Modifier.padding(16.dp)) {
                                Text(stringResource(R.string.agent_start_failed))
                                Text(state.failure!!.details, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                                TextButton(onClick = { vm.engine.command { vm.engine.core.clearFailure() } }) { Text(stringResource(R.string.dismiss)) }
                            }
                        }
                    }

                    if (agent == null) item {
                        Column(Modifier.fillMaxWidth().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            ProductIcon(Mark.Monitor, Muted, Modifier.size(36.dp))
                            Text(stringResource(if (state?.connection == ConnectionStatus.CONNECTED) R.string.workspace_no_agents else R.string.workspace_welcome), color = Muted, modifier = Modifier.padding(16.dp))
                            if (state?.connection == ConnectionStatus.CONNECTED) {
                                val preparing = state.connectionStep == ConnectionStep.AGENT && state.failure == null
                                Button(onClick = vm::prepareAgent, enabled = !preparing) { Text(stringResource(if (preparing) R.string.connection_agent else R.string.retry_agent)) }
                                TextButton(onClick = vm::openDesktop) { Text(stringResource(R.string.open_desktop)) }
                            } else Button(onClick = { vm.settingsVisible = true }) { Text(stringResource(R.string.open_host_settings)) }
                        }
                    }
                    state?.conversation?.let { messages -> items(messages, key = { it.id }) { message -> MessageCard(message) { vm.openDesktop() } } }
                    if (state != null && agent != null && !state.conversationAvailable && state.agentOutput.isNotBlank()) item {
                        Surface(color = Paper, shape = RoundedCornerShape(16.dp), modifier = Modifier.fillMaxWidth().clickable { terminal = true }) {
                            Column(Modifier.padding(14.dp)) {
                                Text(stringResource(R.string.terminal_output), color = Muted, fontSize = 12.sp)
                                Text(state.agentOutput.takeLast(3000), fontFamily = FontFamily.Monospace, fontSize = 12.sp, maxLines = 10, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                    state?.approval?.let { approval -> item { ApprovalCard(approval, state.agentPause == AgentPauseStatus.RUNNING) { state.selectedAgent?.let { id -> vm.respond(id, approval.id, it) } } } }
                    if (agent?.status == "blocked" && state?.approval == null) item {
                        TextButton(onClick = { terminal = true }) { Text(stringResource(R.string.agent_waiting_open_terminal), color = Color(0xFF946307)) }
                    }
                    item { Spacer(Modifier.height(8.dp)) }
                }
                val composeReason = when {
                    state?.connection != ConnectionStatus.CONNECTED -> R.string.composer_no_connection
                    agent == null -> R.string.composer_no_agent
                    agent.launchPending -> R.string.composer_starting
                    state.agentPause == AgentPauseStatus.PAUSED -> R.string.composer_paused
                    agent.status == "blocked" -> R.string.composer_blocked_hint
                    state.agentPause != AgentPauseStatus.RUNNING -> R.string.composer_checking
                    else -> null
                }
                if (composeReason != null) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(composeReason), color = Muted, fontSize = 12.sp, modifier = Modifier.weight(1f))
                    if (agent != null && agent.launchPending) TextButton(onClick = { terminal = true }) { Text(stringResource(R.string.terminal_output)) }
                }
                Row(Modifier.fillMaxWidth().borderTop().padding(horizontal = 8.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box {
                        var plus by remember { mutableStateOf(false) }
                        IconButton(onClick = { plus = true }, modifier = Modifier.semantics { contentDescription = addDescription }) { ProductIcon(Mark.Plus) }
                        DropdownMenu(plus, { plus = false }) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.mode_floating)) }, onClick = { plus = false; vm.openDesktop() }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            DropdownMenuItem(text = { Text(stringResource(R.string.mode_split)) }, onClick = { plus = false; vm.changeLayout(DesktopLayout.Split) }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                            DropdownMenuItem(text = { Text(stringResource(R.string.mode_fullscreen)) }, onClick = { plus = false; vm.changeLayout(DesktopLayout.Fullscreen) }, enabled = state?.connection == ConnectionStatus.CONNECTED)
                        }
                    }
                    OutlinedTextField(vm.draft, { vm.draft = it }, modifier = Modifier.weight(1f), shape = RoundedCornerShape(28.dp), maxLines = 5,
                        placeholder = { Text(stringResource(if (agent?.status == "blocked") R.string.composer_blocked else R.string.composer_hint), color = Muted, fontSize = 15.sp) })
                    if (vm.draft.isNotBlank()) IconButton(onClick = { agent?.id?.let(vm::sendPrompt) }, modifier = Modifier.semantics { contentDescription = sendPromptLabel }, enabled = agent != null && !vm.sending && composeReason == null) {
                        if (vm.sending) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) else ProductIcon(Mark.Send, Blue)
                    } else IconButton(onClick = {
                        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                        vm.externalActivity = true; updatePip()
                        runCatching { voice.launch(intent) }.onFailure { vm.externalActivity = false; updatePip(); warning = voiceUnavailable }
                    }, modifier = Modifier.semantics { contentDescription = voiceDescription }) { ProductIcon(Mark.Mic) }
                }
            }
            if (state != null && vm.showDesktop && state.connection == ConnectionStatus.CONNECTED) {
                val aspect = if (state.desktop.sourceWidth > 0u && state.desktop.sourceHeight > 0u) (state.desktop.sourceWidth.toFloat() / state.desktop.sourceHeight.toFloat()).coerceIn(.1f, 10f) else 16f / 9f
                val safeTop = with(density) { WindowInsets.statusBars.getTop(this).toDp() }
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
                    vm.layout == DesktopLayout.Fullscreen -> { frameX = 12.dp; frameY = 56.dp; frameW = availableWidth - 24.dp; frameH = (availableHeight - if (takeover) 118.dp else 94.dp).coerceAtLeast(80.dp) }
                    split -> { frameX = 0.dp; frameY = safeTop + 56.dp; frameW = availableWidth; frameH = (splitHeight - 38.dp).coerceAtLeast(80.dp) }
                    else -> { frameX = fx; frameY = fy + 12.dp; frameW = floatingWidth; frameH = floatingHeight - 38.dp }
                }
                if (vm.layout == DesktopLayout.Floating && !vm.inPip && !docked) {
                    Box(Modifier.absoluteOffset(fx, fy).width(floatingWidth).height(floatingHeight).background(Paper, RoundedCornerShape(14.dp)).border(1.dp, Color(0xFFB8B8B1), RoundedCornerShape(14.dp)))
                }
                Box(Modifier.absoluteOffset(frameX, frameY).size(frameW, frameH).clipToBounds().background(Paper)) {
                    AndroidView(factory = { VideoSurface(it, vm::videoReady, vm::videoGone) }, update = { surface ->
                        surface.takeover = takeover && !docked
                        surface.trackpad = vm.trackpad
                        surface.panMode = vm.panMode
                        surface.gesturesEnabled = !hideChrome && !docked
                        surface.pip = hideChrome
                        surface.setSourceSize(state.desktop.sourceWidth.toInt(), state.desktop.sourceHeight.toInt())
                        surface.onZoomChanged = { vm.zoom = it }
                        surface.zoom = vm.zoom
                    }, modifier = Modifier.fillMaxSize().onGloballyPositioned { p ->
                        val b = p.boundsInWindow(); vm.videoBounds = Rect(b.left.roundToInt(), b.top.roundToInt(), b.right.roundToInt(), b.bottom.roundToInt())
                    })
                    if (!docked && state.desktop.status != DesktopStatus.STREAMING && state.desktop.status != DesktopStatus.PAIRING) {
                        Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                            if (state.desktop.status == DesktopStatus.FAILED) TextButton(onClick = vm::retryDesktop) { Text(stringResource(R.string.retry)) }
                            else CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp, color = Blue)
                        }
                    }
                }
                if (!hideChrome && !vm.settingsVisible) when (vm.layout) {
                    DesktopLayout.Floating -> {
                        Row(Modifier.absoluteOffset(fx, fy).width(floatingWidth).height(24.dp).background(Color(0xFFB2B2AD), RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp)).pointerInput(availableWidth, availableHeight) {
                            detectDragGestures(onDragEnd = updatePip) { change, amount ->
                                change.consume()
                                val rangeX = with(density) { (availableWidth - floatingWidth - 12.dp).toPx() }.coerceAtLeast(1f)
                                val rangeY = with(density) { (availableHeight - floatingHeight - 154.dp).toPx() }.coerceAtLeast(1f)
                                vm.floatingX = (vm.floatingX + amount.x / rangeX).coerceIn(0f, 1f)
                                vm.floatingY = (vm.floatingY + amount.y / rangeY).coerceIn(0f, 1f)
                            }
                        }, verticalAlignment = Alignment.CenterVertically) {
                            Text(stringResource(if (state.desktop.status == DesktopStatus.STREAMING) R.string.desktop_live else R.string.desktop_connecting), color = Color(0xFFB52B1D), fontSize = 11.sp, modifier = Modifier.padding(start = 10.dp).weight(1f))
                            ProductIcon(Mark.Eye, Color.White, Modifier.size(16.dp).padding(end = 2.dp))
                        }
                        Row(Modifier.absoluteOffset(fx, fy + floatingHeight - 26.dp).width(floatingWidth).height(26.dp), horizontalArrangement = Arrangement.End) {
                            MiniAction(Mark.Dock, R.string.mode_docked) { vm.changeLayout(DesktopLayout.Docked) }
                            MiniAction(Mark.Split, R.string.mode_split) { vm.changeLayout(DesktopLayout.Split) }
                            MiniAction(Mark.Expand, R.string.mode_fullscreen) { vm.changeLayout(DesktopLayout.Fullscreen) }
                            MiniAction(Mark.Close, R.string.close_desktop) { vm.closeDesktop() }
                        }
                    }
                    DesktopLayout.Docked -> Box(Modifier.align(Alignment.CenterEnd).width(34.dp).height(92.dp).background(Paper, RoundedCornerShape(topStart = 16.dp, bottomStart = 16.dp)).border(1.dp, Line, RoundedCornerShape(topStart = 16.dp, bottomStart = 16.dp)).clickable { vm.changeLayout(DesktopLayout.Floating) }.semantics { contentDescription = restoreDesktop }, contentAlignment = Alignment.Center) { ProductIcon(Mark.Monitor) }
                    DesktopLayout.Split -> Row(Modifier.absoluteOffset(0.dp, safeTop + 56.dp + splitHeight - 36.dp).fillMaxWidth().height(36.dp).background(Paper), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.End) {
                        Text(stringResource(if (state.desktop.status == DesktopStatus.STREAMING) R.string.desktop_live else R.string.desktop_connecting), color = Color(0xFFB52B1D), fontSize = 12.sp, modifier = Modifier.padding(start = 12.dp).weight(1f))
                        MiniAction(Mark.Eye, R.string.watch_only) { vm.exitTakeover() }
                        MiniAction(Mark.Touchpad, R.string.takeover) { vm.takeOver(!state.desktop.takeover) }
                        MiniAction(Mark.Dock, R.string.mode_floating) { vm.changeLayout(DesktopLayout.Floating) }
                        MiniAction(Mark.Expand, R.string.mode_fullscreen) { vm.changeLayout(DesktopLayout.Fullscreen) }
                    }
                    DesktopLayout.Fullscreen -> {
                        Row(Modifier.align(Alignment.TopStart).padding(14.dp).widthIn(max = (availableWidth - 270.dp).coerceAtLeast(90.dp)).background(Color.White, RoundedCornerShape(24.dp)).border(1.dp, Line, RoundedCornerShape(24.dp)).padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                            StatusDot(if (state.agentPause == AgentPauseStatus.PAUSED) "blocked" else agent?.status ?: "idle")
                            Spacer(Modifier.width(8.dp))
                            val activity = state.conversation.lastOrNull()?.tool?.takeIf { !it.complete && agent?.status == "working" }
                            Text(if (activity != null) "${agent?.agent.orEmpty()} · ${activity.summary}" else agent?.let(::agentTitle) ?: stringResource(R.string.desktop_title), fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Row(Modifier.align(Alignment.TopEnd).padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                            DesktopModeSwitch(takeover, state.desktop.status == DesktopStatus.STREAMING, vm::takeOver)
                            Spacer(Modifier.width(6.dp))
                            KeyButton(text = stringResource(if (landscape) R.string.rotate_portrait else R.string.rotate_landscape)) { vm.rotateDesktop(landscape) }
                            Spacer(Modifier.width(8.dp)); KeyButton(icon = Mark.Shrink, label = R.string.mode_floating) { vm.changeLayout(DesktopLayout.Floating) }
                        }
                        Row(Modifier.align(Alignment.TopCenter).padding(top = 60.dp).background(Color.White.copy(alpha = .94f), RoundedCornerShape(16.dp)), verticalAlignment = Alignment.CenterVertically) {
                            KeyButton(text = "−", label = R.string.zoom_out) { vm.zoom = (vm.zoom / 1.5f).coerceAtLeast(1f) }
                            Text(stringResource(R.string.zoom_value, vm.zoom), color = Muted, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 6.dp))
                            KeyButton(text = "+", label = R.string.zoom_in) { vm.zoom = (vm.zoom * 1.5f).coerceAtMost(8f) }
                            TextButton(onClick = vm::fitVideo) { Text(stringResource(R.string.fit_desktop)) }
                            KeyButton(text = stringResource(R.string.pan_viewport), active = vm.panMode) { vm.panMode = !vm.panMode }
                        }
                        if (!takeover) Row(Modifier.align(Alignment.BottomStart).padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                            ProductIcon(Mark.Eye, Muted); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.watch_gesture_hint), color = Muted, fontSize = 13.sp)
                            TextButton(onClick = { gestureHelp = true }) { Text(stringResource(R.string.gesture_help)) }
                        }
                        Box(Modifier.align(Alignment.CenterEnd).width(32.dp).height(82.dp).background(Color.White, RoundedCornerShape(topStart = 14.dp, bottomStart = 14.dp)).border(1.dp, Line, RoundedCornerShape(topStart = 14.dp, bottomStart = 14.dp)).clickable { vm.returnToChat(landscape) }.semantics { contentDescription = returnChat }, contentAlignment = Alignment.Center) { ProductIcon(Mark.Chat) }
                    }
                    else -> Unit
                }
                if (takeover && !hideChrome && !vm.settingsVisible && vm.layout == DesktopLayout.Fullscreen) {
                    val toolbarScroll = rememberScrollState()
                    val scrolling = if (availableWidth < 900.dp) Modifier.horizontalScroll(toolbarScroll) else Modifier
                    Row(Modifier.align(Alignment.BottomCenter).fillMaxWidth().background(Paper).then(scrolling).padding(horizontal = 10.dp, vertical = 9.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        KeyButton(icon = Mark.Keyboard, label = R.string.keyboard) { remoteKeyboard = !remoteKeyboard }
                        KeyButton(text = stringResource(R.string.shortcut_esc)) { vm.shortcut(27) }; KeyButton(text = stringResource(R.string.shortcut_tab)) { vm.shortcut(9) }
                        KeyButton(text = stringResource(R.string.key_ctrl), active = vm.ctrlHeld) { vm.modifier(true) }; KeyButton(text = stringResource(R.string.key_cmd), active = vm.metaHeld) { vm.modifier(false) }
                        Box {
                            KeyButton(icon = Mark.Pan, label = R.string.arrow_keys, active = arrows) { arrows = !arrows }
                            DropdownMenu(expanded = arrows, onDismissRequest = { arrows = false }) {
                                Column(Modifier.padding(10.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                    KeyButton(text = stringResource(R.string.key_up)) { vm.shortcut(38) }
                                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                        KeyButton(text = stringResource(R.string.key_left)) { vm.shortcut(37) }
                                        KeyButton(text = stringResource(R.string.key_down)) { vm.shortcut(40) }
                                        KeyButton(text = stringResource(R.string.key_right)) { vm.shortcut(39) }
                                    }
                                    KeyButton(text = stringResource(R.string.shortcut_enter)) { vm.shortcut(13) }
                                }
                            }
                        }
                        KeyButton(icon = Mark.Clipboard, label = R.string.paste_desktop) {
                            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                            val value = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
                            vm.releaseModifiers(); if (value.toByteArray().size > 4096 || !vm.sendText(value)) warning = sendTextFailure
                        }
                        if (availableWidth >= 900.dp) Spacer(Modifier.weight(1f)) else Spacer(Modifier.width(4.dp))
                        KeyButton(text = stringResource(if (vm.trackpad) R.string.trackpad else R.string.direct_touch), icon = Mark.Touchpad, active = vm.trackpad) { vm.trackpad = !vm.trackpad }
                        KeyButton(text = stringResource(R.string.mouse_left)) { vm.leftClick() }
                        KeyButton(text = stringResource(R.string.mouse_right)) { vm.rightClick() }
                        KeyButton(text = "?") { gestureHelp = true }
                    }
                }
            }
            if (vm.settingsVisible && !hideChrome) Surface(Modifier.fillMaxSize(), color = Color.White) { HostSetupScreen(vm) { vm.settingsVisible = false } }
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
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(selected = provider == "codex", onClick = { provider = "codex" }, label = { Text("Codex") })
                FilterChip(selected = provider == "claude", onClick = { provider = "claude" }, label = { Text("Claude") })
            }
            OutlinedTextField(project, { project = it }, label = { Text(stringResource(R.string.project_directory)) }, singleLine = true)
        }
    }, confirmButton = { TextButton(onClick = { vm.startAgent(provider, project); newSession = false }, enabled = !vm.creatingAgent) { Text(stringResource(R.string.start_session)) } }, dismissButton = { TextButton(onClick = { newSession = false }) { Text(stringResource(R.string.cancel)) } })
    if (gestureHelp && !hideChrome) AlertDialog(onDismissRequest = { gestureHelp = false }, title = { Text(stringResource(R.string.gesture_help)) }, text = { Text(stringResource(R.string.gesture_help_text)) }, confirmButton = { TextButton(onClick = { gestureHelp = false }) { Text(stringResource(R.string.dismiss)) } })
    if (remoteKeyboard && !hideChrome) AlertDialog(onDismissRequest = { remoteKeyboard = false }, title = { Text(stringResource(R.string.remote_text)) }, text = {
        OutlinedTextField(remoteText, { remoteText = it }, label = { Text(stringResource(R.string.remote_text)) })
    }, confirmButton = { TextButton(onClick = { vm.releaseModifiers(); if (remoteText.toByteArray().size <= 4096 && vm.sendText(remoteText)) { remoteText = ""; remoteKeyboard = false } else warning = sendTextFailure }) { Text(stringResource(R.string.send_text)) } })
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
            append(frameCounts); state?.failure?.let { append("\n${it.details}") }
        }, fontFamily = FontFamily.Monospace, fontSize = 12.sp, modifier = Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) }
        }
    }, confirmButton = { TextButton(onClick = { terminal = false; diagnostics = false }) { Text(stringResource(R.string.dismiss)) } })
    val platformError by vm.engine.platformError.collectAsState()
    val error = warning ?: platformError
    if (error != null && !hideChrome) AlertDialog(onDismissRequest = { warning = null; vm.clearLocalError() }, title = { Text(stringResource(R.string.error_action)) }, text = { Text(error) }, confirmButton = { TextButton(onClick = { warning = null; vm.clearLocalError() }) { Text(stringResource(R.string.dismiss)) } })
}

@Composable private fun DesktopModeSwitch(operating: Boolean, enabled: Boolean, change: (Boolean) -> Unit) {
    Row(Modifier.background(Color.White, RoundedCornerShape(10.dp)).border(1.dp, Line, RoundedCornerShape(10.dp))) {
        listOf(false, true).forEach { mode ->
            TextButton(onClick = { change(mode) }, enabled = enabled, colors = ButtonDefaults.textButtonColors(containerColor = if (mode == operating) Blue else Color.Transparent, contentColor = if (mode == operating) Color.White else Muted), shape = RoundedCornerShape(9.dp), contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)) {
                Text(stringResource(if (mode) R.string.mode_operate else R.string.mode_watch), fontSize = 14.sp)
            }
        }
    }
}

private fun Modifier.borderBottom() = border(0.5.dp, Line)
private fun Modifier.borderTop() = border(0.5.dp, Line)
private fun agentTitle(a: AgentInfo): String {
    val provider = a.agent ?: a.name ?: "agent"
    val project = a.name?.takeIf { it != provider && !it.matches(Regex("kite-(codex|claude)-[0-9a-f]{8}")) } ?: a.cwd?.trimEnd('/')?.substringAfterLast('/') ?: a.title
    return if (project.isNullOrBlank() || project == provider) provider else "$provider · $project"
}
@Composable private fun statusLabel(status: String, pause: AgentPauseStatus, launching: Boolean = false): String = stringResource(when {
    launching -> R.string.agent_starting_short
    pause == AgentPauseStatus.PAUSED -> R.string.agent_paused
    pause == AgentPauseStatus.UNKNOWN -> R.string.agent_checking
    status == "blocked" -> R.string.agent_blocked
    status == "working" -> R.string.agent_working
    else -> R.string.agent_idle
})
@Composable private fun StatusDot(status: String) { Box(Modifier.size(9.dp).background(if (status == "blocked") Amber else if (status == "working") Blue else Muted, RoundedCornerShape(9.dp))) }
@Composable private fun MiniAction(mark: Mark, label: Int, action: () -> Unit) { val description = stringResource(label); IconButton(onClick = action, modifier = Modifier.size(28.dp).semantics { contentDescription = description }) { ProductIcon(mark, modifier = Modifier.size(16.dp)) } }
@Composable private fun KeyButton(text: String? = null, icon: Mark? = null, label: Int = R.string.keyboard, active: Boolean = false, action: () -> Unit) {
    val translated = stringResource(label)
    val description = if (label == R.string.zoom_in || label == R.string.zoom_out) translated else text ?: translated
    Surface(onClick = action, color = if (active) Color(0xFFDDEBFA) else Color.White, shape = RoundedCornerShape(9.dp), border = androidx.compose.foundation.BorderStroke(1.dp, if (active) Blue else Color(0xFFBFBFB8)), modifier = Modifier.height(38.dp).widthIn(min = 38.dp).semantics { contentDescription = description }) {
        Row(Modifier.padding(horizontal = 9.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) { if (icon != null) ProductIcon(icon, if (active) Blue else Color(0xFF64645F), Modifier.size(20.dp)); if (text != null) { if (icon != null) Spacer(Modifier.width(5.dp)); Text(text, fontSize = 14.sp, color = if (active) Blue else Muted) } }
    }
}
@Composable private fun MessageCard(message: MessageInfo, desktop: () -> Unit) {
    val user = message.kind == MessageKind.USER
    val tool = message.tool
    var expanded by remember(message.id) { mutableStateOf(false) }
    if (tool != null) Surface(color = Color.White, shape = RoundedCornerShape(13.dp), border = androidx.compose.foundation.BorderStroke(1.dp, if (tool.computer) Color(0xFF85B3F4) else Line), modifier = Modifier.fillMaxWidth().clickable { if (tool.computer) desktop() else expanded = !expanded }) {
        Column(Modifier.padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) { ProductIcon(if (tool.computer) Mark.Monitor else Mark.Terminal, if (tool.computer) Blue else Muted, Modifier.size(18.dp)); Spacer(Modifier.width(9.dp)); Text(tool.label + if (tool.summary.isNotBlank()) " · " + tool.summary else "", color = if (tool.computer) Blue else Muted, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f)); if (!tool.complete) CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 1.5.dp) }
            if (expanded) SelectionContainer { Text(tool.arguments + if (tool.output.isNotBlank()) "\n\n" + tool.output else "", fontFamily = FontFamily.Monospace, fontSize = 12.sp, modifier = Modifier.padding(top = 10.dp)) }
        }
    } else Row(Modifier.fillMaxWidth(), horizontalArrangement = if (user) Arrangement.End else Arrangement.Start) {
        Surface(color = if (user) Color(0xFFE2EEFC) else Paper, shape = RoundedCornerShape(19.dp), modifier = Modifier.widthIn(max = 340.dp)) {
            SelectionContainer { Text(message.text, color = if (user) Color(0xFF2269AA) else Ink, fontSize = 16.sp, lineHeight = 25.sp, modifier = Modifier.padding(horizontal = 15.dp, vertical = 11.dp)) }
        }
    }
}
@Composable private fun ApprovalCard(approval: ApprovalInfo, enabled: Boolean, choose: (Int) -> Unit) {
    val requestScroll = remember(approval.id) { ScrollState(0) }
    Surface(color = Color(0xFFFFF2D0), shape = RoundedCornerShape(19.dp), border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFE3BD50)), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.approval_title), fontWeight = FontWeight.SemiBold, color = Color(0xFF8C5D00))
            Text(approval.prompt, color = Color(0xFF8C5D00), fontSize = 14.sp, modifier = Modifier.heightIn(max = 210.dp).verticalScroll(requestScroll))
            if (approval.choices.size == 2) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                approval.choices.forEachIndexed { index, choice ->
                    Button(onClick = { choose(index) }, enabled = enabled, modifier = Modifier.weight(1f), shape = RoundedCornerShape(11.dp), colors = ButtonDefaults.buttonColors(containerColor = if (index == 0) Ink else Color.White, contentColor = if (index == 0) Color.White else Ink)) { Text(choiceLabel(choice.label)) }
                }
            } else approval.choices.forEachIndexed { index, choice ->
                Button(onClick = { choose(index) }, enabled = enabled, modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(11.dp), colors = ButtonDefaults.buttonColors(containerColor = if (index == 0) Ink else Color.White, contentColor = if (index == 0) Color.White else Ink)) { Text(choiceLabel(choice.label)) }
            }
        }
    }
}

@Composable private fun choiceLabel(value: String): String = when (value.trim().lowercase()) {
    "yes" -> stringResource(R.string.approval_allow)
    "no" -> stringResource(R.string.approval_deny)
    else -> value
}
