package io.github.wenshiqi0.kite.data

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withContext
import kotlinx.coroutines.channels.Channel
import io.github.wenshiqi0.kite.ui.ChatTimelineState
import io.github.wenshiqi0.kite.core.AgentNoticeInfo
import java.io.File

/** One process-wide Core and observer; no Activity-owned domain state. */
class CoreController(private val context: Context) {
    private val resourceNotifications = ResourceNotifications(context)
    val agentNotifications=AgentNotifications(context)
    val systemPush=SystemPush(context,this)
    val notificationInbox=MutableStateFlow<List<AgentNoticeInfo>>(emptyList())
    var notificationNavigationActive by androidx.compose.runtime.mutableStateOf(false)
    var stagedNoticeId:String?=null
    @Volatile private var visible=false
    private var noticeRevision:ULong?=null
    private var refreshingNotices=false
    private var refreshNoticesAgain=false
    fun visible(value:Boolean){visible=value;if(value){networkMonitor.refresh();refreshAgentNotifications();systemPush.refresh()}}
    fun stageNotification(id:String){stagedNoticeId=id;notificationNavigationActive=true}
    fun enableAgentNotifications(value:Boolean){agentNotifications.enable(value);systemPush.enabledChanged(value);refreshAgentNotifications()}
    fun refreshAgentNotifications(){
        if(current.value?.initialized!=true)return
        if(refreshingNotices){refreshNoticesAgain=true;return}
        refreshingNotices=true
        command{
          try {
            val notices=withContext(Dispatchers.Default){core.notifications()};notificationInbox.value=notices
            val latest=notices.distinctBy{it.hostId to it.sessionId}.map{it.id}.toSet()
            for(notice in notices) {
                val state=current.value?:break
                val reading=visible&&!notificationNavigationActive&&state.selectedHost==notice.hostId&&state?.selectedSession==notice.sessionId
                if(reading&&!notice.read){core.acknowledgeNotification(notice.id,true,true);agentNotifications.cancel(notice.id)}
                else if(!notice.alerted){
                    if(notice.read||notice.id !in latest||System.currentTimeMillis()-notice.createdMs>3_600_000)core.acknowledgeNotification(notice.id,notice.read,true)
                    else presentNotice(notice)
                }
            }
          } finally { refreshingNotices=false;if(refreshNoticesAgain){refreshNoticesAgain=false;refreshAgentNotifications()} }
        }
    }
    suspend fun presentNotice(notice:AgentNoticeInfo) {
        val state=current.value
        val reading=visible&&!notificationNavigationActive&&state?.selectedHost==notice.hostId&&state?.selectedSession==notice.sessionId
        if(reading||notice.read){core.acknowledgeNotification(notice.id,true,true);agentNotifications.cancel(notice.id)}
        else if(!notice.alerted){
            if(System.currentTimeMillis()-notice.createdMs>3_600_000||agentNotifications.deliver(notice))core.acknowledgeNotification(notice.id,false,true)
        }
    }
    private val audioPreferences=context.getSharedPreferences("desktop-audio",Context.MODE_PRIVATE)
    var desktopAudioEnabled by androidx.compose.runtime.mutableStateOf(audioPreferences.getBoolean("enabled",true))
        private set
    fun setDesktopAudio(enabled:Boolean){desktopAudioEnabled=enabled;audioPreferences.edit().putBoolean("enabled",enabled).apply()}
    private val clipboardPreferences = context.getSharedPreferences("clipboard-bridge", Context.MODE_PRIVATE)
    var clipboardSyncEnabled by androidx.compose.runtime.mutableStateOf(clipboardPreferences.getBoolean("enabled",false))
        private set
    var clipboardProblem by androidx.compose.runtime.mutableStateOf<String?>(null)
    fun setClipboardSync(enabled: Boolean) {clipboardSyncEnabled=enabled;clipboardPreferences.edit().putBoolean("enabled",enabled).apply();clipboardProblem=null}
    private val main = Handler(Looper.getMainLooper())
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val current = MutableStateFlow<CoreSnapshot?>(null)
    val state = current.asStateFlow()
    val platformError = MutableStateFlow<String?>(null)
    val timeline=ChatTimelineState()
    private val conversationRequests=Channel<Unit>(Channel.CONFLATED)
    private var requestedConversation:Pair<String,ULong>?=null
    private var lastRevision: ULong? = null
    private var foregroundWanted = false
    private var foregroundRequested = false
    private var service: ConnectionService? = null
    private var initializing = true

    private val imageDownloads = kotlinx.coroutines.sync.Mutex()
    suspend fun generatedImage(conversation: String, id: String): File = withContext(Dispatchers.IO) {
        require(id.matches(Regex("[a-f0-9]{64}")))
        imageDownloads.lock()
        try {
            val digest = java.security.MessageDigest.getInstance("SHA-256")
            val namespace = digest.digest(conversation.toByteArray()).joinToString("") { "%02x".format(it) }
            val root = File(context.filesDir, "generated-images/$namespace").apply { mkdirs() }
            val file = File(root, id)
            fun valid(): Boolean = file.isFile && file.length() in 1..(25L * 1024 * 1024) &&
                file.inputStream().use { source ->
                    digest.reset()
                    val buffer = ByteArray(65536)
                    while (true) { val count = source.read(buffer); if (count < 0) break; digest.update(buffer, 0, count) }
                    digest.digest().joinToString("") { "%02x".format(it) } == id
                }
            if (!valid()) {
                val bytes = core.chatImage(conversation, id)
                val temporary = File.createTempFile("image-", ".tmp", root)
                try {
                    temporary.outputStream().use { it.write(bytes) }
                    check(temporary.renameTo(file)) { "Could not save image" }
                } finally { temporary.delete() }
            }
            file
        } finally { imageDownloads.unlock() }
    }

    suspend fun messageAttachment(conversation: String, attachment: io.github.wenshiqi0.kite.core.AttachmentInfo): File = withContext(Dispatchers.IO) {
        if (attachment.generatedImage) return@withContext generatedImage(conversation, attachment.id)
        require(attachment.id.matches(Regex("[a-f0-9]{32}")))
        imageDownloads.lock()
        try {
            val digest = java.security.MessageDigest.getInstance("SHA-256")
            fun hash(bytes: ByteArray) = digest.digest(bytes).joinToString("") { "%02x".format(it) }
            val root = File(context.filesDir, "message-attachments/" + hash(conversation.toByteArray())).apply { mkdirs() }
            val file = File(root, attachment.id)
            val checksum = File(root, attachment.id + ".sha256")
            val valid = file.isFile && file.length() == attachment.size.toLong() && file.length() in 1..(25L*1024*1024) && checksum.isFile && hash(file.readBytes()) == checksum.readText()
            if (!valid) {
                val bytes = core.chatAttachment(conversation, attachment.id)
                val temporary = File.createTempFile("attachment-", ".tmp", root)
                try {
                    temporary.writeBytes(bytes)
                    check(temporary.renameTo(file)) { "Could not cache attachment" }
                    checksum.writeText(hash(bytes))
                } finally { temporary.delete() }
            }
            file
        } finally { imageDownloads.unlock() }
    }

    val core = KiteCore(DeviceKeyStore(context), object : CoreObserver {
        override fun stateChanged(snapshot: CoreSnapshot) {
            main.post {
                if (lastRevision == null || snapshot.revision > lastRevision!!) {
                    lastRevision = snapshot.revision
                    timeline.prepareContext(snapshot.conversationContext)
                    val previous=current.value
                    current.value = snapshot
                    resourceNotifications.observe(snapshot, visible)
                    if(snapshot.initialized)systemPush.refresh()
                    if(snapshot.initialized&&(noticeRevision!=snapshot.notificationRevision||previous?.selectedHost!=snapshot.selectedHost||previous?.selectedSession!=snapshot.selectedSession)){noticeRevision=snapshot.notificationRevision;refreshAgentNotifications()}
                    val requested=snapshot.conversationContext to snapshot.conversationRevision
                    if(requestedConversation!=requested){requestedConversation=requested;conversationRequests.trySend(Unit)}
                    refreshForeground(snapshot)
                }
            }
        }
    })

    private val networkMonitor = ConnectionNetworkMonitor(context, main) { available -> core.networkChanged(available) }

    init {
        scope.launch{while(true){kotlinx.coroutines.delay(30_000);systemPush.refresh()}}
        scope.launch {
            for(signal in conversationRequests) {
                val requestContext=timeline.appliedContext
                val requestRevision=timeline.revision
                try {
                    var delta=withContext(Dispatchers.Default){core.conversationSince(requestContext,requestRevision)}
                    if(delta.context!=current.value?.conversationContext)continue
                    try {timeline.apply(delta)}catch(e:IllegalArgumentException) {
                        delta=withContext(Dispatchers.Default){core.conversationSince("",0uL)}
                        if(delta.context==current.value?.conversationContext)timeline.apply(delta)
                    }
                }catch(e:CancellationException){throw e}
                catch(e:Exception){platformError.value=context.getString(R.string.error_action)+"\n"+(e.message?:e.javaClass.simpleName)}
            }
        }
    }

    fun initialize() = command {
        try {
            core.initialize(File(context.filesDir, "core").absolutePath, "kite@${Build.MODEL}", HostStore(context).legacyData())
            core.restoreConnection()
        } finally { main.post { initializing = false; current.value?.let(::refreshForeground) } }
    }

    fun command(action: suspend () -> Unit) {
        scope.launch {
            try { action() } catch(e:CancellationException){throw e} catch (e: Exception) {
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
        foregroundRequested = true
        if (!foregroundWanted && !initializing) instance.stopSelf()
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
        service?.setReconnecting(snapshot.connection == ConnectionStatus.RECONNECTING)
        service?.setVideoActive(snapshot.desktop.status == DesktopStatus.STREAMING || snapshot.desktop.status == DesktopStatus.STARTING)
        if (snapshot.connection == ConnectionStatus.CONNECTING || snapshot.connection == ConnectionStatus.RECONNECTING || snapshot.connection == ConnectionStatus.CONNECTED) {
            ensureForeground()
        } else if (!initializing) {
            foregroundWanted = false
            // Do not cancel a pending startForegroundService before onCreate promotes it.
            service?.stopSelf()
        }
    }
}
