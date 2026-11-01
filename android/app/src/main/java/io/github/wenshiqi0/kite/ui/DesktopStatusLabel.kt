package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.DesktopStatus

/** Idle is disconnected, not an indefinitely pending connection. */
internal fun desktopStatusLabel(status:DesktopStatus)=when(status){
    DesktopStatus.IDLE->R.string.desktop_disconnected
    DesktopStatus.STARTING->R.string.desktop_connecting
    DesktopStatus.STOPPING->R.string.desktop_stopping
    DesktopStatus.FAILED->R.string.desktop_failed
    DesktopStatus.PAIRING->R.string.desktop_pairing
    DesktopStatus.STREAMING->R.string.desktop_live
}
