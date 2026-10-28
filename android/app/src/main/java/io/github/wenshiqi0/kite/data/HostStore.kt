package io.github.wenshiqi0.kite.data

import android.content.Context
import androidx.core.content.edit

/** What the user typed into the host form, kept as text until it is checked. */
data class HostForm(
    val host: String = "",
    val port: String = "22",
    val username: String = "",
    val herdrPath: String = "",
) {
    /** Identifies the host for host-key trust. */
    val address: String get() = "${host.trim().lowercase()}:${port.trim()}"
}

/** The last host the user checked, and the host keys they chose to trust. */
class HostStore(context: Context) {
    private val prefs = context.getSharedPreferences("hosts", Context.MODE_PRIVATE)

    fun loadForm() = HostForm(
        host = prefs.getString("host", "").orEmpty(),
        port = prefs.getString("port", "22").orEmpty(),
        username = prefs.getString("username", "").orEmpty(),
        herdrPath = prefs.getString("herdr_path", "").orEmpty(),
    )

    fun saveForm(form: HostForm) = prefs.edit {
        putString("host", form.host)
        putString("port", form.port)
        putString("username", form.username)
        putString("herdr_path", form.herdrPath)
    }

    fun trustedHostKey(address: String): String? = prefs.getString(hostKeyEntry(address), null)

    fun trustHostKey(address: String, hostKey: String) = prefs.edit {
        putString(hostKeyEntry(address), hostKey)
    }

    fun forgetHostKey(address: String) = prefs.edit { remove(hostKeyEntry(address)) }

    private fun hostKeyEntry(address: String) = "host_key:$address"
}
