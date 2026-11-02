package io.github.wenshiqi0.kite.data

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.os.Handler

/** Native connectivity facts only. Rust decides whether and when to reconnect. */
internal class ConnectionNetworkMonitor(context: Context, handler: Handler, private val changed: (Boolean) -> Unit) {
    private val manager = context.getSystemService(ConnectivityManager::class.java)
    private val state = DefaultNetworkState<Network>(changed)
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) { state.available(network) }
        override fun onLost(network: Network) { state.lost(network) }
    }
    init {
        refresh()
        manager.registerDefaultNetworkCallback(callback, handler)
    }
    fun refresh() { state.refresh(manager.activeNetwork) }
}
