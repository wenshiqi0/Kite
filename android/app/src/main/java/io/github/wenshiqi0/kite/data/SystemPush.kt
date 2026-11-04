package io.github.wenshiqi0.kite.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import io.github.wenshiqi0.kite.core.ConnectionStatus
import kotlinx.coroutines.launch
import java.util.UUID

/** Native SDK lifecycle. Subscription identity and transport registration go through Core. */
class SystemPush(private val context:Context,private val engine:CoreController) {
    private val preferences=context.getSharedPreferences("system-push",Context.MODE_PRIVATE)
    private val deviceId=preferences.getString("device",null)?:UUID.randomUUID().toString().also{preferences.edit().putString("device",it).apply()}
    private var token=preferences.getString("token",null)
    private var registered:String?=null
    private var busy=false
    private var requesting=false
    private var sdkRegistered=false
    private var retryAt=0L
    var status by mutableStateOf(Status.UNCONFIGURED)
        private set
    enum class Status { UNCONFIGURED, NO_PLAY_SERVICES, PERMISSION, REGISTERING, HOST_UNCONFIGURED, READY, RETRY, DISABLED }
    private fun messaging():FirebaseMessaging? {
        val options=FirebaseOptions.fromResource(context)?:return null
        if(FirebaseApp.getApps(context).isEmpty())FirebaseApp.initializeApp(context,options)
        return FirebaseMessaging.getInstance()
    }
    fun tokenChanged(value:String){token=value;preferences.edit().putString("token",value).apply();registered=null;refresh()}
    fun refresh() {
        if(!engine.agentNotifications.enabled){status=Status.DISABLED;return}
        if(!engine.agentNotifications.permissionGranted()){status=Status.PERMISSION;return}
        if(FirebaseOptions.fromResource(context)==null){status=Status.UNCONFIGURED;return}
        if(GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context)!=ConnectionResult.SUCCESS){status=Status.NO_PLAY_SERVICES;return}
        if(System.currentTimeMillis()<retryAt)return
        val service=messaging()?:return
        service.isAutoInitEnabled=true
        if(!sdkRegistered){
            if(requesting)return
            requesting=true;status=Status.REGISTERING
            service.register().addOnCompleteListener{task->
                requesting=false
                if(task.isSuccessful){sdkRegistered=true;refresh()}
                else {status=Status.RETRY;retryAt=System.currentTimeMillis()+30_000}
            }
            return
        }
        if(token==null){status=Status.REGISTERING;return}
        val state=engine.state.value?:return
        val hosts=state.hostConnections.filter{it.connection==ConnectionStatus.CONNECTED}.map{it.hostId}.sorted()
        if(hosts.isEmpty()){registered=null;return}
        val key="$hosts/$token"
        if(registered==key||busy)return
        busy=true;status=Status.REGISTERING
        val sending=token!!
        engine.scope.launch {
            try {
                val configured=engine.core.registerPush(deviceId,sending,FirebaseOptions.fromResource(context)!!.projectId.orEmpty())
                if(engine.state.value?.hostConnections?.filter{it.connection==ConnectionStatus.CONNECTED}?.map{it.hostId}?.sorted()==hosts&&token==sending){registered=if(configured)key else null;status=if(configured)Status.READY else Status.HOST_UNCONFIGURED;if(!configured)retryAt=System.currentTimeMillis()+30_000}
            }catch(e:kotlinx.coroutines.CancellationException){throw e}
            catch(_:Exception){status=Status.RETRY;retryAt=System.currentTimeMillis()+30_000}
            finally{busy=false}
        }
    }
    fun enabledChanged(enabled:Boolean){
        registered=null
        if(!enabled){
            status=Status.DISABLED
            token=null;sdkRegistered=false;preferences.edit().remove("token").apply()
            messaging()?.let{it.isAutoInitEnabled=false;it.unregister()}
            if(engine.state.value?.hostConnections?.any{it.connection==ConnectionStatus.CONNECTED}==true)engine.scope.launch{runCatching{engine.core.registerPush(deviceId,null,"")}}
        }else{retryAt=0;refresh()}
    }
}
