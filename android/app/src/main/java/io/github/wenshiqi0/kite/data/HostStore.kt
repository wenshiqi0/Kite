package io.github.wenshiqi0.kite.data

import android.content.Context
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.core.LegacyData
import io.github.wenshiqi0.kite.core.TrustedHostRecord

/** Unsaved text belongs to the native form; saved hosts belong to Rust. */
data class HostForm(
    val id: String = "",
    val host: String = "",
    val port: String = "22",
    val username: String = "",
    val herdrPath: String = "",
    val streamHost: String = "",
    val sunshinePort: String = "47989",
)

fun HostRecord.toForm() = HostForm(id, host, port.toString(), username, herdrPath.orEmpty(), streamHost.orEmpty(), sunshinePort.toString())

/** Read-only M1 import. Rust's atomic marker makes retries idempotent. */
class HostStore(context: Context) {
    private val prefs = context.getSharedPreferences("hosts", Context.MODE_PRIVATE)
    fun legacyData(): LegacyData {
        val host = prefs.getString("host", "").orEmpty()
        val username = prefs.getString("username", "").orEmpty()
        val saved = if (host.isBlank()) null else HostRecord(
            id = "legacy-m1", host = host,
            port = prefs.getString("port", "22").orEmpty().trim().toUShortOrNull()
                ?: throw IllegalStateException("invalid legacy SSH port"),
            username = username,
            herdrPath = prefs.getString("herdr_path", "").orEmpty().ifBlank { null },
            streamHost = null, sunshinePort = 47989u,
        )
        val keys = prefs.all.filterKeys { it.startsWith("host_key:") }.map { (key, value) ->
            TrustedHostRecord(key.removePrefix("host_key:"), value as? String
                ?: throw IllegalStateException("invalid legacy host key"))
        }
        return LegacyData("android-m1", saved, keys)
    }
}
