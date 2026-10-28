package io.github.wenshiqi0.kite.ui

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.HerdrCheck
import io.github.wenshiqi0.kite.core.HostConfig
import io.github.wenshiqi0.kite.core.KiteException
import io.github.wenshiqi0.kite.core.SshKeyPair
import io.github.wenshiqi0.kite.core.checkHerdr
import io.github.wenshiqi0.kite.data.DeviceKeyStore
import io.github.wenshiqi0.kite.data.HostForm
import io.github.wenshiqi0.kite.data.HostStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

sealed interface CheckState {
    data object Idle : CheckState
    data object Running : CheckState
    data class Passed(val check: HerdrCheck) : CheckState

    /** First connection to this host: the user must confirm the fingerprint. */
    data class ConfirmHostKey(val address: String, val fingerprint: String, val hostKey: String) : CheckState

    data class Failed(val message: String, val hostKeyChanged: Boolean = false) : CheckState
}

class HostCheckViewModel(application: Application) : AndroidViewModel(application) {
    private val deviceKeys = DeviceKeyStore(application)
    private val hosts = HostStore(application)

    var deviceKey by mutableStateOf<SshKeyPair?>(null)
        private set
    var deviceKeyError by mutableStateOf<String?>(null)
        private set
    var form by mutableStateOf(hosts.loadForm())
        private set
    var state by mutableStateOf<CheckState>(CheckState.Idle)
        private set

    init {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { deviceKeys.loadOrCreate() } }
                .onSuccess { deviceKey = it }
                .onFailure { deviceKeyError = it.message ?: it.toString() }
        }
    }

    fun updateForm(form: HostForm) {
        this.form = form
    }

    fun check() {
        val key = deviceKey ?: return
        val form = form
        val port = form.port.trim().toIntOrNull()
        when {
            form.host.isBlank() -> return fail(R.string.error_host_required)
            form.username.isBlank() -> return fail(R.string.error_username_required)
            port == null || port !in 1..65535 -> return fail(R.string.error_port_invalid)
        }
        hosts.saveForm(form)
        val config = HostConfig(
            host = form.host.trim(),
            port = port!!.toUShort(),
            username = form.username.trim(),
            herdrPath = form.herdrPath.trim().ifEmpty { null },
        )
        state = CheckState.Running
        viewModelScope.launch {
            state = try {
                CheckState.Passed(checkHerdr(config, key.privateKeyOpenssh, hosts.trustedHostKey(form.address)))
            } catch (e: KiteException) {
                describe(e, form)
            }
        }
    }

    fun trustHostKey(confirm: CheckState.ConfirmHostKey) {
        hosts.trustHostKey(confirm.address, confirm.hostKey)
        check()
    }

    fun dismissHostKey() {
        state = CheckState.Idle
    }

    fun forgetHostKey() {
        hosts.forgetHostKey(form.address)
        state = CheckState.Idle
    }

    private fun describe(e: KiteException, form: HostForm): CheckState = when (e) {
        is KiteException.UnknownHostKey -> CheckState.ConfirmHostKey(form.address, e.fingerprint, e.hostKey)
        is KiteException.HostKeyChanged -> CheckState.Failed(
            text(R.string.error_host_key_changed, e.trustedFingerprint, e.presentedFingerprint),
            hostKeyChanged = true,
        )
        is KiteException.AuthRejected -> CheckState.Failed(text(R.string.error_auth_rejected, e.username))
        is KiteException.HerdrNotFound -> CheckState.Failed(text(R.string.error_herdr_not_found))
        is KiteException.HerdrUnsupported -> CheckState.Failed(text(R.string.error_herdr_unsupported, e.path))
        is KiteException.Connection -> CheckState.Failed(text(R.string.error_connection, e.message))
        is KiteException.InvalidKey -> CheckState.Failed(text(R.string.error_invalid_key, e.message))
        is KiteException.Probe -> CheckState.Failed(text(R.string.error_probe, e.message))
    }

    private fun fail(message: Int) {
        state = CheckState.Failed(text(message))
    }

    private fun text(id: Int, vararg args: Any): String = getApplication<Application>().getString(id, *args)
}
