package io.github.wenshiqi0.kite.data

/** An old default-network loss must not hide the replacement network. */
internal class DefaultNetworkState<T>(private val changed: (Boolean) -> Unit) {
    private var current: T? = null
    fun available(network: T) { if (current != network) { current = network; changed(true) } }
    fun lost(network: T) { if (current == network) { current = null; changed(false) } }
    fun refresh(network: T?) { current = network; changed(network != null) }
}
