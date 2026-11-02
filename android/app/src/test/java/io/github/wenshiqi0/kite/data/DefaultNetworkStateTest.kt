package io.github.wenshiqi0.kite.data

import org.junit.Assert.assertEquals
import org.junit.Test

class DefaultNetworkStateTest {
    @Test fun aLateLossFromWifiDoesNotHideTheNewCellularOrVpnNetwork() {
        val events = mutableListOf<Boolean>()
        val state = DefaultNetworkState<String>(events::add)
        state.available("wifi")
        state.available("cellular")
        state.lost("wifi")
        state.available("cellular")
        state.available("vpn")
        state.lost("cellular")
        assertEquals(listOf(true, true, true), events)
        state.lost("vpn")
        assertEquals(listOf(true, true, true, false), events)
    }
    @Test fun returningToTheForegroundRechecksAnUnchangedNetwork() {
        val events = mutableListOf<Boolean>()
        val state = DefaultNetworkState<String>(events::add)
        state.refresh(null)
        state.available("wifi")
        state.refresh("wifi")
        assertEquals(listOf(false, true, true), events)
    }
}
